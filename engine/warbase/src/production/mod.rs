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
//!
//! PRD2/PRD3 resource-queue note: `StartResearch`/`CancelResearch` and
//! `set_rally` travel as RESOURCE command queues (`ResearchCommands`,
//! `RallyCommands`), not `Messages` — registering a new message type needs
//! a `lib.rs` `add_message` line, and lib.rs is frozen for parallel domain
//! increments (plan D3). The queues are drained in Tick; headless tests and
//! UI layers push commands into the resource directly.
//!
//! ---------------------------------------------------------------------------
//! MP2 ECONOMY PER-PLAYER EVALUATION CARD (evaluation only — decided
//! against implementing in this increment per plan §7 MP2 / ADR-0017;
//! recorded here so the next increment starts from a map of the work):
//!
//! 1. Wallets (effort M, cross-domain): `econ::Resources` and
//!    `econ::Population` are global singletons; multiplayer needs
//!    `HashMap<PlayerId, Resources>` (or a keyed resource) plus
//!    per-player income in `econ::apply_econ` and per-team or per-player
//!    population caps. Touches econ + build (`try_place` pays from the
//!    same global), so it is a SEAM-level change, not a PRD-local one.
//! 2. Train charging (effort S once wallets exist): `consume_train_commands`
//!    and its refund path must resolve the builder's `Owner(PlayerId)` and
//!    charge THAT player's wallet; unowned builders keep the legacy global
//!    wallet (single-player zero drift).
//! 3. Trained-unit ownership (effort S): `advance_production_queues` spawns
//!    graduates with a hard-coded `Allegiance::Defender`; multiplayer must
//!    inherit the builder's `Owner` and let the existing
//!    `unit::sync_allegiance` derive the side.
//! 4. Research columns (effort S–M): `ResearchState` is one global queue
//!    (plan §12.6 pins single-column as the v1 simplification); per-player
//!    research means keying the queue by `PlayerId` and validating each
//!    player's own wallet.
//! 5. Rally/command path (effort none): already per-entity; no player split
//!    required.
//! Verdict: do the wallet layer first (1), then (2)–(4) are mechanical
//! Owner plumbing. Until then this module is single-wallet by design.
//! ---------------------------------------------------------------------------

use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};

use crate::buff::{ModEntry, StatKind, StatMods};
use crate::econ::{EconChanged, Population, Resources};
use crate::movement::{Order, UnitCommand};
use crate::sfx::{SfxKind, SfxRequest};
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

/// Assembled tech tree (PRD2): research id -> def. Empty on maps without a
/// production section, so every `Start` below is rejected and the whole
/// research pipeline is a no-op — zero drift.
#[derive(Resource, Debug, Default, Clone)]
pub struct ResearchTable(pub HashMap<String, ResearchDef>);

/// Player-side research column (PRD2). Single queue by design (plan §12.6
/// pins the WC3 simplification): `cap` bounds concurrent entries, default 1
/// = one research at a time. (Multiplayer: one column per player — see the
/// economy evaluation card in the module header.)
#[derive(Resource, Debug)]
pub struct ResearchState {
    pub entries: VecDeque<QueueEntry>,
    pub cap: u8,
}

impl Default for ResearchState {
    fn default() -> Self {
        Self { entries: VecDeque::new(), cap: 1 }
    }
}

/// Player research commands (resource queue, NOT Messages — see the module
/// header note about the frozen lib.rs registration list).
#[derive(Debug, Clone)]
pub enum ResearchCommand {
    /// Start researching `id` (validated: def exists, gold suffices, the
    /// column has room; pays on start, WC3 semantics).
    Start { id: String },
    /// Cancel the ACTIVE (front) research; 100% refund regardless of
    /// progress, mirroring `CancelTrain`.
    Cancel,
}

/// Drained every Tick by [`consume_research_commands`].
#[derive(Resource, Debug, Default)]
pub struct ResearchCommands(pub VecDeque<ResearchCommand>);

/// One stable `ModEntry::source` for every research stat grant (pinned
/// design): a bare marker entity spawned once in `ProductionPlugin::build`
/// and never despawned. Because `StatMods::upsert` replaces by
/// (source, kind), all research grants funnel through this single source:
/// upserts are idempotent across frames, and a later grant for the same
/// (unit, kind) OVERWRITES the earlier one instead of stacking (WC3
/// attack/defense upgrades are distinct kinds, so same-kind stacking never
/// occurs in practice — documented v1 limitation).
#[derive(Resource, Debug)]
pub struct ResearchSource(pub Entity);

/// Completed research stat grants keyed by (unit def id, stat kind) — the
/// global type-modifier table (plan §8 PRD2). Written only by research
/// completion; read every frame by [`apply_research_stats`]. HashMap is
/// D6-safe: point lookups only, never iterated in order-dependent logic.
#[derive(Resource, Debug, Default, Clone)]
pub struct TypeModifiers(pub HashMap<(String, StatKind), f32>);

/// Build/skill unlocks revealed by finished research (PRD2). Consumption
/// seams live outside this module by design: command-card build buttons and
/// hero skill slots query via [`is_build_unlocked`] /
/// [`is_skill_unlocked`] (the ui/game side wires them when it lands).
#[derive(Resource, Debug, Default, Clone)]
pub struct Unlocks {
    pub builds: HashSet<String>,
    pub skills: HashSet<String>,
}

/// Pub query fn: has research revealed buildable `id`? Pure membership on
/// [`Unlocks`] — which ids are DEFAULT-gated (hidden until researched) vs
/// always-open is a game-side command-card concern; this base only records
/// and reports what finished research has revealed.
pub fn is_build_unlocked(unlocks: &Unlocks, id: &str) -> bool {
    unlocks.builds.contains(id)
}

/// Pub query fn: has research revealed skill `id` on its carriers? Same
/// membership-report contract as [`is_build_unlocked`].
pub fn is_skill_unlocked(unlocks: &Unlocks, id: &str) -> bool {
    unlocks.skills.contains(id)
}

/// Player rally commands (PRD3; resource queue — see module header).
#[derive(Debug, Clone)]
pub enum RallyCommand {
    /// Point freshly trained units at an order (typically Move/AttackMove
    /// to a point).
    Set { builder: Entity, order: Order },
    /// Remove the rally point; graduates hold position instead.
    Clear { builder: Entity },
}

/// Drained every Tick by [`consume_rally_commands`].
#[derive(Resource, Debug, Default)]
pub struct RallyCommands(pub VecDeque<RallyCommand>);

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

/// Bundle dispatch (SEAM0d): assemble the PRD1 train + PRD2 research
/// resources. Empty research lists leave the (default) empty table, so
/// maps without a production section keep the whole pipeline inert.
pub fn apply_production(world: &mut World, bundle: &crate::bundle::MapBundle) {
    world.insert_resource(TrainTable(bundle.production.trains.clone()));
    world.insert_resource(TrainTimes(bundle.production.train_times.clone()));
    let table = ResearchTable(
        bundle.production.research.iter().map(|d| (d.id.clone(), d.clone())).collect(),
    );
    world.insert_resource(table);
    world.insert_resource(Unlocks::default());
    world.insert_resource(TypeModifiers::default());
    world.insert_resource(ResearchState {
        entries: VecDeque::new(),
        // Whole-section absence deserializes research_cap as 0 (derive
        // Default); clamp to the plan's single-column minimum so an
        // authored tech tree is always startable.
        cap: bundle.production.research_cap.max(1),
    });
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
/// PRD3: a builder with `RallyPoint(Some(order))` broadcasts the order to
/// the graduate as a `UnitCommand` — deliberately NOT constructing a
/// `MoveAgent` here: its field set is still moving on the MOV line, while
/// `UnitCommand { order, subjects }` is the stable registered seam. Until
/// MOV4 `tick_orders` lands, an unconsumed command simply leaves the
/// graduate standing (hold) — zero-dependency decoupling. No rally point =
/// no command at all: the unit holds position.
pub fn advance_production_queues(
    time: Res<Time>,
    registry: Res<UnitRegistry>,
    res: Res<Resources>,
    mut pop: ResMut<Population>,
    mut changed: MessageWriter<EconChanged>,
    mut orders: MessageWriter<UnitCommand>,
    mut queues: Query<(&mut ProductionQueue, &Transform, Option<&RallyPoint>)>,
    mut commands: Commands,
) {
    let dt = time.delta_secs();
    for (mut queue, transform, rally) in queues.iter_mut() {
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
            if let Some(graduate) = spawn_unit_deferred(
                &mut commands,
                &registry,
                &head.what,
                pos,
                Allegiance::Defender,
                1.0,
            ) {
                pop.used += 1;
                changed.write(econ_changed(&res, &pop));
                if let Some(order) = rally.and_then(|r| r.0.clone()) {
                    orders.write(UnitCommand { order, subjects: vec![graduate] });
                }
            }
            queue.entries.pop_front();
        }
    }
}

/// Consume [`RallyCommands`] (PRD3): set/clear a builder's rally order.
/// Setting on an entity without the component inserts it; clearing writes
/// `None` (component stays, readable "no rally" state).
pub fn consume_rally_commands(
    mut queue: ResMut<RallyCommands>,
    mut builders: Query<&mut RallyPoint>,
    mut commands: Commands,
) {
    while let Some(cmd) = queue.0.pop_front() {
        match cmd {
            RallyCommand::Set { builder, order } => match builders.get_mut(builder) {
                Ok(mut rally) => rally.0 = Some(order),
                Err(_) => {
                    commands.entity(builder).insert(RallyPoint(Some(order)));
                }
            },
            RallyCommand::Clear { builder } => match builders.get_mut(builder) {
                Ok(mut rally) => rally.0 = None,
                Err(_) => {
                    commands.entity(builder).insert(RallyPoint::default());
                }
            },
        }
    }
}

/// Consume [`ResearchCommands`] (PRD2): validate (def exists -> column has
/// room -> gold suffices), then pay & enqueue; `Cancel` refunds the active
/// research 100%. Rejections skip silently, matching the train consumer.
pub fn consume_research_commands(
    mut queue: ResMut<ResearchCommands>,
    table: Res<ResearchTable>,
    mut state: ResMut<ResearchState>,
    mut res: ResMut<Resources>,
    pop: Res<Population>,
    mut changed: MessageWriter<EconChanged>,
) {
    while let Some(cmd) = queue.0.pop_front() {
        match cmd {
            ResearchCommand::Start { id } => {
                let Some(def) = table.0.get(&id) else { continue };
                if state.entries.len() >= state.cap as usize {
                    continue; // column busy — no charge (WC3 reject)
                }
                if res.gold < def.cost_gold {
                    continue;
                }
                res.gold -= def.cost_gold;
                let total = def.time.max(0.0);
                state.entries.push_back(QueueEntry { what: id, remaining: total, total });
                changed.write(econ_changed(&res, &pop));
            }
            ResearchCommand::Cancel => {
                // 100% refund regardless of elapsed progress.
                let Some(entry) = state.entries.pop_front() else { continue };
                if let Some(def) = table.0.get(&entry.what) {
                    res.gold += def.cost_gold;
                    changed.write(econ_changed(&res, &pop));
                }
            }
        }
    }
}

/// Advance the research column head (PRD2). Completion applies every grant:
/// `Stat` overwrites the (unit, kind) slot in [`TypeModifiers`] (pinned
/// overwrite semantics, see [`ResearchSource`]), `UnlockBuild`/`UnlockSkill`
/// insert into [`Unlocks`]. Exactly one `SfxRequest { ResearchComplete }`
/// is emitted per completed research — the owning-domain Ack contract
/// (sfx module header), written directly rather than through a throttle.
pub fn advance_research(
    time: Res<Time>,
    table: Res<ResearchTable>,
    mut state: ResMut<ResearchState>,
    mut type_mods: ResMut<TypeModifiers>,
    mut unlocks: ResMut<Unlocks>,
    mut sfx: MessageWriter<SfxRequest>,
) {
    if state.entries.is_empty() {
        return;
    }
    let dt = time.delta_secs();
    if let Some(head) = state.entries.front_mut() {
        head.remaining -= dt;
    }
    loop {
        let Some(head) = state.entries.front() else { break };
        if head.remaining > 0.0 {
            break;
        }
        let finished = state.entries.pop_front().expect("front checked above");
        if let Some(def) = table.0.get(&finished.what) {
            for grant in &def.grants {
                match grant {
                    ResearchGrant::Stat { unit, kind, amount } => {
                        type_mods.0.insert((unit.clone(), *kind), *amount);
                    }
                    ResearchGrant::UnlockBuild(id) => {
                        unlocks.builds.insert(id.clone());
                    }
                    ResearchGrant::UnlockSkill(id) => {
                        unlocks.skills.insert(id.clone());
                    }
                }
            }
        }
        sfx.write(SfxRequest { kind: SfxKind::ResearchComplete });
    }
}

/// Keep every unit's `StatMods` in sync with [`TypeModifiers`] (PRD2): for
/// each unit whose def hits the table, upsert one `ModEntry` sourced from
/// the [`ResearchSource`] marker. Runs every frame, so freshly spawned
/// units inherit completed research on their first tick, and upserts are
/// idempotent. The per-frame fold into `EffectiveStats` stays in buff
/// (`aggregate_stats`, untouched) — this system only feeds it. Empty table
/// (maps without research) exits before touching a single unit: zero drift.
///
/// D6 note: the table is a HashMap but is only probed per (unit, kind)
/// key — iteration order cannot change the outcome (each upsert targets a
/// distinct (source, kind) slot), so no BTreeMap is required.
pub fn apply_research_stats(
    source: Res<ResearchSource>,
    table: Res<TypeModifiers>,
    mut units: Query<(&UnitInstance, &mut StatMods)>,
) {
    if table.0.is_empty() {
        return;
    }
    for (instance, mut mods) in units.iter_mut() {
        for ((def, kind), amount) in table.0.iter() {
            if def == &instance.def {
                mods.upsert(ModEntry {
                    source: source.0,
                    kind: *kind,
                    amount: *amount,
                    expires_at: None,
                    from_aura: false,
                });
            }
        }
    }
}

/// Domain systems (registered once by WarbasePlugin; PRD increments add
/// systems HERE, never in lib.rs).
pub struct ProductionPlugin;

impl Plugin for ProductionPlugin {
    fn build(&self, app: &mut App) {
        // The research marker entity (PRD2): one bare, never-despawned
        // entity standing in as the `ModEntry::source` of every research
        // stat grant. Spawned here (plugin build) so it exists before any
        // system reads `ResearchSource`.
        let marker = app.world_mut().spawn_empty().id();
        app.insert_resource(ResearchSource(marker))
            .init_resource::<TrainTable>()
            .init_resource::<TrainTimes>()
            .init_resource::<ResearchTable>()
            .init_resource::<ResearchState>()
            .init_resource::<ResearchCommands>()
            .init_resource::<RallyCommands>()
            .init_resource::<TypeModifiers>()
            .init_resource::<Unlocks>()
            .add_systems(
                Update,
                (
                    attach_production_queues,
                    consume_train_commands,
                    consume_rally_commands,
                    consume_research_commands,
                    advance_production_queues,
                    advance_research,
                    apply_research_stats,
                )
                    .chain()
                    // After the econ command stream settles, so same-frame
                    // bounty income is visible to train validations; after
                    // the aura rebuilds and before buff::aggregate_stats
                    // folds, so research stat entries are visible in the
                    // SAME frame's EffectiveStats (non-aura entries are
                    // disjoint from the aura rebuild — order between the
                    // two writers does not matter, the before/after pins
                    // only the fold).
                    .after(crate::econ::apply_econ)
                    .after(crate::buff::apply_follower_auras)
                    .before(crate::buff::aggregate_stats)
                    .in_set(crate::WarbaseSet::Tick),
            );
    }
}

pub mod prelude {
    pub use super::{
        advance_research, apply_research_stats, consume_rally_commands, consume_research_commands,
        is_build_unlocked, is_skill_unlocked, CancelTrain, ProductionDef, ProductionPlugin,
        ProductionQueue, QueueEntry, RallyCommand, RallyCommands, RallyPoint, ResearchCommand,
        ResearchCommands, ResearchDef, ResearchGrant, ResearchSource, ResearchState, ResearchTable,
        TrainCommand, TrainTable, TrainTimes, TypeModifiers, Unlocks, DEFAULT_QUEUE_CAP,
        DEFAULT_TRAIN_TIME, SPAWN_OFFSET,
    };
    // Frozen-lib escape hatch (MP2): rule::MatchWinner cannot be added to
    // the crate prelude without editing lib.rs (plan D3 freezes it for
    // parallel domain increments), and this module's prelude IS crate-
    // prelude-reachable via lib.rs's wildcard `pub use
    // crate::production::prelude::*`. Re-exported here so MP2 consumers
    // (tests, future FOG2/UI) can read the structured winner. Lift into
    // rule's own prelude line at the next SEAM revision.
    pub use crate::rule::MatchWinner;
}
