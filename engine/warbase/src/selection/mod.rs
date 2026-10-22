//! Selection (WC3 alignment, ADR-0016): entity selection sets, control
//! groups, and world picking. Logic is command-driven and fully headless —
//! the input layer (input module, ui feature) translates mouse/keyboard into
//! the messages defined here; tests inject messages directly.
//!
//! Scheduling (D2): `apply_group_commands` -> `apply_select_commands` ->
//! `cycle_subgroups` -> `fill_panel_data` run chained in Tick (group recalls
//! emit `SelectCommand::Set` consumed the same frame); `sweep_dead_selection`
//! runs in Cleanup and drops despawned entities from the selection and all
//! control groups. Smart right-click resolution (SEL4) runs before Intent.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::unit::{Hp, MaxHp, UnitId, UnitInstance, UnitRegistry};

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
/// — SEL1b) and the build-grid cell under it, if any.
#[derive(Message, Debug, Clone)]
pub struct PickResolved {
    pub world: Vec2,
    pub entities: Vec<Entity>,
    pub cell: Option<(i32, i32)>,
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

/// GroupCommand slot index (groups are 1..=9); None = out of range, ignored.
fn group_slot(digit: u8) -> Option<usize> {
    (1..=9).contains(&digit).then_some(digit as usize - 1)
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
                            transform.translation.truncate(),
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
        app.add_systems(
            Update,
            (
                apply_group_commands,
                apply_select_commands,
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
        AABB_EDGE_EPSILON, ControlGroups, CycleSubgroups, GroupCommand, PickAttempt, PickButton,
        PickResolved, SelectCommand, Selectable, Selected, SelectionDetail, SelectionPanelData,
        SelectionPlugin, SelectionState, selection_subgroups,
    };
}
