//! Harvest (ADR-0018 Phase-1 HARV): the WC3 worker loop — walk to a mine,
//! load one trip of cargo, haul it to a dropoff building, bank it through
//! the economy seam (upkeep factor applied), walk back to the nearest mine,
//! repeat. Depleted mines refuse to load (`Carrying` never increases);
//! cargo gathered before a depletion still delivers in full.
//!
//! Mechanism only, no game numbers (ADR-0012): the per-trip load, dropoff
//! ids, and the upkeep ladder all ship in the map bundle's `harvest`
//! section (`HarvestDef`); the WC3 classic three-tier table is map content.
//! Zero drift: maps without deposits (and agents without `Order::Harvest`)
//! leave every system here a no-op — the base's `Resources`/`Population`
//! behave exactly as before.
//!
//! Movement is observed, never modified: the cycle watches `MoveAgent`s
//! holding `Order::Harvest`/`Order::ReturnCargo` (parked by movement's
//! `tick_orders`), seeds their `waypoints` directly (straight-line
//! best-effort, the documented `MoveAgent` contract — same as MOV4's seed),
//! and chains legs through public `UnitCommand` messages so replays and
//! tests observe the loop exactly like a human-issued command.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;

use crate::econ::{EconChanged, Population, Resources};
use crate::movement::{MoveAgent, Order, UnitCommand};
use crate::production::ProductionQueue;
use crate::unit::UnitInstance;

/// Ground distance at which gathering and delivery trigger (HARV spec:
/// within 1.5 of the mine / dropoff). A mechanism constant like
/// `DEFAULT_BODY_RADIUS`, not map content; workers should spawn with
/// `arrival_radius` below it so the locomotion brake still lands inside
/// the trigger zone.
pub const GATHER_RANGE: f32 = 1.5;

/// Map-bundle harvest section: the resource deposits plus the gathering
/// rules. Everything serde-defaults so pre-HARV maps load unchanged.
/// Also installed as the live config `Resource` by `apply_harvest`.
#[derive(Debug, Clone, Resource, Serialize, Deserialize)]
pub struct HarvestDef {
    #[serde(default)]
    pub deposits: Vec<MineDef>,
    /// Cargo units one worker loads per trip (plan default 10).
    #[serde(default = "default_per_trip")]
    pub per_trip: u32,
    /// Unit ids of cargo-accepting buildings. EMPTY = any building with a
    /// `ProductionQueue` (the pre-authoring default).
    #[serde(default)]
    pub dropoffs: Vec<String>,
    /// Upkeep ladder, ascending by population floor: (floor, payout
    /// factor). EMPTY = no upkeep — every delivery banks in full (zero
    /// drift). Tier selection is `upkeep_factor`; the WC3 classic ladder
    /// is map content, e.g. [(0, 1.0), (51, 0.7), (80, 0.4)].
    #[serde(default)]
    pub upkeep_tiers: Vec<(u8, f32)>,
}

fn default_per_trip() -> u32 {
    10
}

impl Default for HarvestDef {
    fn default() -> Self {
        Self {
            deposits: Vec::new(),
            per_trip: default_per_trip(),
            dropoffs: Vec::new(),
            upkeep_tiers: Vec::new(),
        }
    }
}

/// One authored resource deposit (a gold mine / lumber patch; the id names
/// the resource kind, content stays game-side).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MineDef {
    pub id: String,
    /// Ground (x, z) — hole-1 XZ convention.
    pub pos: [f32; 2],
    /// Extractable units before depletion (HARV decides the per-trip cut).
    #[serde(default)]
    pub capacity: u32,
}

/// Runtime state of a live deposit (spawned by `apply_harvest`).
#[derive(Component, Debug)]
pub struct Mine {
    pub deposit: String,
    pub remaining: u32,
}

/// Cargo a worker is currently hauling. Inserted on the first gather,
/// zeroed on every delivery (the component stays — a zero read is "empty
/// handed", cheaper than insert/remove churn).
#[derive(Component, Debug, Default)]
pub struct Carrying(pub u32);

/// Payout factor for a population under an upkeep ladder. Pinned rule:
/// the LAST tier whose floor is `<= used` wins (tiers are read as an
/// ascending ladder of population floors); no tier applies (empty ladder,
/// or `used` below every floor) => 1.0, i.e. no upkeep — the zero-drift
/// default. Pure; tests assert it directly.
pub fn upkeep_factor(tiers: &[(u8, f32)], used: u8) -> f32 {
    tiers
        .iter()
        .filter(|(floor, _)| *floor <= used)
        .map(|(_, factor)| *factor)
        .next_back()
        .unwrap_or(1.0)
}

/// Is this building a legal cargo dropoff under the section rules?
/// Authored `dropoffs` (unit ids) decide; an empty list means "any
/// building with a ProductionQueue".
fn is_dropoff(inst: &UnitInstance, cfg: &HarvestDef, has_queue: bool) -> bool {
    if cfg.dropoffs.is_empty() {
        has_queue
    } else {
        cfg.dropoffs.iter().any(|d| d == &inst.def)
    }
}

/// Distance-ascending nearest with the entity-index tie-break (D6 — query
/// iteration order never decides logic).
fn nearest(mut candidates: impl Iterator<Item = (f32, Entity)>) -> Option<Entity> {
    let mut best: Option<(f32, u32, Entity)> = None;
    for (dist, e) in &mut candidates {
        let idx = e.index_u32();
        let better = match best {
            None => true,
            Some((bd, bi, _)) => dist.total_cmp(&bd).then(idx.cmp(&bi)) == Ordering::Less,
        };
        if better {
            best = Some((dist, idx, e));
        }
    }
    best.map(|(_, _, e)| e)
}

/// `EconChanged` snapshot (same shape econ::apply_econ publishes — the
/// production domain's direct-mutation precedent, copied verbatim).
fn econ_changed(res: &Resources, pop: &Population) -> EconChanged {
    EconChanged { gold: res.gold, wood: res.wood, pop_used: pop.used, pop_cap: pop.cap }
}

/// Director (after `move_agents`): the worker loop, one pass over every
/// `MoveAgent` holding a harvest order.
///
/// `Order::Harvest { target }` (a `Mine` entity):
/// - Out of `GATHER_RANGE`: seed `waypoints = [mine]` every frame
///   (straight-line best-effort; locomotion drives the walk — the
///   AttackTarget-chase pattern, without touching movement).
/// - In range and `remaining > 0`: load `min(per_trip, remaining)` into
///   `Carrying` (a re-gather while already carrying overwrites — manual
///   command interrupts restart the trip at one load, never stack), then
///   chain `UnitCommand::ReturnCargo` at the NEAREST legal dropoff (no
///   dropoff on the map yet: the worker waits loaded and retries every
///   frame, so the loop resumes the moment one spawns).
/// - In range and `remaining == 0`: REFUSE to gather — `Carrying` never
///   increases (pinned). The worker idles at the mine; no auto ReturnCargo
///   happens without cargo.
///
/// `Order::ReturnCargo { target }` (a building entity):
/// - Out of range: seed the walk toward the building.
/// - In range: bank `Carrying` through `Resources.gold +=
///   round(carried * upkeep_factor)` and publish one `EconChanged`
///   snapshot (zero cargo => no econ write, no message — nothing changed);
///   then chain `UnitCommand::Harvest` at the NEAREST mine so the loop
///   re-arms by itself (nearest, because cargo does not remember its
///   source — with one mine per base this IS the source mine; a depleted
///   target is fine, the gather step above refuses there). No mine on the
///   map: the worker idles, order spent.
/// - Target despawned: drop the order, KEEP the cargo (a later harvest
///   round-trip delivers it — WC3 workers never spill).
///
/// Any other order / no order: untouched (zero drift for every other
/// agent — warden, loop-td, creeps).
#[allow(clippy::type_complexity)]
pub fn harvest_cycle(
    cfg: Res<HarvestDef>,
    pop: Res<Population>,
    mut res: ResMut<Resources>,
    mut econ: MessageWriter<EconChanged>,
    mut orders: MessageWriter<UnitCommand>,
    mut mines: Query<(Entity, &mut Mine, &Transform)>,
    buildings: Query<(Entity, &UnitInstance, &Transform, Has<ProductionQueue>)>,
    mut workers: Query<(Entity, &mut MoveAgent, &Transform, Option<&mut Carrying>)>,
    mut commands: Commands,
) {
    for (entity, mut agent, tf, carrying) in &mut workers {
        let Some(order) = agent.order.clone() else { continue };
        match order {
            Order::Harvest { target } => {
                let Ok((_, mut mine, mtf)) = mines.get_mut(target) else {
                    // Not a (live) mine: drop the order, worker idles.
                    agent.order = None;
                    agent.waypoints.clear();
                    continue;
                };
                if tf.translation.distance(mtf.translation) > GATHER_RANGE {
                    agent.waypoints = vec![mtf.translation]; // walk to the mine
                    continue;
                }
                if mine.remaining == 0 {
                    // Depleted: refuse to gather — Carrying never increases.
                    continue;
                }
                let take = cfg.per_trip.min(mine.remaining);
                mine.remaining -= take;
                match carrying {
                    Some(mut c) => c.0 = take,
                    None => {
                        commands.entity(entity).insert(Carrying(take));
                    }
                }
                let depot = nearest(buildings.iter().filter_map(|(e, inst, btf, q)| {
                    is_dropoff(inst, &*cfg, q)
                        .then(|| (tf.translation.distance(btf.translation), e))
                }));
                if let Some(depot) = depot {
                    orders.write(UnitCommand {
                        order: Order::ReturnCargo { target: depot },
                        subjects: vec![entity],
                    });
                }
            }
            Order::ReturnCargo { target } => {
                let Ok((_, _, btf, _)) = buildings.get(target) else {
                    // Dropoff gone: end the order, keep the cargo.
                    agent.order = None;
                    agent.waypoints.clear();
                    continue;
                };
                if tf.translation.distance(btf.translation) > GATHER_RANGE {
                    agent.waypoints = vec![btf.translation]; // walk to the dropoff
                    continue;
                }
                // In range: bank the cargo at the upkeep-adjusted rate.
                let carried = carrying.as_ref().map(|c| c.0).unwrap_or(0);
                if carried > 0 {
                    let factor = upkeep_factor(&cfg.upkeep_tiers, pop.used);
                    res.gold += ((carried as f32) * factor).round() as u32;
                    econ.write(econ_changed(&res, &pop));
                    if let Some(mut c) = carrying {
                        c.0 = 0;
                    }
                }
                // Chain the return leg: nearest mine (depleted is fine).
                let back = nearest(
                    mines.iter().map(|(e, _, mtf)| (tf.translation.distance(mtf.translation), e)),
                );
                if let Some(back) = back {
                    orders.write(UnitCommand {
                        order: Order::Harvest { target: back },
                        subjects: vec![entity],
                    });
                }
            }
            _ => continue,
        }
    }
}

/// Bundle dispatch (SEAM0d symmetry): spawn one `Mine` entity per authored
/// deposit (remaining = capacity) and install the section as the live
/// `HarvestDef` resource (the default resource from the plugin is the
/// neutral no-op config; assembling overwrites it). Empty section =>
/// zero entities, default config stays — zero drift.
pub fn apply_harvest(world: &mut World, bundle: &crate::bundle::MapBundle) {
    for d in &bundle.harvest.deposits {
        world.spawn((
            Mine { deposit: d.id.clone(), remaining: d.capacity },
            Transform::from_translation(Vec3::new(d.pos[0], 0.0, d.pos[1])),
        ));
    }
    world.insert_resource(bundle.harvest.clone());
}

/// Domain plugin (SEAM2d skeleton + HARV cycle). The default `HarvestDef`
/// is neutral: no deposits, no upkeep — every map without the section
/// behaves exactly as before.
pub struct HarvestPlugin;

impl Plugin for HarvestPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<HarvestDef>().add_systems(
            Update,
            harvest_cycle
                .after(crate::movement::move_agents)
                .in_set(crate::WarbaseSet::Director),
        );
    }
}

pub mod prelude {
    pub use super::{
        apply_harvest, harvest_cycle, upkeep_factor, Carrying, GATHER_RANGE, HarvestDef,
        HarvestPlugin, Mine, MineDef,
    };
}
