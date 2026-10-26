//! Selection (WC3 alignment, ADR-0016): entity selection sets, control
//! groups, and world picking. Logic is command-driven and fully headless —
//! the input layer (input module, ui feature) translates mouse/keyboard into
//! the messages defined here; tests inject messages directly.
//!
//! PLANE CONTRACT (hole-1 fix): the gameplay plane is **y = 0 (XZ)** and
//! every `Vec2` below means ground `(x, z)` — bodies hit-test on
//! `(translation.x, translation.z)`, pick points construct
//! `Vec3::new(x, 0.0, z)`. Same convention as movement/nav/build/combat.
//!
//! Scheduling (D2): `apply_group_commands` -> `apply_select_commands` ->
//! `resolve_picks` -> `apply_point_selects` -> `apply_command_hotkeys` ->
//! `issue_targeted_orders` -> `smart_resolve` -> `cycle_subgroups` ->
//! `fill_panel_data` run chained in Tick (group recalls emit
//! `SelectCommand::Set` consumed the same frame, so pick resolution and
//! smart commands see the fresh selection, still before Intent; point
//! selects emit `SelectCommand`s consumed the NEXT frame — the select
//! applier already ran this frame, see `apply_point_selects`);
//! `sweep_dead_selection` runs in Cleanup and drops despawned entities from
//! the selection and all control groups.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::build::{BuildGrid, BuildMode};
use crate::movement::{Order, UnitCommand};
use crate::unit::{Allegiance, Hostility, Hp, MaxHp, UnitId, UnitInstance, UnitRegistry};

/// Boundary tolerance for box-select hit tests: a body disc whose edge is
/// within this distance of the rect still counts as hit (pinned by spec).
pub const AABB_EDGE_EPSILON: f32 = 1e-3;

/// Declares an entity player-selectable (towers, free agents, shops...).
#[derive(Component, Debug)]
pub struct Selectable;

/// Present while the entity is part of the current selection set.
#[derive(Component, Debug)]
pub struct Selected;

/// Which mouse button produced a pick (game-agnostic, serde-friendly).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PickButton {
    Left,
    Right,
    Middle,
}

/// Input-layer pick attempt: a world-space pointer event (the input module
/// converts screen -> world; headless tests write these directly).
#[derive(Message, Debug, Clone, Copy)]
pub struct PickAttempt {
    pub world: Vec2,
    pub button: PickButton,
    /// 1 = single click, 2 = double click (select-same-type).
    pub click_count: u8,
    pub ctrl: bool,
    pub shift: bool,
}

/// Resolved pick: entities under the point (body_radius hit, nearest first
/// — SEL1b) and the build-grid cell under it, if any. Carries the
/// originating button so `smart_resolve` can filter right-clicks.
#[derive(Message, Debug, Clone)]
pub struct PickResolved {
    pub world: Vec2,
    pub button: PickButton,
    pub entities: Vec<Entity>,
    pub cell: Option<(i32, i32)>,
}

/// Keyboard command intents (SEL4): the ui-feature input layer translates
/// raw A/H/S/P/Esc presses into these; the state machine consuming them
/// stays pure logic here (headless tests write them directly).
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandHotkey {
    /// A: arm AttackMove targeting — the next left click is the target point.
    AttackMove,
    /// H: HoldPosition for the current selection, immediately.
    Hold,
    /// S: Stop for the current selection, immediately.
    Stop,
    /// P: arm Patrol targeting — two left clicks (point a, then point b).
    Patrol,
    /// Esc: cancel any pending targeting state (never touches the selection
    /// set itself — Clear owns that, SEL2).
    Cancel,
}

/// Modal command-targeting cursor state (A/P keys; SEL4). Lives in the
/// selection domain — not the ui input module — so headless tests can drive
/// the two-click state machine through `CommandHotkey` + `PickAttempt`.
#[derive(Resource, Debug, Default, Clone, Copy, PartialEq)]
pub enum PendingTargetOrder {
    /// Normal cursor: left clicks do not target commands.
    #[default]
    Idle,
    /// A pressed: the next left click issues AttackMove at that point.
    AttackMove,
    /// P pressed: waiting for the first click (point a).
    PatrolFirst,
    /// Point a captured: waiting for the second click (point b).
    PatrolSecond { a: Vec3 },
}

/// Selection set mutations. `SameTypeOnScreen` is resolved by the input
/// layer (it owns screen space) into `Set` before reaching here.
#[derive(Message, Debug, Clone)]
pub enum SelectCommand {
    Set(Vec<Entity>),
    Add(Entity),
    Toggle(Entity),
    /// Box select: world-space AABB (drag select, SEL3). Bodies whose discs
    /// touch the rect are selected (see `AABB_EDGE_EPSILON`); replaces the
    /// selection set.
    Aabb { min: Vec2, max: Vec2 },
    Clear,
}

/// The current selection set. `entities` is kept sorted by `Entity::index()`
/// ascending (determinism discipline D6 — never iterate an unordered set).
#[derive(Resource, Debug, Default)]
pub struct SelectionState {
    pub entities: Vec<Entity>,
    /// Tab-cursor into the UnitId-partitioned subgroups (SEL2).
    pub subgroup_cursor: usize,
}

/// WC3 control groups: Ctrl+<digit> binds, <digit> recalls, Shift appends.
#[derive(Message, Debug, Clone, Copy)]
pub enum GroupCommand {
    Bind(u8),
    Recall(u8),
    Insert(u8),
}

/// Modal keyboard claim (hole-8 upcast, SEAM2b): while `Some(owner)`, the
/// ui-feature keyboard bridge SKIPS its digit -> `GroupCommand` translation
/// (digits belong to the modal UI — build placement, skill targeting,
/// inventory hotbars); Tab and the letter hotkeys (A/H/S/P/M/Esc) stay
/// live. CONTRACT: the module that opens a modal writes
/// `ModalKeyClaim(Some("its id"))` and MUST clear it back to `None` on
/// close (cancel, confirm, or despawn) — the base never releases a claim
/// on anyone's behalf. Stashing the previous value on claim and restoring
/// it on release keeps nested modals well-behaved. Headless logic never
/// reads this (domain messages are injected directly); it exists purely as
/// the bridge's yield seam, so tests drive it through the keyboard bridge.
#[derive(Resource, Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ModalKeyClaim(pub Option<&'static str>);

/// Control-group storage (keys 1..=9; dead entities are swept in Cleanup).
#[derive(Resource, Debug, Default)]
pub struct ControlGroups {
    pub groups: [Vec<Entity>; 9],
}

/// Tab: advance the subgroup cursor through the def-id partition of the
/// current selection (input layer pins Tab to this message).
#[derive(Message, Debug, Clone, Copy)]
pub struct CycleSubgroups;

/// Data-layer projection of the selection for UI panels (SEL3 fills the
/// content; rendering stays in the ui feature).
#[derive(Resource, Debug, Default)]
pub struct SelectionPanelData {
    pub names: Vec<String>,
    /// Aggregate or per-unit readouts, when a single unit is selected.
    pub detail: Option<SelectionDetail>,
}

#[derive(Debug, Default, Clone)]
pub struct SelectionDetail {
    pub hp: Option<(f32, f32)>,
    pub mana: Option<(f32, f32)>,
    pub skills: Vec<String>,
    pub items: Vec<String>,
}

// ---------------------------------------------------------------------------
// Pure helpers
// ---------------------------------------------------------------------------

/// Sort ascending by `Entity::index()` and drop duplicates (D6 discipline).
fn sorted_dedup(mut entities: Vec<Entity>) -> Vec<Entity> {
    entities.sort_by_key(|e| e.index());
    entities.dedup();
    entities
}

/// Ground-plane position of a transform: `(x, z)` (hole-1 unified base
/// convention — the world plays on y = 0; the old `truncate()` read
/// `(x, y)` and collapsed XZ-plane units onto the x axis).
fn ground_pos(translation: Vec3) -> Vec2 {
    Vec2::new(translation.x, translation.z)
}

/// World point on the gameplay plane for a ground `(x, z)` pair:
/// `Vec3::new(x, 0.0, z)` (hole-1; the old `extend(0.0)` produced
/// `(x, y, 0)` — off the ground).
fn ground_vec(p: Vec2) -> Vec3 {
    Vec3::new(p.x, 0.0, p.y)
}

/// Body radius of an instance from its def; 0.0 when the entity carries no
/// UnitInstance/def (point hit) — defensive, selections are def-driven.
fn body_radius(instance: Option<&UnitInstance>, registry: &UnitRegistry) -> f32 {
    instance
        .and_then(|i| registry.get(&i.def))
        .map_or(0.0, |d| d.attrs.body_radius)
}

/// Circle-vs-rect hit test with the pinned edge epsilon: closest point on
/// the rect to the disc center, compared against radius + epsilon.
fn disc_hits_rect(center: Vec2, radius: f32, min: Vec2, max: Vec2) -> bool {
    let closest = Vec2::new(
        center.x.clamp(min.x, max.x),
        center.y.clamp(min.y, max.y),
    );
    (center - closest).length_squared() <= (radius + AABB_EDGE_EPSILON).powi(2)
}

/// Selectable bodies whose disc contains the ground point (the pinned SEL1b
/// rule, hole-1 XZ geometry): `body_radius + edge epsilon`, nearest first,
/// equal distances broken by Entity index (D6). Shared by `resolve_picks`
/// and `apply_point_selects` so point select and pick resolution can never
/// disagree about what a click hit.
fn point_hits(
    bodies: &Query<(Entity, &Transform, Option<&UnitInstance>), With<Selectable>>,
    registry: &UnitRegistry,
    point: Vec2,
) -> Vec<Entity> {
    let mut hits: Vec<(f32, Entity)> = bodies
        .iter()
        .filter_map(|(entity, transform, instance)| {
            let radius = body_radius(instance, registry);
            let dist = ground_pos(transform.translation).distance(point);
            (dist <= radius + AABB_EDGE_EPSILON).then_some((dist, entity))
        })
        .collect();
    // Distance ascending (total_cmp: IEEE-total, NaN-safe); ties fall back
    // to Entity index so the order is fully deterministic.
    hits.sort_by(|a, b| a.0.total_cmp(&b.0).then_with(|| a.1.index().cmp(&b.1.index())));
    hits.into_iter().map(|(_, entity)| entity).collect()
}

/// GroupCommand slot index (groups are 1..=9); None = out of range, ignored.
fn group_slot(digit: u8) -> Option<usize> {
    (1..=9).contains(&digit).then_some(digit as usize - 1)
}

/// Build-grid cell under a world point, if the grid covers it. FixedSlots
/// maps carry no cell lattice (slots are authored world points), so only
/// FreeGrid resolves a cell; fixed-slot games snap to slots in their own
/// layer. Guards `cell_size <= 0` (FixedSlots default) against the
/// division in `world_to_cell`.
fn cell_under(grid: &BuildGrid, world: Vec2) -> Option<(i32, i32)> {
    if grid.mode != BuildMode::FreeGrid || grid.cell_size <= 0.0 {
        return None;
    }
    let cell = grid.world_to_cell(world);
    (cell.0 >= 0 && cell.1 >= 0 && cell.0 < grid.cols && cell.1 < grid.rows)
        .then_some((cell.0, cell.1))
}

/// Def-id partition of a selection from a system query (sorted, deduped).
fn subgroup_keys(state: &SelectionState, instances: &Query<&UnitInstance>) -> Vec<UnitId> {
    let mut keys: Vec<UnitId> = state
        .entities
        .iter()
        .map(|e| {
            instances
                .get(*e)
                .map_or(String::new(), |i| i.def.clone())
        })
        .collect();
    keys.sort();
    keys.dedup();
    keys
}

/// Read-only subgroup partition of the current selection for tests/UI: the
/// def ids present, ascending (Tab cycles buckets in this order).
pub fn selection_subgroups(world: &World, state: &SelectionState) -> Vec<UnitId> {
    let mut keys: Vec<UnitId> = state
        .entities
        .iter()
        .map(|e| {
            world
                .get::<UnitInstance>(*e)
                .map_or(String::new(), |i| i.def.clone())
        })
        .collect();
    keys.sort();
    keys.dedup();
    keys
}

// ---------------------------------------------------------------------------
// Systems (Tick chain: groups -> select -> cycle -> panel)
// ---------------------------------------------------------------------------

/// Consumes `GroupCommand`s (Tick). Recall re-emits `SelectCommand::Set`,
/// consumed the same frame because this runs before `apply_select_commands`.
/// Recall does not pre-filter dead entries — Set's alive/Selectable filter
/// and the Cleanup sweep keep groups and selection consistent.
fn apply_group_commands(
    mut commands: MessageReader<GroupCommand>,
    state: Res<SelectionState>,
    mut groups: ResMut<ControlGroups>,
    mut select: MessageWriter<SelectCommand>,
) {
    for command in commands.read() {
        let digit = match *command {
            GroupCommand::Bind(d) | GroupCommand::Recall(d) | GroupCommand::Insert(d) => d,
        };
        let Some(slot) = group_slot(digit) else {
            continue;
        };
        match *command {
            // WC3: binding overwrites the slot with the current selection;
            // binding an empty selection clears it.
            GroupCommand::Bind(_) => groups.groups[slot] = state.entities.clone(),
            GroupCommand::Recall(_) => {
                select.write(SelectCommand::Set(groups.groups[slot].clone()));
            }
            // Shift+digit: union of slot and selection, deduped.
            GroupCommand::Insert(_) => {
                let mut merged = std::mem::take(&mut groups.groups[slot]);
                merged.extend_from_slice(&state.entities);
                groups.groups[slot] = sorted_dedup(merged);
            }
        }
    }
}

/// Consumes `SelectCommand`s (Tick): maintains `SelectionState.entities`
/// sorted by Entity index, deduped, and filtered to alive Selectable
/// entities; keeps the `Selected` marker strictly in sync.
fn apply_select_commands(
    mut commands: MessageReader<SelectCommand>,
    mut state: ResMut<SelectionState>,
    mut entity_commands: Commands,
    marked: Query<Entity, With<Selected>>,
    bodies: Query<(Entity, &Transform, Option<&UnitInstance>), With<Selectable>>,
    registry: Res<UnitRegistry>,
) {
    let mut changed = false;
    for command in commands.read() {
        match command.clone() {
            SelectCommand::Set(list) => {
                state.entities = list;
                changed = true;
            }
            SelectCommand::Add(entity) => {
                state.entities.push(entity);
                changed = true;
            }
            SelectCommand::Toggle(entity) => {
                if let Some(pos) = state.entities.iter().position(|e| *e == entity) {
                    state.entities.remove(pos);
                } else {
                    state.entities.push(entity);
                }
                changed = true;
            }
            SelectCommand::Aabb { min, max } => {
                let mut hits: Vec<Entity> = bodies
                    .iter()
                    .filter(|(_, transform, instance)| {
                        disc_hits_rect(
                            ground_pos(transform.translation),
                            body_radius(*instance, &registry),
                            min,
                            max,
                        )
                    })
                    .map(|(entity, _, _)| entity)
                    .collect();
                hits.sort_by_key(|e| e.index());
                state.entities = hits;
                changed = true;
            }
            SelectCommand::Clear => {
                state.entities.clear();
                changed = true;
            }
        }
    }
    if !changed {
        return;
    }
    // Normalize: alive + Selectable only, index-ascending, deduped; a fresh
    // selection restarts the Tab cursor.
    state.entities.retain(|e| bodies.get(*e).is_ok());
    state.entities = sorted_dedup(std::mem::take(&mut state.entities));
    state.subgroup_cursor = 0;
    // Sync the marker: remove from entities that left, insert for newcomers.
    let current: Vec<Entity> = marked.iter().collect();
    for entity in &current {
        if !state.entities.contains(entity) {
            entity_commands.entity(*entity).remove::<Selected>();
        }
    }
    for entity in &state.entities {
        if !current.contains(entity) {
            entity_commands.entity(*entity).insert(Selected);
        }
    }
}

/// Consumes `PickAttempt`s (Tick, after selection settles): resolves each
/// ground pick into a `PickResolved` — Selectable bodies whose disc contains
/// the point (`body_radius` + edge epsilon, the same pinned rule as box
/// select), nearest first with equal distances broken by Entity index (D6),
/// plus the build-grid cell under the point when a FreeGrid `BuildGrid`
/// resource is present (Option-gated: gridless apps keep working). Hit
/// geometry is the XZ ground plane (hole-1).
fn resolve_picks(
    mut attempts: MessageReader<PickAttempt>,
    mut resolved: MessageWriter<PickResolved>,
    bodies: Query<(Entity, &Transform, Option<&UnitInstance>), With<Selectable>>,
    registry: Res<UnitRegistry>,
    grid: Option<Res<BuildGrid>>,
) {
    for attempt in attempts.read() {
        resolved.write(PickResolved {
            world: attempt.world,
            button: attempt.button,
            entities: point_hits(&bodies, &registry, attempt.world),
            cell: grid.as_deref().and_then(|g| cell_under(g, attempt.world)),
        });
    }
}

/// Consumes left-click `PickAttempt`s (Tick, after `resolve_picks`): the
/// WC3 point-select "last hop" (hole-2 upcast from rts-lab's game-side
/// bridge) — a single left click on a body selects it, a click on open
/// ground clears. Pinned semantics (matching the incubated patch):
/// - plain hit  -> `SelectCommand::Set` (first hit only);
/// - shift+hit  -> `Add`; ctrl+hit -> `Toggle` (both modifiers -> Toggle);
/// - open ground -> `Clear`, except shift+ground keeps the selection;
/// - while a `PendingTargetOrder` is armed the click targets the ORDER
///   (see `issue_targeted_orders`) and never touches the selection;
/// - double clicks (click_count > 1, select-same-type) are the input
///   layer's business (it resolves them to `Set` directly, SEL2).
/// Scheduling note: this runs after `apply_select_commands`, so the emitted
/// `SelectCommand` is consumed the NEXT frame — one frame of latency, same
/// as real input (the bridge writes `PickAttempt`s before Tick anyway).
fn apply_point_selects(
    mut attempts: MessageReader<PickAttempt>,
    bodies: Query<(Entity, &Transform, Option<&UnitInstance>), With<Selectable>>,
    registry: Res<UnitRegistry>,
    pending: Res<PendingTargetOrder>,
    mut select: MessageWriter<SelectCommand>,
) {
    if *pending != PendingTargetOrder::Idle {
        return; // A/P targeting owns left clicks while armed
    }
    for attempt in attempts.read() {
        if attempt.button != PickButton::Left || attempt.click_count != 1 {
            continue;
        }
        let hits = point_hits(&bodies, &registry, attempt.world);
        match (hits.first(), attempt.ctrl, attempt.shift) {
            (Some(&first), false, false) => {
                select.write(SelectCommand::Set(vec![first]));
            }
            (Some(&first), false, true) => {
                select.write(SelectCommand::Add(first));
            }
            (Some(&first), true, _) => {
                select.write(SelectCommand::Toggle(first));
            }
            // Shift+ground keeps the current selection (WC3 habit: shift
            // only ever adds/toggles, never clears).
            (None, _, true) => {}
            (None, _, _) => {
                select.write(SelectCommand::Clear);
            }
        }
    }
}

/// Consumes `CommandHotkey`s (Tick, after selection settles): H/S fire their
/// order for the current selection at once; A/P arm the targeting cursor
/// (`PendingTargetOrder`); Esc clears the pending state only — the selection
/// set belongs to `SelectCommand::Clear` (SEL2), never to Esc. An empty
/// selection makes every targeting key a no-op (WC3 semantics).
fn apply_command_hotkeys(
    mut hotkeys: MessageReader<CommandHotkey>,
    state: Res<SelectionState>,
    mut pending: ResMut<PendingTargetOrder>,
    mut commands: MessageWriter<UnitCommand>,
) {
    for hotkey in hotkeys.read() {
        match *hotkey {
            CommandHotkey::Cancel => *pending = PendingTargetOrder::Idle,
            CommandHotkey::AttackMove if !state.entities.is_empty() => {
                *pending = PendingTargetOrder::AttackMove;
            }
            CommandHotkey::Patrol if !state.entities.is_empty() => {
                *pending = PendingTargetOrder::PatrolFirst;
            }
            CommandHotkey::Hold if !state.entities.is_empty() => {
                commands.write(UnitCommand {
                    order: Order::HoldPosition,
                    subjects: state.entities.clone(),
                });
            }
            CommandHotkey::Stop if !state.entities.is_empty() => {
                commands.write(UnitCommand {
                    order: Order::Stop,
                    subjects: state.entities.clone(),
                });
            }
            _ => {}
        }
    }
}

/// Consumes left-click `PickAttempt`s while a `PendingTargetOrder` is armed
/// (Tick): the click targets the pending order instead of selecting — A
/// fires AttackMove at the point; P is a two-click machine (first parks a,
/// second fires Patrol{a, b}); Esc (see `apply_command_hotkeys`) cancels.
/// A selection emptied between arming and clicking disarms the cursor
/// instead of issuing an empty command.
fn issue_targeted_orders(
    mut attempts: MessageReader<PickAttempt>,
    state: Res<SelectionState>,
    mut pending: ResMut<PendingTargetOrder>,
    mut commands: MessageWriter<UnitCommand>,
) {
    for attempt in attempts.read() {
        if *pending == PendingTargetOrder::Idle || attempt.button != PickButton::Left {
            continue;
        }
        if state.entities.is_empty() {
            *pending = PendingTargetOrder::Idle;
            continue;
        }
        let to = ground_vec(attempt.world); // gameplay plane is y = 0 (XZ)
        *pending = match *pending {
            PendingTargetOrder::Idle => continue,
            PendingTargetOrder::AttackMove => {
                commands.write(UnitCommand {
                    order: Order::AttackMove { to },
                    subjects: state.entities.clone(),
                });
                PendingTargetOrder::Idle
            }
            PendingTargetOrder::PatrolFirst => PendingTargetOrder::PatrolSecond { a: to },
            PendingTargetOrder::PatrolSecond { a } => {
                commands.write(UnitCommand {
                    order: Order::Patrol { a, b: to, toward_b: true },
                    subjects: state.entities.clone(),
                });
                PendingTargetOrder::Idle
            }
        };
    }
}

/// Consumes right-click `PickResolved`s (Tick, before Intent): the WC3 smart
/// command — exactly one `UnitCommand` per right click, the whole selection
/// as subjects (formation scatter is MOV5's business at consumption). The
/// first hit decides: a hostile body -> AttackTarget; open ground -> Move to
/// the pick point. Commander side = the first subject's Allegiance
/// (selections are single-side; MP1 re-derives this via Owner/diplomacy
/// inside the `Hostility` oracle, not here).
fn smart_resolve(
    mut picks: MessageReader<PickResolved>,
    state: Res<SelectionState>,
    hostility: Res<Hostility>,
    sides: Query<&Allegiance>,
    mut commands: MessageWriter<UnitCommand>,
) {
    for pick in picks.read() {
        if pick.button != PickButton::Right || state.entities.is_empty() {
            continue;
        }
        let Some(&target) = pick.entities.first() else {
            // Open ground: walk there.
            commands.write(UnitCommand {
                order: Order::Move { to: ground_vec(pick.world) },
                subjects: state.entities.clone(),
            });
            continue;
        };
        let side = sides
            .get(state.entities[0])
            .copied()
            .unwrap_or(Allegiance::Defender);
        let target_side = sides.get(target).copied().unwrap_or(Allegiance::Defender);
        if hostility.hostile(side, target_side) {
            commands.write(UnitCommand {
                order: Order::AttackTarget { target },
                subjects: state.entities.clone(),
            });
        } else {
            // TODO(SEL4->INV): a GroundItem hit resolves to Order::Pickup
            // once ground-item entities exist in the item domain.
            // TODO(SEL4->MOV): a friendly Selectable hit resolves to
            // Order::Follow; until then friendly hits issue nothing.
        }
    }
}

/// Consumes `CycleSubgroups` (Tick, after selection settles): advances the
/// subgroup cursor one step per message through the def-id partition.
fn cycle_subgroups(
    mut cycles: MessageReader<CycleSubgroups>,
    mut state: ResMut<SelectionState>,
    instances: Query<&UnitInstance>,
) {
    let buckets = subgroup_keys(&state, &instances).len().max(1);
    for _ in cycles.read() {
        state.subgroup_cursor = (state.subgroup_cursor + 1) % buckets;
    }
}

/// Refreshes `SelectionPanelData` (Tick, after selection settles): names
/// from def ids, single-selection detail with hp and skill placeholders
/// (mana/items belong to the SPL/INV domains).
fn fill_panel_data(
    state: Res<SelectionState>,
    instances: Query<&UnitInstance>,
    hp: Query<(&Hp, &MaxHp)>,
    registry: Res<UnitRegistry>,
    mut panel: ResMut<SelectionPanelData>,
) {
    panel.names.clear();
    panel.detail = None;
    for entity in &state.entities {
        let name = instances
            .get(*entity)
            .ok()
            .and_then(|i| registry.get(&i.def))
            .map_or_else(|| "?".to_string(), |d| d.id.clone());
        panel.names.push(name);
    }
    if state.entities.len() == 1 {
        let entity = state.entities[0];
        let def = instances
            .get(entity)
            .ok()
            .and_then(|i| registry.get(&i.def));
        panel.detail = Some(SelectionDetail {
            hp: hp.get(entity).ok().map(|(h, m)| (h.0, m.0)),
            mana: None,
            skills: def.map_or_else(Vec::new, |d| d.skills.clone()),
            items: Vec::new(),
        });
    }
}

/// Cleanup: drops despawned entities from the selection and every control
/// group (alive refs only; marker components die with their entities).
fn sweep_dead_selection(world: &mut World) {
    world.resource_scope(|world, mut state: Mut<SelectionState>| {
        state.entities.retain(|e| world.entities().contains(*e));
    });
    world.resource_scope(|world, mut groups: Mut<ControlGroups>| {
        for group in &mut groups.groups {
            group.retain(|e| world.entities().contains(*e));
        }
    });
}

/// Bundle dispatch stub (SEAM0d): selection needs no map content today;
/// kept for dispatch symmetry.
pub fn apply_selection(world: &mut World, bundle: &crate::bundle::MapBundle) {
    let _ = (world, bundle);
}

/// Domain systems (registered once by WarbasePlugin; increments add systems
/// HERE, never in lib.rs).
pub struct SelectionPlugin;

impl Plugin for SelectionPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<CommandHotkey>()
            .init_resource::<PendingTargetOrder>()
            .init_resource::<ModalKeyClaim>()
            .add_systems(
                Update,
                (
                    apply_group_commands,
                    apply_select_commands,
                    resolve_picks,
                    apply_point_selects,
                    apply_command_hotkeys,
                    issue_targeted_orders,
                    smart_resolve,
                    cycle_subgroups,
                    fill_panel_data,
                )
                    .chain()
                    .in_set(crate::WarbaseSet::Tick),
            )
            .add_systems(Update, sweep_dead_selection.in_set(crate::WarbaseSet::Cleanup));
    }
}

pub mod prelude {
    pub use super::{
        AABB_EDGE_EPSILON, CommandHotkey, ControlGroups, CycleSubgroups, GroupCommand,
        ModalKeyClaim, PendingTargetOrder, PickAttempt, PickButton, PickResolved, SelectCommand,
        Selectable, Selected, SelectionDetail, SelectionPanelData, SelectionPlugin, SelectionState,
        selection_subgroups,
    };
}
