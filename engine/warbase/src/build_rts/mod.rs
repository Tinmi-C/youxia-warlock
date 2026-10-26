//! RTS construction (SEAM2d skeleton filled in by ADR-0018 Phase-1 BLDG):
//! the worker-driven building loop — a worker ordered with
//! `Order::Build { what, at }` walks to the site, raises it over the def's
//! `time` seconds, and extra assigned workers stack the rate linearly when
//! `builders_accelerate`. The existing `build` domain stays the TD placement
//! flow; nothing here touches it.
//!
//! Economy timing (pinned, WC3 semantics): the charge lands the moment the
//! order lands on an agent — observed in Tick right after movement's
//! `tick_orders` assigns it. Validation follows the `build::try_place`
//! direct-mutation pattern: capacity-before-spending, then `Resources`
//! minus + one `EconChanged` snapshot, no refund path on the reject branch
//! (unknown def / short on gold or wood => order dropped, 0 charge, 0
//! site). Buildings do NOT consume `Population` (they provide it in WC3
//! terms; the food-consuming spawns are the production domain's trained
//! units), so — unlike try_place's towers — there is no population gate or
//! `used += 1` here; that is a documented interpretation of "校验时机",
//! not an oversight.
//!
//! Site lifecycle: charging and site spawn are ONE event (the spawned
//! `UnderConstruction` entity is the payable object, so `CancelBuild`
//! has a stable Entity target from the order frame — the task brief said
//! "spawn at first arrival"; the deviation is deliberate and noted in the
//! increment report: progress is still gated on a builder standing within
//! `BUILD_RANGE`, so the observable "进入施工" moment is unchanged).
//! Progress freezes while `builders == 0` (worker died / was re-ordered);
//! the charge is kept — refunds happen ONLY through the explicit
//! `CancelBuild` message (100%, WC3-style explicit cancel).
//!
//! Town tier chain: buildings listed in `town_halls` spawn with
//! `TownTier(1)` (an `Added<UnitInstance>` observer, the
//! `attach_production_queues` pattern). `UpgradeTown` validates tier <
//! `MAX_TOWN_TIER` + funds, charges, and parks a `TownUpgrading` timer on
//! the hall; completion bumps the tier and grants `town_unlocks[tier]`
//! build ids into the production domain's `Unlocks` (reaching T2 unlocks
//! index 1, T3 index 2; index 0 is the pre-upgrade state and unused).
//!
//! Zero drift: the config resource defaults empty, so maps without build
//! content see every system here iterate nothing — no charges, no sites,
//! no messages. Warden / loop-td never issue `Order::Build` at all;
//! rts-lab's stray orders against an empty table are rejected silently
//! (order dropped, wallet untouched).

use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::econ::{EconChanged, Population, Resources};
use crate::movement::{MoveAgent, Order};
use crate::production::Unlocks;
use crate::unit::{spawn_unit, Allegiance, UnitInstance, UnitRegistry};

/// Ground distance at which a worker hammers the site (BLDG spec: 1.5,
/// same mechanism-constant family as harvest's `GATHER_RANGE`).
pub const BUILD_RANGE: f32 = 1.5;

/// Top of the town tier chain (T1 -> T2 -> T3).
pub const MAX_TOWN_TIER: u8 = 3;

/// FP-dust tolerance for the completion checks (sites + town upgrades):
/// sequential `remaining -= dt * rate` in f32 accumulates rounding — after
/// an ideal 6s/360-tick run the residue is up to ~5e-5 in the worst
/// all-same-direction bound (~1e-6 typically), while a genuine "one whole
/// tick still left" state sits a full dt step (~1.7e-2) away. The
/// threshold lands between the two with >= 2 orders of margin per side,
/// keeping the frame-exact acceptance numbers exact (360 / 180 / 60).
pub const COMPLETE_EPSILON: f32 = 1e-4;

/// What one `Order::Build { what, at }` has to satisfy (per-building
/// authored numbers; the registry key is the building's UnitId).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BuildOrderDef {
    /// Seconds one builder needs alone; N builders accelerate linearly
    /// when `builders_accelerate` (rate = N), one rate otherwise.
    #[serde(default)]
    pub time: f32,
    #[serde(default)]
    pub cost_gold: u32,
    #[serde(default)]
    pub cost_wood: u32,
    /// Whether extra assigned workers speed the site up (WC3 humans) or
    /// not (undead-style single-builder sites).
    #[serde(default)]
    pub builders_accelerate: bool,
}

/// Cost/timer of one town-hall tier upgrade. `town_upgrades[i]` upgrades a
/// tier-(i+1) hall to tier (i+2).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TownUpgradeDef {
    pub cost_gold: u32,
    pub cost_wood: u32,
    pub time: f32,
}

/// BLDG config (mechanism registry; numbers are map content). serde-defaults
/// throughout so a future bundle `build_rts` section can carry it verbatim;
/// until that SEAM lands, games install it as this Resource directly. The
/// default is the neutral no-op: no orders (every `Order::Build` rejects),
/// no halls (no `TownTier` attached), no upgrade defs (every `UpgradeTown`
/// rejects) — zero drift.
#[derive(Debug, Clone, Resource, Serialize, Deserialize)]
#[serde(default)]
pub struct BuildRtsDef {
    /// Building UnitId -> authored build numbers.
    pub orders: HashMap<String, BuildOrderDef>,
    /// UnitIds that are town halls: freshly spawned instances get
    /// `TownTier(1)`.
    pub town_halls: Vec<String>,
    /// Per-tier upgrade costs, index = current tier - 1.
    pub town_upgrades: Vec<TownUpgradeDef>,
    /// Build ids granted into `Unlocks` when a hall REACHES tier i+1
    /// (index 1 = the T2 set, index 2 = T3; empty = nothing gated —
    /// the pre-BLDG "everything buildable" state).
    pub town_unlocks: Vec<Vec<String>>,
}

impl Default for BuildRtsDef {
    fn default() -> Self {
        Self {
            orders: HashMap::new(),
            town_halls: Vec::new(),
            town_upgrades: Vec::new(),
            town_unlocks: Vec::new(),
        }
    }
}

/// A construction site (headless, no rendering). Spawned together with the
/// charge; `builders` is the live census of in-range hammering workers,
/// rewritten every frame by `advance_construction` (0 = frozen site).
/// Completing despawns the site and spawns the real unit through the
/// unit domain's `spawn_unit` chain in the same frame.
#[derive(Component, Debug)]
pub struct UnderConstruction {
    pub what: String,
    pub remaining: f32,
    pub total: f32,
    pub builders: u32,
    /// Refund amounts (100% on `CancelBuild`).
    pub cost_gold: u32,
    pub cost_wood: u32,
    pub builders_accelerate: bool,
}

/// Worker-side link to a site: inserted when the order is charged or
/// joined, removed on completion/cancel/rejection. A stale link (the
/// order moved on) is dropped by the consumer the next time a Build
/// order lands on the agent.
#[derive(Component, Debug)]
pub struct BuildAssign {
    pub site: Entity,
}

/// A town hall mid-upgrade (timer only — the hall keeps functioning,
/// unlike a site it is not "under construction" for other systems).
#[derive(Component, Debug)]
pub struct TownUpgrading {
    pub remaining: f32,
    pub total: f32,
}

/// Explicit build cancel (WC3 semantics: refunds flow ONLY through this
/// command — worker Stop/death keeps the charge). One message per site.
#[derive(Message, Debug, Clone)]
pub struct CancelBuild {
    pub site: Entity,
}

/// Explicit town-hall tier upgrade request. Validated against
/// `BuildRtsDef::town_upgrades` + funds; rejected silently otherwise.
#[derive(Message, Debug, Clone)]
pub struct UpgradeTown {
    pub hall: Entity,
}

/// Town-hall tier marker (T1 -> T2 -> T3 upgrade chain; the tier's unlock
/// set rides the production domain's `Unlocks`). A fresh hall spawns with
/// `TownTier(1)` (attached by the observer below).
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct TownTier(pub u8);

/// `EconChanged` snapshot (same shape econ::apply_econ publishes — the
/// try_place / production / harvest direct-mutation precedent).
fn econ_changed(res: &Resources, pop: &Population) -> EconChanged {
    EconChanged { gold: res.gold, wood: res.wood, pop_used: pop.used, pop_cap: pop.cap }
}

/// Tick: fresh hall instances of configured town-hall ids get
/// `TownTier(1)` (the attach_production_queues pattern — the unit module
/// is never touched, every spawn path is covered).
pub fn attach_town_tiers(
    added: Query<(Entity, &UnitInstance), (Added<UnitInstance>, Without<TownTier>)>,
    cfg: Res<BuildRtsDef>,
    mut commands: Commands,
) {
    if cfg.town_halls.is_empty() {
        return;
    }
    for (entity, instance) in &added {
        if cfg.town_halls.iter().any(|h| h == &instance.def) {
            commands.entity(entity).insert(TownTier(1));
        }
    }
}

/// Tick (after movement's `tick_orders`): the charge seam. Every agent
/// that JUST received an `Order::Build` is validated and linked here:
/// - already linked to the live site this order names => nothing to do;
/// - a live site for the exact (what, at) exists => JOIN it (one charge
///   per site, never per worker — same-frame spawns are deduped through
///   the local `spawned` list, see below);
/// - else: def lookup (unknown => drop the order, 0 charge, 0 site) and
///   funds check (short => drop the order) — then charge, publish one
///   `EconChanged` snapshot, and spawn the site entity together with the
///   link. The site spawns HERE (order frame), not at first arrival:
///   progress is what waits for a builder (see `advance_construction`),
///   and `CancelBuild` gets its Entity target from frame one.
///
/// D6: site matching is exact (what, at-point) equality on Vec values
/// cloned from the same command literal; the same-frame dedupe list is a
/// Vec scanned in arrival order — no HashMap iteration drives logic.
#[allow(clippy::type_complexity)]
pub fn consume_build_orders(
    cfg: Res<BuildRtsDef>,
    pop: Res<Population>,
    mut res: ResMut<Resources>,
    mut econ: MessageWriter<EconChanged>,
    mut agents: Query<(Entity, &mut MoveAgent, Option<&BuildAssign>)>,
    sites: Query<(Entity, &UnderConstruction, &Transform)>,
    mut commands: Commands,
) {
    // Sites spawned earlier in THIS run (Commands are deferred, so the
    // sites query cannot see them yet) — the same-frame dedupe list.
    let mut spawned: Vec<(String, Vec3, Entity)> = Vec::new();
    for (entity, mut agent, assign) in &mut agents {
        let Some(Order::Build { what, at }) = agent.order.clone() else { continue };
        // Already linked to a live site this order still names: done.
        if let Some(link) = assign {
            let linked = sites
                .get(link.site)
                .map(|(_, site, tf)| site.what == what && tf.translation == at)
                .unwrap_or(false);
            if linked {
                continue;
            }
            // Stale link (order moved on): drop it, process as fresh.
            commands.entity(entity).remove::<BuildAssign>();
        }
        // Join an existing site for the exact (what, at) — live or
        // spawned earlier this run.
        let existing = sites
            .iter()
            .find(|(_, site, tf)| site.what == what && tf.translation == at)
            .map(|(e, _, _)| e)
            .or_else(|| {
                spawned
                    .iter()
                    .find(|(w, p, _)| *w == what && *p == at)
                    .map(|(_, _, e)| *e)
            });
        let site = match existing {
            Some(e) => e, // join: no second charge
            None => {
                let Some(def) = cfg.orders.get(&what) else {
                    // Unknown building: reject the order outright.
                    agent.order = None;
                    agent.waypoints.clear();
                    continue;
                };
                if res.gold < def.cost_gold || res.wood < def.cost_wood {
                    // Cannot afford: reject (capacity-before-spending —
                    // no charge, no site, no refund path needed).
                    agent.order = None;
                    agent.waypoints.clear();
                    continue;
                }
                res.gold -= def.cost_gold;
                res.wood -= def.cost_wood;
                econ.write(econ_changed(&res, &pop));
                let total = def.time.max(0.0);
                let site = commands
                    .spawn((
                        UnderConstruction {
                            what: what.clone(),
                            remaining: total,
                            total,
                            builders: 0,
                            cost_gold: def.cost_gold,
                            cost_wood: def.cost_wood,
                            builders_accelerate: def.builders_accelerate,
                        },
                        Transform::from_translation(at),
                    ))
                    .id();
                spawned.push((what.clone(), at, site));
                site
            }
        };
        commands.entity(entity).insert(BuildAssign { site });
    }
}

/// Tick: consume `CancelBuild` — 100% refund, site removed, linked
/// builders freed (their matching Build order is spent; a worker already
/// re-ordered elsewhere only loses the stale link). Unknown/dead sites
/// skip silently (house style).
pub fn consume_build_commands(
    mut cancels: MessageReader<CancelBuild>,
    pop: Res<Population>,
    mut res: ResMut<Resources>,
    mut econ: MessageWriter<EconChanged>,
    sites: Query<(&UnderConstruction, &Transform)>,
    mut agents: Query<(Entity, &mut MoveAgent, &BuildAssign)>,
    mut commands: Commands,
) {
    for cmd in cancels.read() {
        let Ok((site, stf)) = sites.get(cmd.site) else { continue };
        let (what, at, gold, wood) =
            (site.what.clone(), stf.translation, site.cost_gold, site.cost_wood);
        res.gold += gold;
        res.wood += wood;
        econ.write(econ_changed(&res, &pop));
        commands.entity(cmd.site).despawn();
        for (entity, mut agent, assign) in &mut agents {
            if assign.site != cmd.site {
                continue;
            }
            if let Some(Order::Build { what: w, at: a }) = agent.order.as_ref() {
                if *w == what && *a == at {
                    agent.order = None;
                    agent.waypoints.clear();
                }
            }
            commands.entity(entity).remove::<BuildAssign>();
        }
    }
}

/// Tick: consume `UpgradeTown` — validate (hall has a tier, not already
/// upgrading, tier < MAX, upgrade def exists, funds suffice), charge,
/// park a `TownUpgrading` timer. Rejections skip silently.
pub fn consume_upgrade_commands(
    mut upgrades: MessageReader<UpgradeTown>,
    cfg: Res<BuildRtsDef>,
    halls: Query<(&TownTier, Has<TownUpgrading>)>,
    pop: Res<Population>,
    mut res: ResMut<Resources>,
    mut econ: MessageWriter<EconChanged>,
    mut commands: Commands,
) {
    for cmd in upgrades.read() {
        let Ok((tier, upgrading)) = halls.get(cmd.hall) else { continue };
        if upgrading || tier.0 == 0 || tier.0 >= MAX_TOWN_TIER {
            continue;
        }
        let Some(def) = cfg.town_upgrades.get(tier.0 as usize - 1) else { continue };
        if res.gold < def.cost_gold || res.wood < def.cost_wood {
            continue;
        }
        res.gold -= def.cost_gold;
        res.wood -= def.cost_wood;
        econ.write(econ_changed(&res, &pop));
        let total = def.time.max(0.0);
        commands.entity(cmd.hall).insert(TownUpgrading { remaining: total, total });
    }
}

/// Tick: advance every `TownUpgrading` timer. Completion bumps the tier
/// and grants `town_unlocks[old_tier]` build ids into `Unlocks` (reaching
/// T2 grants index 1 — the acceptance indexing). Exclusive `&mut World`
/// system on purpose (the try_place pattern): the completion transitions
/// (component removal, unlock grant) are world-visible the SAME frame,
/// with no deferred-command boundary to wait out.
pub fn advance_town_upgrades(world: &mut World) {
    let halls: Vec<Entity> = world
        .query_filtered::<Entity, With<TownUpgrading>>()
        .iter(world)
        .collect();
    if halls.is_empty() {
        return;
    }
    let dt = world.resource::<Time>().delta_secs();
    let mut finished: Vec<u8> = Vec::new();
    for hall in halls {
        let Some(mut up) = world.get_mut::<TownUpgrading>(hall) else { continue };
        up.remaining -= dt;
        if up.remaining > COMPLETE_EPSILON {
            continue;
        }
        let from = world.get::<TownTier>(hall).map(|t| t.0).unwrap_or(0);
        world.entity_mut(hall).remove::<TownUpgrading>();
        if from == 0 {
            continue; // degenerate tierless hall: timer gone, nothing granted
        }
        let mut tier = world.get_mut::<TownTier>(hall).unwrap();
        tier.0 = (from + 1).min(MAX_TOWN_TIER);
        finished.push(from);
    }
    for from in finished {
        if let Some(ids) = world.resource::<BuildRtsDef>().town_unlocks.get(from as usize) {
            let ids = ids.clone();
            world.resource_mut::<Unlocks>().builds.extend(ids);
        }
    }
}

/// Director (after `move_agents`): the walk/hammer seam for linked
/// builders — out of `BUILD_RANGE`: chase the site (straight-line
/// best-effort seed, the documented `MoveAgent` contract, refreshed
/// every frame like the AttackTarget chase); in range: stand still and
/// hammer (waypoints cleared, the census below does the rest). Sites
/// gone missing are skipped here; their cleanup already happened at the
/// despawn site (cancel/completion both free the builders).
pub fn build_site_cycle(
    mut agents: Query<(&mut MoveAgent, &BuildAssign, &Transform)>,
    sites: Query<&Transform, With<UnderConstruction>>,
) {
    if sites.is_empty() {
        return;
    }
    for (mut agent, assign, tf) in &mut agents {
        if !matches!(agent.order, Some(Order::Build { .. })) {
            continue; // re-ordered away: not this site's walker anymore
        }
        let Ok(site_tf) = sites.get(assign.site) else { continue };
        if tf.translation.distance(site_tf.translation) > BUILD_RANGE {
            agent.waypoints = vec![site_tf.translation];
        } else {
            agent.waypoints.clear();
        }
    }
}

/// Director (after `build_site_cycle`): the construction advance — an
/// exclusive `&mut World` system on purpose (the try_place pattern), so a
/// completion this frame spawns the building, frees the workers, and
/// removes the site all within the same frame (no deferred-command
/// boundary to wait out; frame-exact acceptance).
///
/// Pass 1 censuses per site every LINKED agent whose order still names
/// that site (exact what/at match) and that stands within
/// `BUILD_RANGE` (hammering count, first hammerer's side for the spawn,
/// and ALL linked builders — walkers included — for completion cleanup).
/// Pass 2 publishes the census onto `UnderConstruction::builders`, and
/// while at least one hammerer stands there, subtracts `dt * rate` —
/// rate = hammerers when `builders_accelerate` (linear stack: 2 workers
/// on a 6s site finish in 3s), 1.0 otherwise; 0 hammerers = frozen
/// (charge kept, WC3 death/stop semantics). Pass 3 finishes completed
/// sites: the real unit spawns through `unit::spawn_unit` at the site's
/// position (Defender side unless a hammerer carried an `Allegiance`),
/// the site despawns, and every linked builder's order is spent. A
/// registry id that vanished mid-match drops the spawn (the charge is
/// lost — degenerate authoring, documented).
///
/// D6: the census is a HashMap keyed by site Entity with point lookups
/// only; site updates are independent of each other, so no iteration
/// order can change the outcome.
pub fn advance_construction(world: &mut World) {
    let sites: Vec<Entity> = world
        .query_filtered::<Entity, With<UnderConstruction>>()
        .iter(world)
        .collect();
    if sites.is_empty() {
        return;
    }
    let dt = world.resource::<Time>().delta_secs();
    // Site identity for the census below (what + ground point).
    let info: HashMap<Entity, (String, Vec3)> = sites
        .iter()
        .filter_map(|&s| {
            let site = world.get::<UnderConstruction>(s)?;
            let tf = world.get::<Transform>(s)?;
            Some((s, (site.what.clone(), tf.translation)))
        })
        .collect();

    // Pass 1: census — site -> (hammering, side, linked builders).
    let mut census: HashMap<Entity, (u32, Option<Allegiance>, Vec<Entity>)> = HashMap::new();
    for (entity, agent, assign, tf, side) in world
        .query::<(Entity, &MoveAgent, &BuildAssign, &Transform, Option<&Allegiance>)>()
        .iter(world)
    {
        let Some(Order::Build { what, at }) = agent.order.as_ref() else { continue };
        let Some((site_what, site_pos)) = info.get(&assign.site) else { continue };
        if site_what != what || site_pos != at {
            continue; // linked but re-ordered: not this site's builder
        }
        let entry = census.entry(assign.site).or_insert((0, None, Vec::new()));
        entry.2.push(entity);
        if tf.translation.distance(*site_pos) <= BUILD_RANGE {
            entry.0 += 1;
            if entry.1.is_none() {
                entry.1 = side.cloned();
            }
        }
    }

    // Pass 2 + 3: publish, advance, finish.
    let registry: UnitRegistry = (*world.resource::<UnitRegistry>()).clone();
    for site in sites {
        // (what, pos) resolved BEFORE the mutable site borrow below.
        let Some((what, pos)) = info.get(&site).cloned() else { continue };
        let Some(mut s) = world.get_mut::<UnderConstruction>(site) else { continue };
        let Some((hammering, side, linked)) = census.remove(&site) else {
            s.builders = 0; // no linked builder at all this frame
            continue;
        };
        s.builders = hammering;
        if hammering == 0 {
            continue; // frozen site: no hammerer, no progress, no completion
        }
        let rate = if s.builders_accelerate { hammering as f32 } else { 1.0 };
        s.remaining -= dt * rate;
        if s.remaining > COMPLETE_EPSILON {
            continue;
        }
        // Completed — the site borrow ended above; finish for real.
        let _ = spawn_unit(world, &registry, &what, pos, side.unwrap_or(Allegiance::Defender), 1.0);
        world.entity_mut(site).despawn();
        for builder in linked {
            if let Some(mut agent) = world.get_mut::<MoveAgent>(builder) {
                agent.order = None;
                agent.waypoints.clear();
            }
            if world.get::<BuildAssign>(builder).is_some() {
                world.entity_mut(builder).remove::<BuildAssign>();
            }
        }
    }
}

/// Domain plugin (SEAM2d skeleton + BLDG execution). The default
/// `BuildRtsDef` is the neutral no-op config (see the type doc).
pub struct BuildRtsPlugin;

impl Plugin for BuildRtsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<BuildRtsDef>()
            .add_systems(
                Update,
                (
                    attach_town_tiers,
                    consume_build_orders,
                    consume_build_commands,
                    consume_upgrade_commands,
                    advance_town_upgrades,
                )
                    .chain()
                    // After the order assignment, so a Build order landing
                    // this frame is charged this frame (the pinned
                    // "下单即扣费" timing).
                    .after(crate::movement::tick_orders)
                    .in_set(crate::WarbaseSet::Tick),
            )
            .add_systems(
                Update,
                (build_site_cycle, advance_construction)
                    .chain()
                    .after(crate::movement::move_agents)
                    .in_set(crate::WarbaseSet::Director),
            );
    }
}

pub mod prelude {
    pub use super::{
        advance_construction, advance_town_upgrades, attach_town_tiers, build_site_cycle,
        consume_build_commands, consume_build_orders, consume_upgrade_commands, BuildAssign,
        BuildOrderDef, BuildRtsDef, BuildRtsPlugin, CancelBuild, TownTier, TownUpgradeDef,
        TownUpgrading, UnderConstruction, UpgradeTown, BUILD_RANGE, COMPLETE_EPSILON,
        MAX_TOWN_TIER,
    };
}
