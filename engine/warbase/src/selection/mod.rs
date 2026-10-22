//! Selection (WC3 alignment, ADR-0016): entity selection sets, control
//! groups, and world picking. Logic is command-driven and fully headless —
//! the input layer (input module, ui feature) translates mouse/keyboard into
//! the messages defined here; tests inject messages directly.
//!
//! Scheduling: SelectCommand/GroupCommand are consumed in Tick; smart
//! right-click resolution runs before Intent (SEL4). SEAM0b pins the types.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

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
    /// Box select: world-space AABB (drag select, SEL3).
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

/// Bundle dispatch stub (SEAM0d): selection needs no map content today;
/// kept for dispatch symmetry.
pub fn apply_selection(world: &mut World, bundle: &crate::bundle::MapBundle) {
    let _ = (world, bundle);
}

/// Domain systems (registered once by WarbasePlugin; increments add systems
/// HERE, never in lib.rs).
pub struct SelectionPlugin;

impl Plugin for SelectionPlugin {
    fn build(&self, _app: &mut App) {
        // TODO(SEL2): SelectCommand/GroupCommand consumer in Tick; SEL1b
        // resolve_picks before Tick; SEL4 smart_resolve before Intent.
    }
}

pub mod prelude {
    pub use super::{
        ControlGroups, GroupCommand, PickAttempt, PickButton, PickResolved, SelectCommand,
        Selectable, Selected, SelectionDetail, SelectionPanelData, SelectionPlugin, SelectionState,
    };
}
