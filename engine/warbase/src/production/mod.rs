//! Production & research (WC3 alignment, ADR-0016): building train queues
//! (pay on enqueue, 100% refund on cancel, population-gated pause), player
//! research (stat grants / build & skill unlocks), rally points.
//!
//! Scheduling: queue/research progress ticks in Tick (PRD1/2); completed
//! units spawn via unit::spawn_unit_deferred. SEAM0b pins the types.
//!
//! PRD1 economics follow the build domain (build::try_place), NOT the
//! EconCommand stream: pre-validate against `Resources`, then mutate
//! directly and publish an `EconChanged` snapshot — the message stream
//! stays free of an enqueue/reject rollback dance. Population uses the
//! same `used >= cap` gate try_place applies, re-checked every frame so a
//! full cap PAUSES training instead of rejecting the enqueue (WC3).

use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};

use crate::econ::{EconChanged, Population, Resources};
use crate::unit::{spawn_unit_deferred, Allegiance, UnitInstance, UnitRegistry};

/// WC3 default: at most five queued entries per building.
pub const DEFAULT_QUEUE_CAP: u8 = 5;
/// Fallback train duration for units missing from `ProductionDef::train_times`.
pub const DEFAULT_TRAIN_TIME: f32 = 5.0;
/// Trained units spawn this far from the builder's origin (one grid cell).
pub const SPAWN_OFFSET: f32 = 1.0;

/// One queued entry (unit or research id) with its remaining seconds.
#[derive(Debug, Clone)]
pub struct QueueEntry {
    pub what: String,
    pub remaining: f32,
    /// Total duration (progress = 1 - remaining/total).
    pub total: f32,
}

/// Building-side production component. `trains` lists trainable unit ids;
/// `queue_cap` bounds the VecDeque (WC3 default 5).
#[derive(Component, Debug)]
pub struct ProductionQueue {
    pub trains: Vec<String>,
    pub queue_cap: u8,
    pub entries: VecDeque<QueueEntry>,
    /// Population-full pause (readable state, PRD1).
    pub paused: bool,
}

/// Player command: enqueue a unit on a building (pays immediately).
#[derive(Message, Debug, Clone)]
pub struct TrainCommand {
    pub builder: Entity,
    pub unit: String,
}

/// Player command: cancel queue entry `index` (100% refund).
#[derive(Message, Debug, Clone)]
pub struct CancelTrain {
    pub builder: Entity,
    pub index: usize,
}

/// Rally point: freshly trained units receive this default order (none =
/// hold position). PRD3.
#[derive(Component, Debug, Default, Clone)]
pub struct RallyPoint(pub Option<crate::movement::Order>);

/// Research grant vocabulary (PRD2): what a finished research unlocks.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ResearchGrant {
    /// Permanent stat mod for every unit of `unit` (e.g. attack upgrade).
    Stat { unit: String, kind: crate::buff::StatKind, amount: f32 },
    /// Reveals a hidden buildable in command cards.
    UnlockBuild(String),
    /// Reveals a hidden skill on its carriers.
    UnlockSkill(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResearchDef {
    pub id: String,
    pub cost_gold: u32,
    pub time: f32,
    pub grants: Vec<ResearchGrant>,
}

/// Map-bundle production/research section (SEAM0d; optional, defaults
/// empty). `research` is the per-map tech tree; queues live on entities.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProductionDef {
    #[serde(default)]
    pub research: Vec<ResearchDef>,
    /// Player-side single research queue (simplification, plan §8 PRD2).
    #[serde(default = "default_research_cap")]
    pub research_cap: u8,
    /// Building UnitId -> trainable UnitIds (PRD1). Buildings whose id is
    /// absent from the table never grow a ProductionQueue.
    #[serde(default)]
    pub trains: HashMap<String, Vec<String>>,
    /// Trainable UnitId -> train duration in seconds (PRD1); missing ids
    /// fall back to [`DEFAULT_TRAIN_TIME`].
    #[serde(default)]
    pub train_times: HashMap<String, f32>,
}

fn default_research_cap() -> u8 {
    1
}

/// Assembled train table (PRD1): building UnitId -> trainable UnitIds.
/// Maps without a production section get the empty default, so their
/// spawned units never match and the observer below stays a no-op —
/// warden / loop-td behavior is untouched.
#[derive(Resource, Debug, Default, Clone)]
pub struct TrainTable(pub HashMap<String, Vec<String>>);

/// Assembled train durations (PRD1): unit UnitId -> seconds.
#[derive(Resource, Debug, Default, Clone)]
pub struct TrainTimes(pub HashMap<String, f32>);

/// Bundle dispatch (SEAM0d): assemble the PRD1 train resources.
pub fn apply_production(world: &mut World, bundle: &crate::bundle::MapBundle) {
    world.insert_resource(TrainTable(bundle.production.trains.clone()));
    world.insert_resource(TrainTimes(bundle.production.train_times.clone()));
    // TODO(PRD2): insert ResearchTable from ProductionDef.
}

/// `EconChanged` snapshot (same shape econ::apply_econ publishes), so UI
/// panels refresh on train charges/refunds exactly like on tower builds.
fn econ_changed(res: &Resources, pop: &Population) -> EconChanged {
    EconChanged { gold: res.gold, wood: res.wood, pop_used: pop.used, pop_cap: pop.cap }
}

/// Attach a `ProductionQueue` to freshly spawned builders: any new
/// `UnitInstance` whose def id hits the train table gets one on its spawn
/// frame. Watching `Added<UnitInstance>` (instead of hooking spawn_unit)
/// keeps the unit module untouched and covers every spawn path.
pub fn attach_production_queues(
    added: Query<(Entity, &UnitInstance), Added<UnitInstance>>,
    table: Res<TrainTable>,
    mut commands: Commands,
) {
    for (entity, instance) in &added {
        if let Some(trains) = table.0.get(&instance.def) {
            commands.entity(entity).insert(ProductionQueue {
                trains: trains.clone(),
                queue_cap: DEFAULT_QUEUE_CAP,
                entries: VecDeque::new(),
                paused: false,
            });
        }
    }
}

/// Consume `TrainCommand` (validate + pay + enqueue) and `CancelTrain`
/// (remove + 100% refund). Failures skip silently, matching the command
/// consumption style of the other domain consumers.
pub fn consume_train_commands(
    mut trains_in: MessageReader<TrainCommand>,
    mut cancels: MessageReader<CancelTrain>,
    registry: Res<UnitRegistry>,
    times: Res<TrainTimes>,
    mut res: ResMut<Resources>,
    pop: Res<Population>,
    mut queues: Query<&mut ProductionQueue>,
    mut changed: MessageWriter<EconChanged>,
) {
    for cmd in trains_in.read() {
        let Ok(mut queue) = queues.get_mut(cmd.builder) else { continue };
        if !queue.trains.iter().any(|t| t == &cmd.unit) {
            continue;
        }
        if queue.entries.len() >= queue.queue_cap as usize {
            continue;
        }
        let Some(def) = registry.get(&cmd.unit) else { continue };
        // Capacity-before-spending, same as build::try_place: no refund
        // path needed.
        if res.gold < def.cost_gold || res.wood < def.cost_wood {
            continue;
        }
        res.gold -= def.cost_gold;
        res.wood -= def.cost_wood;
        let total = times.0.get(&cmd.unit).copied().unwrap_or(DEFAULT_TRAIN_TIME).max(0.0);
        queue.entries.push_back(QueueEntry { what: cmd.unit.clone(), remaining: total, total });
        changed.write(econ_changed(&res, &pop));
    }
    for cmd in cancels.read() {
        let Ok(mut queue) = queues.get_mut(cmd.builder) else { continue };
        // 100% refund regardless of elapsed progress (WC3 semantics).
        let Some(entry) = queue.entries.remove(cmd.index) else { continue };
        if let Some(def) = registry.get(&entry.what) {
            res.gold += def.cost_gold;
            res.wood += def.cost_wood;
            changed.write(econ_changed(&res, &pop));
        }
    }
}

/// Advance queue heads: population gate FIRST (full cap = paused, frozen
/// remaining), then `remaining -= dt`. Finished heads spawn beside the
/// builder (Defender side), consume one population, and the queue moves on.
pub fn advance_production_queues(
    time: Res<Time>,
    registry: Res<UnitRegistry>,
    res: Res<Resources>,
    mut pop: ResMut<Population>,
    mut changed: MessageWriter<EconChanged>,
    mut queues: Query<(&mut ProductionQueue, &Transform)>,
    mut commands: Commands,
) {
    let dt = time.delta_secs();
    for (mut queue, transform) in queues.iter_mut() {
        if queue.entries.is_empty() {
            queue.paused = false;
            continue;
        }
        if pop.used >= pop.cap {
            queue.paused = true; // gate before progress: remaining frozen
            continue;
        }
        queue.paused = false;
        if let Some(head) = queue.entries.front_mut() {
            head.remaining -= dt;
        }
        // Finish every head that reached zero this frame, re-checking the
        // population gate between spawns: a cap filled by an earlier
        // graduate holds the rest of the queue too.
        loop {
            let Some(head) = queue.entries.front() else { break };
            if head.remaining > 0.0 {
                break;
            }
            if pop.used >= pop.cap {
                queue.paused = true;
                break;
            }
            let pos = transform.translation + Vec3::new(SPAWN_OFFSET, 0.0, 0.0);
            // Enqueue already validated the def id against the registry; a
            // vanished id (registry swap mid-match) drops the entry without
            // charging population.
            if spawn_unit_deferred(&mut commands, &registry, &head.what, pos, Allegiance::Defender, 1.0)
                .is_some()
            {
                pop.used += 1;
                changed.write(econ_changed(&res, &pop));
            }
            queue.entries.pop_front();
        }
    }
}

/// Domain systems (registered once by WarbasePlugin; PRD increments add
/// systems HERE, never in lib.rs).
pub struct ProductionPlugin;

impl Plugin for ProductionPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<TrainTable>()
            .init_resource::<TrainTimes>()
            .add_systems(
                Update,
                (
                    attach_production_queues,
                    consume_train_commands,
                    advance_production_queues,
                )
                    .chain()
                    // After the econ command stream settles, so same-frame
                    // bounty income is visible to train validations.
                    .after(crate::econ::apply_econ)
                    .in_set(crate::WarbaseSet::Tick),
            );
    }
}

pub mod prelude {
    pub use super::{
        CancelTrain, ProductionDef, ProductionPlugin, ProductionQueue, QueueEntry, RallyPoint,
        ResearchDef, ResearchGrant, TrainCommand, TrainTable, TrainTimes, DEFAULT_QUEUE_CAP,
        DEFAULT_TRAIN_TIME, SPAWN_OFFSET,
    };
}
