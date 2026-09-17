//! Debug cheat core (DB1, capability card "调试面板 + 作弊核心").
//!
//! The whole module is `#[cfg(debug_assertions)]` (see `systems/mod.rs`):
//! release builds contain none of this. Design:
//!   - `DebugAction` = intent. The egui panel (plugins/debug.rs) and the CLI
//!     scenario loader (DB2) only *push* actions into `DebugQueue`.
//!   - `debug_cheat_executor` = the single drain point. It applies every
//!     action and marks the run as debug (`DebugRun`) — a marked run pays no
//!     meta coins and never writes `meta_save.txt` (test-run isolation).
//! Keeping the panel and the effects decoupled lets headless tests drive the
//! exact same cheat path without any egui/window.

use bevy::ecs::change_detection::Mut;
use bevy::ecs::system::SystemParam;
use bevy::ecs::world::CommandQueue;
use bevy::prelude::*;

use crate::components::{Enemy, Projectile, Tower, TowerSlot};
use crate::resources::{
    BaseHp, Boosts, BuildMode, DebugRun, Economy, EnemyDefs, Hand, MetaState, PathInfo, RunRng,
    Selection, ShopOffers, TowerDefs, WaveChoice, WavePhase, WaveSchedule, WaveState,
};
use crate::states::GameState;
use crate::systems::acquisition::{deal_opening_hand_into, refresh_shop_offers};
use crate::systems::enemy::{spawn_enemy, spawn_enemy_at, path_units_per_point};
use crate::systems::pointer::HoverGhost;
use crate::systems::tower::spawn_base_tower;

/// One debug cheat intent. Panel button / CLI / test all speak this.
#[derive(Debug, Clone, Copy)]
pub enum DebugAction {
    GoldAdd(u32),
    GoldSub(u32),
    GoldSet(u32),
    BaseHpSet(u32),
    /// Land in the build phase of wave `n` (1-based), auto-granting the
    /// rewards of every skipped wave so the economy stays real.
    JumpToWave(u32),
    /// Resolve the current wave immediately (reward + intermission happen
    /// through the normal `wave_resolve` path).
    ClearWave,
    SummonEnemy { def_index: usize, count: u32 },
    UnlockAllTowers,
    GrantTower(usize),
    InstantLose,
    InstantWin,
    /// One-click full run reset (fresh economy/hand/wave; replays the CLI
    /// seed when one was given). Keeps the debug mark.
    Restart,
    // --- DB3: time control + overlay toggles. Observation tools, NOT cheats:
    // they never mark the run (see `is_observation`). ---
    /// Toggle the debug (time-level) pause.
    TimePauseToggle,
    /// Toggle the debug 4x fast-forward.
    TimeSpeedToggle4x,
    /// While debug-paused: let exactly one frame run, then re-pause.
    TimeStepFrame,
    /// While debug-paused: let 0.1s of virtual time run, then re-pause.
    TimeStepTenths,
    /// Toggle the all-tower range-ring overlay.
    OverlaysToggleRanges,
    /// Toggle the enemy stat-plate overlay.
    OverlaysToggleEnemyStats,
    /// Reset every skill cooldown (SK1 debug convenience — a CHEAT: marks
    /// the run like every other one).
    SkillResetCd,
}

impl DebugAction {
    /// Observation-only actions (DB3 time control / overlays): using them
    /// must NOT mark the run as debug — freezing time or showing rings does
    /// not change any outcome, so a run inspected with them still pays meta.
    pub fn is_observation(&self) -> bool {
        matches!(
            self,
            DebugAction::TimePauseToggle
                | DebugAction::TimeSpeedToggle4x
                | DebugAction::TimeStepFrame
                | DebugAction::TimeStepTenths
                | DebugAction::OverlaysToggleRanges
                | DebugAction::OverlaysToggleEnemyStats
        )
    }
}

/// Pending cheat intents; drained (emptied) by `debug_cheat_executor`.
#[derive(Resource, Default)]
pub struct DebugQueue(pub Vec<DebugAction>);

/// egui panel scratch state (F1, DB1). The numbers are editable fields; the
/// buttons beside them push the matching `DebugAction`.
#[derive(Resource)]
pub struct DebugPanel {
    pub open: bool,
    pub gold_set: i32,
    pub base_set: i32,
    pub jump_wave: i32,
    pub summon_def: usize,
    pub summon_count: i32,
}

impl Default for DebugPanel {
    fn default() -> Self {
        Self {
            open: false,
            gold_set: 100,
            base_set: 10,
            jump_wave: 1,
            summon_def: 0,
            summon_count: 1,
        }
    }
}

/// The RNG seed given on the CLI (`--seed N`, DB2). `None` = no seed; the
/// debug restart replays the sequence only when a seed exists.
#[derive(Resource, Default)]
pub struct DebugSeed(pub Option<u64>);

/// F1 toggles the debug panel (DB1). Also clears the input-capture flags on
/// close so world input resumes immediately.
pub fn debug_panel_toggle(
    keys: Res<ButtonInput<KeyCode>>,
    mut panel: ResMut<DebugPanel>,
    mut capture: ResMut<crate::resources::DebugPanelCapture>,
) {
    if keys.just_pressed(KeyCode::F1) {
        panel.open = !panel.open;
        if !panel.open {
            capture.pointer = false;
            capture.keyboard = false;
        }
        info!("[debug] panel {}", if panel.open { "open" } else { "closed" });
    }
}

// --- The executor --------------------------------------------------------------
//
// Two `SystemParam` bundles keep the single executor under Bevy's 16-param
// system cap while every action still drains in ONE system (ordering + a
// single debug-mark write).

/// Run-wide resources the cheats touch (economy / wave bookkeeping / state).
#[derive(SystemParam)]
pub struct CheatsCore<'w, 's> {
    pub commands: Commands<'w, 's>,
    pub queue: ResMut<'w, DebugQueue>,
    pub run: ResMut<'w, DebugRun>,
    pub economy: ResMut<'w, Economy>,
    pub base: ResMut<'w, BaseHp>,
    pub wave: ResMut<'w, WaveState>,
    pub schedule: Res<'w, WaveSchedule>,
    pub hand: ResMut<'w, Hand>,
    pub next: ResMut<'w, NextState<GameState>>,
    pub state: Res<'w, State<GameState>>,
}

/// Board-scoped resources (spawning, restart resets, the entities to clear).
#[derive(SystemParam)]
pub struct CheatsBoard<'w, 's> {
    pub defs: Res<'w, EnemyDefs>,
    pub path: Res<'w, PathInfo>,
    pub meshes: ResMut<'w, Assets<Mesh>>,
    pub materials: ResMut<'w, Assets<StandardMaterial>>,
    pub rng: ResMut<'w, RunRng>,
    pub meta: Res<'w, MetaState>,
    pub boosts: ResMut<'w, Boosts>,
    pub shop: ResMut<'w, ShopOffers>,
    pub choice: ResMut<'w, WaveChoice>,
    pub selection: ResMut<'w, Selection>,
    pub build: ResMut<'w, BuildMode>,
    pub seed: Res<'w, DebugSeed>,
    /// DB3 time control + overlay state (observation tools — see the actions).
    pub time: ResMut<'w, crate::systems::debug_time::DebugTime>,
    pub overlays: ResMut<'w, crate::systems::debug_time::DebugOverlays>,
    /// Skill cooldowns (SK1 "reset skill CDs" cheat).
    pub skills: ResMut<'w, crate::resources::SkillState>,
    /// Enemies + in-flight projectiles (cleared on wave jumps / restart).
    pub mobs: Query<'w, 's, Entity, Or<(With<Enemy>, With<Projectile>)>>,
    /// Placed towers (cleared only on restart).
    pub towers: Query<'w, 's, Entity, With<Tower>>,
    /// The build-mode ghost is hidden (not despawned): its owner system keeps
    /// the entity id in a Local cache and would panic on a stale insert.
    pub ghosts: Query<'w, 's, Entity, With<HoverGhost>>,
}

/// Single drain point for every debug action (DB1). Marks the run as debug on
/// the first CHEAT (DB3 observation actions — time control / overlays — are
/// exempt: they change no outcome): no meta payout, no save write for a
/// marked run.
pub fn debug_cheat_executor(mut core: CheatsCore, mut board: CheatsBoard) {
    if core.queue.0.is_empty() {
        return;
    }
    let actions = std::mem::take(&mut core.queue.0);
    if actions.iter().any(|a| !a.is_observation()) && !core.run.marked {
        core.run.marked = true;
        info!("[debug] run marked as debug: meta payout/save disabled");
    }
    for action in actions {
        match action {
            DebugAction::GoldAdd(n) => {
                core.economy.gold = core.economy.gold.saturating_add(n);
                info!("[debug] gold +{n} -> {}", core.economy.gold);
            }
            DebugAction::GoldSub(n) => {
                core.economy.gold = core.economy.gold.saturating_sub(n);
                info!("[debug] gold -{n} -> {}", core.economy.gold);
            }
            DebugAction::GoldSet(n) => {
                core.economy.gold = n;
                info!("[debug] gold = {n}");
            }
            DebugAction::BaseHpSet(n) => {
                if n > core.base.max_hp {
                    core.base.max_hp = n;
                }
                core.base.hp = n;
                info!("[debug] base hp = {n}");
            }
            DebugAction::JumpToWave(n) => jump_to_wave(&mut core, &mut board, n),
            DebugAction::ClearWave => clear_wave(&mut core, &mut board),
            DebugAction::SummonEnemy { def_index, count } => {
                for _ in 0..count {
                    spawn_enemy(
                        &mut core.commands,
                        &board.defs,
                        &board.path,
                        def_index,
                        &mut board.meshes,
                        &mut board.materials,
                    );
                    core.wave.active += 1;
                }
                // Summons force combat so the wave machinery tracks them
                // (and resolves the wave once they are all gone).
                core.wave.phase = WavePhase::Combat;
                info!(
                    "[debug] summoned def {def_index} x{count} at the entry (active={})",
                    core.wave.active
                );
            }
            DebugAction::UnlockAllTowers => {
                core.hand.owned_towers = (0..4).collect();
                info!("[debug] unlocked all tower types");
            }
            DebugAction::GrantTower(t) => {
                if !core.hand.owned_towers.contains(&t) {
                    core.hand.owned_towers.push(t);
                }
                info!("[debug] granted tower type {t} (hand {:?})", core.hand.owned_towers);
            }
            DebugAction::InstantLose => {
                core.base.hp = 0;
                if *core.state.get() == GameState::Playing {
                    core.next.set(GameState::GameOver);
                }
                info!("[debug] instant lose");
            }
            DebugAction::InstantWin => {
                if *core.state.get() == GameState::Playing {
                    core.next.set(GameState::Win);
                }
                info!("[debug] instant win");
            }
            DebugAction::Restart => apply_restart(&mut core, &mut board),
            // --- DB3: time control + overlays (observation, no marking) ---
            DebugAction::TimePauseToggle => {
                board.time.paused = !board.time.paused;
                board.time.step = crate::systems::debug_time::DebugStep::None;
                info!(
                    "[db3] debug pause {}",
                    if board.time.paused { "on" } else { "off" }
                );
            }
            DebugAction::TimeSpeedToggle4x => {
                board.time.speed_4x = !board.time.speed_4x;
                info!(
                    "[db3] debug speed {}",
                    if board.time.speed_4x { "4x" } else { "1x" }
                );
            }
            DebugAction::TimeStepFrame => {
                if board.time.paused {
                    board.time.step = crate::systems::debug_time::DebugStep::Frames(1);
                    info!("[db3] step: one frame");
                }
            }
            DebugAction::TimeStepTenths => {
                if board.time.paused {
                    board.time.step = crate::systems::debug_time::DebugStep::Secs(0.1);
                    info!("[db3] step: 0.1s");
                }
            }
            DebugAction::OverlaysToggleRanges => {
                board.overlays.show_ranges = !board.overlays.show_ranges;
            }
            DebugAction::OverlaysToggleEnemyStats => {
                board.overlays.show_enemy_stats = !board.overlays.show_enemy_stats;
            }
            DebugAction::SkillResetCd => {
                board.skills.reset_all();
                info!("[debug] skill cooldowns reset");
            }
        }
    }
}

/// Jump to the build phase of wave `n` (1-based). Rewards of every skipped
/// wave are auto-granted (economy stays real); skipping backwards grants
/// nothing (the already-paid rewards are simply kept). Live enemies and
/// projectiles are cleared; skipped 三选一 offers are dismissed (they are
/// skipped, not banked — deterministic for seed replay).
fn jump_to_wave(core: &mut CheatsCore, board: &mut CheatsBoard, n: u32) {
    let len = core.schedule.waves.len().max(1) as u32;
    let target = n.clamp(1, len) as usize; // 1-based wave number
    let target_idx = target - 1;
    let start_idx = core.wave.current as usize; // 0-based index of the current wave
    let mut granted = 0;
    if target_idx > start_idx {
        for w in &core.schedule.waves[start_idx..target_idx] {
            granted += w.reward;
        }
        core.economy.gold += granted;
    }
    for e in &board.mobs {
        core.commands.entity(e).despawn();
    }
    hide_ghosts(core, board);
    core.wave.current = target_idx as u32;
    core.wave.phase = WavePhase::Intermission;
    core.wave.spawn_queue.clear();
    core.wave.active = 0;
    core.wave.spawn_timer = 0.0;
    board.choice.pending = false;
    *board.selection = Selection::None;
    board.build.armed = false;
    info!(
        "[debug] jumped to wave {target} build phase (skipped rewards +{granted}, gold={})",
        core.economy.gold
    );
}

/// Clear the current wave: despawn everything live, drain the spawn queue and
/// zero the active counter. The wave then settles through the normal
/// `wave_resolve` (reward + intermission + 三选一) in this frame's Cleanup.
fn clear_wave(core: &mut CheatsCore, board: &mut CheatsBoard) {
    for e in &board.mobs {
        core.commands.entity(e).despawn();
    }
    core.wave.spawn_queue.clear();
    core.wave.active = 0;
    info!("[debug] current wave force-cleared (settles via wave_resolve)");
}

/// Full run reset (DB1 "快速重开"): fresh economy (meta upgrade 2 re-applied),
/// base hp, wave, boosts, hand (re-dealt via the real AC1 logic, upgrade 1
/// included), shop offer, selection/build mode; board entities despawned;
/// state back to Playing. With a CLI seed the RNG is re-seeded so the run
/// replays the exact same sequence. The debug mark intentionally survives —
/// only a fresh app launch gives a clean run again.
fn apply_restart(core: &mut CheatsCore, board: &mut CheatsBoard) {
    for e in &board.mobs {
        core.commands.entity(e).despawn();
    }
    for e in &board.towers {
        core.commands.entity(e).despawn();
    }
    hide_ghosts(core, board);

    *core.economy = Economy::default();
    if board.meta.upgrade2 {
        core.economy.gold += 20; // same run-start effect as apply_meta_on_run_start
    }
    *core.base = BaseHp::default();
    *core.wave = WaveState::default();
    *board.boosts = Boosts::default();
    *board.choice = WaveChoice::default();
    *board.selection = Selection::None;
    board.build.armed = false;

    if let Some(s) = board.seed.0 {
        *board.rng = RunRng::seeded(s);
    }
    core.hand.owned_towers.clear();
    deal_opening_hand_into(&mut core.hand, &mut board.rng, &board.meta);
    refresh_shop_offers(&mut board.shop, &core.hand, &mut board.rng);

    core.next.set(GameState::Playing);
    info!(
        "[debug] run restarted (gold={}, hand={:?}); debug mark kept",
        core.economy.gold, core.hand.owned_towers
    );
}

/// Hide (not despawn — see `CheatsBoard::ghosts`) any build-mode ghost.
fn hide_ghosts(core: &mut CheatsCore, board: &mut CheatsBoard) {
    for e in &board.ghosts {
        core.commands.entity(e).insert(Visibility::Hidden);
    }
}

// --- DB2: one-shot test scenarios ----------------------------------------------
//
// A scenario lands the game in a preset state in a single step (no waiting for
// waves/economy). Two equivalent entrances:
//   - CLI: `warden --scenario NAME --seed N` (parsed in `main`, applied via
//     the same queue below — the queue is drained on the FIRST Update frame,
//     after Startup built the map/slots the scenarios place things on);
//   - F1 panel: the scenario buttons push into the same queue.
// Loading ANY scenario marks the run as debug (DB1 save isolation).

/// The six preset scenarios (DB2). `name` doubles as the CLI value and the
/// panel button label.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scenario {
    /// Two archers on the board + 100 gold + build phase (fusion testing).
    Fusion,
    /// One tower + one ordinary enemy exactly 8 units away (projectile tests).
    Projectile,
    /// A wounded (1 hp) enemy pack plus a healer nearby (FX tests).
    Fx,
    /// Straight to the GameOver screen (meta/UI5 tests).
    Meta,
    /// A pending three-choose-one with real randomized options (AC4 tests).
    Choice,
    /// All 8 slots occupied (full-board tests).
    FullBoard,
}

impl Scenario {
    /// All scenarios with their CLI/panel names, in display order.
    pub const ALL: [(Scenario, &'static str); 6] = [
        (Scenario::Fusion, "fusion"),
        (Scenario::Projectile, "projectile"),
        (Scenario::Fx, "fx"),
        (Scenario::Meta, "meta"),
        (Scenario::Choice, "choice"),
        (Scenario::FullBoard, "full-board"),
    ];

    pub fn name(self) -> &'static str {
        Self::ALL
            .iter()
            .find(|(s, _)| *s == self)
            .map(|(_, n)| *n)
            .unwrap_or("?")
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL
            .iter()
            .find(|(_, n)| *n == name)
            .map(|(s, _)| *s)
    }
}

/// Pending scenario loads, drained by `debug_scenario_executor`. CLI and F1
/// panel both only push here — one queue, one code path.
#[derive(Resource, Default)]
pub struct DebugScenarioQueue(pub Vec<Scenario>);

/// Parsed `--scenario NAME --seed N` CLI overrides (DB2).
#[derive(Debug, Default, PartialEq, Eq)]
pub struct CliOverrides {
    pub scenario: Option<Scenario>,
    pub seed: Option<u64>,
}

/// Parse the overrides from an args iterator — `std::env::args` in `main`,
/// a fixed vector in tests. Returns a usage error for unknown scenario names
/// or malformed numbers.
pub fn parse_cli_args(args: impl IntoIterator<Item = String>) -> Result<CliOverrides, String> {
    let valid = Scenario::ALL
        .iter()
        .map(|(_, n)| *n)
        .collect::<Vec<_>>()
        .join(", ");
    let usage = format!("usage: warden [--scenario NAME] [--seed N]  (scenarios: {valid})");
    let mut out = CliOverrides::default();
    let mut it = args.into_iter().peekable();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--scenario" => {
                let Some(name) = it.next() else {
                    return Err(format!("--scenario needs a value ({valid})"));
                };
                out.scenario = Some(Scenario::from_name(&name).ok_or_else(|| {
                    format!("unknown scenario '{name}' ({valid})")
                })?);
            }
            "--seed" => {
                let Some(raw) = it.next() else {
                    return Err("--seed needs a number".to_string());
                };
                out.seed = Some(raw.parse::<u64>().map_err(|_| format!("bad seed '{raw}'"))?);
            }
            other => return Err(format!("unknown argument '{other}'\n{usage}")),
        }
    }
    Ok(out)
}

/// `main` entry (debug builds): parse the real CLI and wire the overrides in.
/// The seed replaces `RunRng` BEFORE Startup so the opening hand is replayable;
/// the scenario is queued for the first Update frame (after the map exists).
/// Invalid args abort with the usage text — this is a dev tool, fail loud.
pub fn apply_cli_overrides(app: &mut App) {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let overrides = match parse_cli_args(args) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("[debug] {e}");
            std::process::exit(2);
        }
    };
    if let Some(seed) = overrides.seed {
        app.insert_resource(RunRng::seeded(seed));
        app.init_resource::<DebugSeed>();
        app.world_mut().resource_mut::<DebugSeed>().0 = Some(seed);
        info!("[debug] seed {seed} applied (RunRng re-seeded before Startup)");
    }
    if let Some(s) = overrides.scenario {
        app.init_resource::<DebugScenarioQueue>();
        app.world_mut()
            .resource_mut::<DebugScenarioQueue>()
            .0
            .push(s);
        info!("[debug] scenario '{}' queued (loads on the first frame)", s.name());
    }
}

/// Exclusive drain point for scenario loads (DB2), after the cheat executor
/// within `GameSet::Input`. Marks the run as debug on first use, then applies
/// every queued scenario.
pub fn debug_scenario_executor(world: &mut World) {
    let scenarios = std::mem::take(&mut world.resource_mut::<DebugScenarioQueue>().0);
    if scenarios.is_empty() {
        return;
    }
    {
        let mut run = world.resource_mut::<DebugRun>();
        if !run.marked {
            run.marked = true;
            info!("[debug] run marked as debug: meta payout/save disabled");
        }
    }
    for s in scenarios {
        apply_scenario(world, s);
    }
}

/// Land the game in the scenario's preset state. Every scenario starts from
/// the same clean slate: no enemies/projectiles/towers, all slots free, wave
/// back to the wave-1 build phase, no pending choice/selection/build arm.
fn apply_scenario(world: &mut World, scenario: Scenario) {
    clear_board(world);
    match scenario {
        Scenario::Fusion => {
            world.resource_mut::<Economy>().gold = 100;
            let slots = free_slots_in_order(world).into_iter().take(2).collect::<Vec<_>>();
            place_towers(world, 0, &slots);
        }
        Scenario::Projectile => {
            let (slot, tower_pos) = free_slots_in_order(world)
                .first()
                .copied()
                .expect("the map has tower slots");
            place_towers(world, 0, &[(slot, tower_pos)]);
            // Exactly 8 units from the tower on the XZ plane, heading for the
            // base waypoint so it walks like a real target.
            let enemy_pos = tower_pos + Vec3::new(8.0, 0.0, 0.0);
            let next_wp = world.resource::<PathInfo>().waypoints.len().saturating_sub(1);
            spawn_debug_enemy(world, 0, enemy_pos, next_wp, None);
        }
        Scenario::Fx => {
            // Four ordinary enemies at 1 hp walking the middle segment, one
            // healer among them at full hp (heal aura radius 6 covers all).
            for (x, z) in [(-4.0, 3.0), (-4.0, 0.5), (-4.0, -2.0), (-4.0, -4.5)] {
                spawn_debug_enemy(world, 0, Vec3::new(x, 0.0, z), 2, Some(1.0));
            }
            spawn_debug_enemy(world, 3, Vec3::new(-4.0, 0.0, -1.2), 2, None);
        }
        Scenario::Meta => {
            world.resource_mut::<BaseHp>().hp = 0;
            world
                .resource_mut::<NextState<GameState>>()
                .set(GameState::GameOver);
        }
        Scenario::Choice => {
            let defs = TowerDefs {
                list: world.resource::<TowerDefs>().list.clone(),
            };
            let hand = Hand {
                owned_towers: world.resource::<Hand>().owned_towers.clone(),
            };
            world.resource_scope(|world, mut choice: Mut<WaveChoice>| {
                world.resource_scope(|_world, mut rng: Mut<RunRng>| {
                    crate::systems::wave::generate_choices(&mut choice, &defs, &hand, &mut rng);
                });
            });
        }
        Scenario::FullBoard => {
            let slots = free_slots_in_order(world);
            for (i, slot) in slots.iter().enumerate() {
                place_towers(world, i % 4, &[*slot]); // cycle the 4 base types
            }
        }
    }
    info!("[debug] scenario '{}' loaded", scenario.name());
}

/// Wipe the board and interaction state (the common scenario prelude).
/// Ghosts are hidden, not despawned — `update_hover_ghost` caches the entity
/// id in a Local and must keep finding it.
fn clear_board(world: &mut World) {
    let doomed: Vec<Entity> = {
        let mut q = world.query_filtered::<Entity, Or<(With<Enemy>, With<Projectile>, With<Tower>)>>();
        q.iter(world).collect()
    };
    for e in doomed {
        world.despawn(e);
    }
    let ghosts: Vec<Entity> = {
        let mut q = world.query_filtered::<Entity, With<HoverGhost>>();
        q.iter(world).collect()
    };
    for e in ghosts {
        if let Some(mut vis) = world.get_mut::<Visibility>(e) {
            *vis = Visibility::Hidden;
        }
    }
    let slots: Vec<Entity> = {
        let mut q = world.query_filtered::<Entity, With<TowerSlot>>();
        q.iter(world).collect()
    };
    for e in slots {
        if let Some(mut slot) = world.get_mut::<TowerSlot>(e) {
            slot.occupied = false;
        }
    }
    *world.resource_mut::<WaveState>() = WaveState::default();
    world.resource_mut::<WaveChoice>().pending = false;
    *world.resource_mut::<Selection>() = Selection::None;
    world.resource_mut::<BuildMode>().armed = false;
}

/// Free tower slots in map order, as (slot entity, world position).
fn free_slots_in_order(world: &mut World) -> Vec<(Entity, Vec3)> {
    let mut q = world.query::<(Entity, &TowerSlot, &Transform)>();
    let mut slots: Vec<_> = q
        .iter(world)
        .filter(|(_, slot, _)| !slot.occupied)
        .map(|(e, slot, tf)| (slot.index, e, tf.translation))
        .collect();
    slots.sort_by_key(|(index, _, _)| *index);
    slots.into_iter().map(|(_, e, pos)| (e, pos)).collect()
}

/// Spawn base towers of type `ty` at the given free slots and mark the slots
/// occupied (DB2 scenarios; same construction as a player placement).
fn place_towers(world: &mut World, ty: usize, slots: &[(Entity, Vec3)]) {
    let Some(def) = world.resource::<TowerDefs>().list.get(ty).cloned() else {
        warn!("[debug] tower def {ty} missing; scenario towers skipped");
        return;
    };
    let mut queue = CommandQueue::default();
    world.resource_scope(|world, mut meshes: Mut<Assets<Mesh>>| {
        world.resource_scope(|world, mut materials: Mut<Assets<StandardMaterial>>| {
            let server = world
                .get_resource::<AssetServer>()
                .map(|s| s.clone());
            let mut cmds = Commands::new(&mut queue, world);
            for (_slot_e, pos) in slots {
                spawn_base_tower(
                    &mut cmds,
                    &def,
                    ty,
                    *pos,
                    &mut meshes,
                    &mut materials,
                    server.as_ref(),
                );
            }
        });
    });
    // Slot flags flip only after the Commands borrow of the world is over.
    for (slot_e, _) in slots {
        if let Some(mut slot) = world.get_mut::<TowerSlot>(*slot_e) {
            slot.occupied = true;
        }
    }
    queue.apply(world);
}

/// Spawn one enemy for a scenario (DB2): def index, explicit position and
/// waypoint to head toward, optional HP override (wounded-pack setups).
fn spawn_debug_enemy(
    world: &mut World,
    def_index: usize,
    pos: Vec3,
    next_wp: usize,
    hp_override: Option<f32>,
) {
    let Some(def) = world.resource::<EnemyDefs>().list.get(def_index).cloned() else {
        warn!("[debug] enemy def {def_index} missing; scenario enemy skipped");
        return;
    };
    let units = path_units_per_point(world.resource::<PathInfo>());
    let mut queue = CommandQueue::default();
    world.resource_scope(|world, mut meshes: Mut<Assets<Mesh>>| {
        world.resource_scope(|world, mut materials: Mut<Assets<StandardMaterial>>| {
            let mut cmds = Commands::new(&mut queue, world);
            spawn_enemy_at(
                &mut cmds,
                &def,
                def_index,
                pos,
                next_wp,
                hp_override,
                units,
                &mut meshes,
                &mut materials,
            );
        });
    });
    queue.apply(world);
}
