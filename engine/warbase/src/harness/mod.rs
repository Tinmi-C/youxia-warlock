//! Headless harness (BX10): drives a REAL match from a bundle — same
//! systems, same messages, seeded RNG — and returns a structured report.
//! BAL anchors consume this; drivers are the generalized form of warden's
//! BAL2 strategy. Reproducibility rule: same seed => identical report.

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;

use crate::build::{try_place, try_sell, try_upgrade, BuildGrid, Cell};
use crate::bundle::{assemble, build_grid, MapBundle};
use crate::combat::DamageDealt;
use crate::econ::{Population, Resources};
use crate::path::PathFollower;
use crate::rule::{LeakPolicy, MatchFlow, MatchRules, StockCount};
use crate::unit::{spawn_unit, Allegiance, UnitInstance};
use crate::wave::{DirectorPhase, WaveCleared, WaveDirector};

/// What the driver may see each frame (read-only snapshot).
pub struct HarnessView {
    pub frame: u64,
    pub gold: u32,
    pub wood: u32,
    pub pop_used: u8,
    pub pop_cap: u8,
    pub stock: u32,
    pub stock_cap: Option<u32>,
    /// Next/running wave, 1-based.
    pub wave: usize,
    pub spawning: bool,
    pub enemy_count: usize,
    /// Free build cells (bounded, deterministic order).
    pub free_cells: Vec<Cell>,
}

#[derive(Debug, Clone)]
pub enum DriverAction {
    Place { def: String, cell: Cell },
    Sell { cell: Cell },
    /// Raise the buildable at `cell` one tier (UnitDef.upgrade_to chain).
    Upgrade { cell: Cell },
    /// Start the next wave now (Manual advance) / early-call it (Auto, and
    /// the table's early-call bonus applies).
    CallWave,
    /// Cast `skill` on the first caster that owns it (cooldown/aiming
    /// validation is the base's cast pipeline's job, as with real input).
    Cast { skill: String },
    /// Spend one banked skill point on `skill` (XpHolder heroes only).
    SpendPoint { skill: String },
}

/// Execute one driver action against the world. Returns whether it took
/// effect (placed/upgraded/spent/cast) — the report counts on this. Shared
/// by run_headless's per-frame loop and direct tests.
pub fn apply_driver_action(world: &mut World, action: &DriverAction) -> bool {
    match action {
        DriverAction::Place { def, cell } => try_place(world, def, *cell).is_ok(),
        DriverAction::Sell { cell } => try_sell(world, *cell).is_ok(),
        DriverAction::Upgrade { cell } => try_upgrade(world, *cell).is_ok(),
        DriverAction::CallWave => {
            world
                .resource_mut::<Messages<crate::wave::StartWave>>()
                .write(crate::wave::StartWave);
            true
        }
        DriverAction::Cast { skill } => {
            let found = {
                let mut q = world.query::<(Entity, &crate::skill::SkillCaster)>();
                q.iter(world).find_map(|(e, c)| {
                    c.slots.iter().position(|s| &s.skill == skill).map(|slot| (e, slot))
                })
            };
            match found {
                Some((caster, slot)) => {
                    world
                        .resource_mut::<Messages<crate::skill::CastSkill>>()
                        .write(crate::skill::CastSkill { caster, slot });
                    true
                }
                None => false,
            }
        }
        DriverAction::SpendPoint { skill } => {
            let found = {
                let mut q =
                    world.query_filtered::<(Entity, &crate::skill::SkillCaster), With<crate::unit::XpHolder>>();
                q.iter(world).find_map(|(e, c)| {
                    c.slots.iter().position(|s| &s.skill == skill).map(|slot| (e, slot))
                })
            };
            match found {
                Some((entity, slot)) => {
                    crate::skill::spend_skill_point(world, entity, slot).is_ok()
                }
                None => false,
            }
        }
    }
}

/// BAL2's driver generalized: decide placements/sells per frame.
pub trait DriverStrategy {
    fn decide(&mut self, view: &HarnessView) -> Vec<DriverAction>;
}

/// Simplest honest driver: place `tower` at the first free cell whenever
/// asked (try_place itself rejects when broke/full) — the zero-strategy
/// baseline every anchor compares against.
pub struct GreedyDriver {
    pub tower: String,
}

impl DriverStrategy for GreedyDriver {
    fn decide(&mut self, view: &HarnessView) -> Vec<DriverAction> {
        match view.free_cells.first() {
            Some(cell) => vec![DriverAction::Place { def: self.tower.clone(), cell: *cell }],
            None => Vec::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct HarnessOpts {
    pub max_frames: u64,
    pub seed: u64,
    /// Inject leaks at match start: (unit id, count) spawned on path 0.
    /// This is how anchor A2 ("forced leak, must be cleared") drives its
    /// scenario.
    pub inject: Option<(String, u32)>,
}

impl Default for HarnessOpts {
    fn default() -> Self {
        Self { max_frames: 60 * 120, seed: 1, inject: None }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct HarnessReport {
    /// Ok(reason) = won, Err(reason) = lost, Ok("timeout") = frames ran out.
    pub outcome: Result<String, String>,
    pub frames: u64,
    pub waves_cleared: usize,
    pub stock_peak: u32,
    pub towers_built: u32,
    pub gold_left: u32,
    /// Damage attribution by unit def (BAL's per-source readings).
    pub damage_by_def: std::collections::BTreeMap<String, f32>,
    /// Damage attribution by origin channel — attack / skill / system (the
    /// warden BAL three-column share readout, base-side).
    pub damage_by_origin: std::collections::BTreeMap<String, f32>,
    /// Clear time per wave, game seconds (paired with WaveStarted/WaveCleared).
    pub wave_times: Vec<f32>,
    /// Stock gained per cleared wave (leaks of that wave).
    pub leaks_per_wave: Vec<u32>,
    /// Casts issued per skill id through DriverAction::Cast.
    pub skills_cast: std::collections::BTreeMap<String, u32>,
    /// End-of-run slot levels by skill id (point-spending outcome).
    pub skill_levels: std::collections::BTreeMap<String, u8>,
}

/// Run a full headless match. Deterministic for a given (bundle, driver,
/// opts.seed) triple.
pub fn run_headless(
    bundle: &MapBundle,
    driver: &mut dyn DriverStrategy,
    opts: HarnessOpts,
) -> HarnessReport {
    let mut app = crate::testing::test_app();
    app.insert_resource(crate::rng::MatchRng::seeded(opts.seed));
    assemble(&mut app.world_mut(), bundle);
    app.insert_resource(build_grid(&bundle.world));

    // Injection: spawn extra units on path 0 before frame 1.
    if let Some((unit, count)) = &opts.inject {
        let registry: crate::unit::UnitRegistry =
            (*app.world().resource::<crate::unit::UnitRegistry>()).clone();
        let path_e = app.world().resource::<crate::wave::Paths>().0[0];
        for _ in 0..*count {
            if let Some(e) = spawn_unit(
                &mut app.world_mut(),
                &registry,
                unit,
                Vec3::ZERO,
                Allegiance::Attacker,
                1.0,
            ) {
                app.world_mut().entity_mut(e).insert(PathFollower {
                    path: path_e,
                    progress: 0.0,
                    laps: 0,
                    finished: false,
                });
            }
        }
    }

    let mut dmg_cursor = MessageCursor::<DamageDealt>::default();
    let mut wave_cursor = MessageCursor::<WaveCleared>::default();
    let mut started_cursor = MessageCursor::<crate::wave::WaveStarted>::default();
    let mut damage_by_def = std::collections::BTreeMap::new();
    let mut damage_by_origin = std::collections::BTreeMap::new();
    let mut skills_cast = std::collections::BTreeMap::new();
    let mut wave_times = Vec::new();
    let mut leaks_per_wave = Vec::new();
    let mut wave_started_secs: Option<f32> = None;
    let mut last_stock = 0u32;
    let mut stock_peak = 0u32;
    let mut waves_cleared = 0usize;
    let mut towers_built = 0u32;
    let mut frames = 0u64;

    loop {
        // Driver acts before the frame (the same hook real input uses).
        let view = snapshot_view(&mut app, frames);
        for action in driver.decide(&view) {
            let ok = apply_driver_action(&mut app.world_mut(), &action);
            if ok {
                if let DriverAction::Place { .. } = action {
                    towers_built += 1;
                }
                if let DriverAction::Cast { skill } = &action {
                    *skills_cast.entry(skill.clone()).or_default() += 1;
                }
            }
        }

        app.update();
        frames += 1;

        // Per-frame drains (double-buffer pitfall).
        {
            let world = app.world();
            let now = world.resource::<Time>().elapsed_secs();
            for _ in started_cursor.read(world.resource::<Messages<crate::wave::WaveStarted>>()) {
                wave_started_secs = Some(now);
            }
            let msgs = world.resource::<Messages<DamageDealt>>();
            for d in dmg_cursor.read(msgs) {
                let origin_key = match d.source {
                    crate::combat::DamageOrigin::Attack(_) => "attack",
                    crate::combat::DamageOrigin::Skill { .. } => "skill",
                    crate::combat::DamageOrigin::System => "system",
                };
                *damage_by_origin.entry(origin_key.into()).or_default() += d.amount;
                let source_entity = match d.source {
                    crate::combat::DamageOrigin::Attack(e) => e,
                    crate::combat::DamageOrigin::Skill { caster, .. } => caster,
                    crate::combat::DamageOrigin::System => {
                        *damage_by_def.entry("system".into()).or_default() += d.amount;
                        continue;
                    }
                };
                let def = world
                    .get::<UnitInstance>(source_entity)
                    .map(|i| i.def.clone())
                    .unwrap_or_else(|| "unknown".into());
                *damage_by_def.entry(def).or_default() += d.amount;
            }
            let cleared = world.resource::<Messages<WaveCleared>>();
            for w in wave_cursor.read(cleared) {
                waves_cleared = waves_cleared.max(w.0 + 1);
                wave_times.push(wave_started_secs.map(|s| (now - s).max(0.0)).unwrap_or(0.0));
                let stock = world.resource::<StockCount>().0;
                leaks_per_wave.push(stock.saturating_sub(last_stock));
                last_stock = stock;
            }
            stock_peak = stock_peak.max(world.resource::<StockCount>().0);
        }

        if *app.world().resource::<MatchFlow>() != MatchFlow::Running {
            break;
        }
        if frames >= opts.max_frames {
            break;
        }
    }

    // End-of-run skill levels (point-spending readout).
    let mut skill_levels = std::collections::BTreeMap::new();
    {
        let world = app.world_mut();
        let mut q = world.query::<&crate::skill::SkillCaster>();
        for caster in q.iter(world) {
            for slot in &caster.slots {
                skill_levels
                    .entry(slot.skill.clone())
                    .and_modify(|l: &mut u8| *l = (*l).max(slot.level))
                    .or_insert(slot.level);
            }
        }
    }

    let outcome = match app.world().resource::<MatchFlow>() {
        MatchFlow::Won(r) => Ok(r.clone()),
        MatchFlow::Lost(r) => Err(r.clone()),
        MatchFlow::Running => Ok("timeout".into()),
    };
    HarnessReport {
        outcome,
        frames,
        waves_cleared,
        stock_peak,
        towers_built,
        gold_left: app.world().resource::<Resources>().gold,
        damage_by_def,
        damage_by_origin,
        wave_times,
        leaks_per_wave,
        skills_cast,
        skill_levels,
    }
}

fn snapshot_view(app: &mut App, frame: u64) -> HarnessView {
    let enemy_count = app
        .world_mut()
        .query_filtered::<(), With<PathFollower>>()
        .iter(app.world())
        .count();
    let world = app.world();
    let res = world.resource::<Resources>();
    let pop = world.resource::<Population>();
    let stock = world.resource::<StockCount>().0;
    let rules = world.resource::<MatchRules>();
    let director = world.resource::<WaveDirector>();
    let grid = world.resource::<BuildGrid>();
    let mut free_cells = Vec::new();
    'cells: for y in 0..grid.rows {
        for x in 0..grid.cols {
            let c = Cell(x, y);
            if grid.check(c).is_ok() {
                free_cells.push(c);
                if free_cells.len() >= 32 {
                    break 'cells;
                }
            }
        }
    }
    let _ = enemy_count; // moved into the struct below
    HarnessView {
        frame,
        gold: res.gold,
        wood: res.wood,
        pop_used: pop.used,
        pop_cap: pop.cap,
        stock,
        stock_cap: match &rules.leak {
            LeakPolicy::Stockpile { cap } => Some(*cap),
            _ => None,
        },
        wave: director.running.map(|w| w + 1).unwrap_or(director.next_wave + 1),
        spawning: matches!(director.phase, DirectorPhase::Spawning),
        enemy_count,
        free_cells,
    }
}
