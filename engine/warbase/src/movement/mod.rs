//! Movement (WC3 alignment, ADR-0016): free-roaming agents for commanded
//! units — the counterpart of `path` (track followers for wave creeps).
//! Agent orders are consumed in Tick (MOV4 `tick_orders`), locomotion and
//! separation advance in Director (MOV1/3, beside `path::advance_followers`).
//! Grid A* ships with a per-frame budget + dirty-recompute on grid changes.
//!
//! SEAM0b ships the pinned types only; systems land with MOV1+ per the
//! alignment plan. `MoveAgent` and `PathFollower` are mutually exclusive on
//! an entity (debug_assert) — track creeps and free agents are two worlds.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// A commanded unit's current order (WC3 command set; `Smart` is resolved
/// into a concrete variant by selection::smart_resolve before it reaches
/// movement). Shared vocabulary between SEL4 and MOV — pinned here so both
/// domains build against it independently.
#[derive(Debug, Clone, PartialEq)]
pub enum Order {
    /// Walk to a point; ignore everything.
    Move { to: Vec3 },
    /// Walk to a point, engaging any hostile in weapon range on the way
    /// (A-key). Resume the original route after the target dies.
    AttackMove { to: Vec3 },
    /// Chase one entity into range and fire (right-click on a hostile).
    AttackTarget { target: Entity },
    /// Stand ground: acquire targets in range but never chase (H-key).
    HoldPosition,
    /// Clear the order and stop in place (S-key).
    Stop,
    /// Patrol between two points with attack-move semantics (P-key).
    Patrol { a: Vec3, b: Vec3, toward_b: bool },
    /// Walk onto a ground item and pick it up (right-click on an item).
    Pickup { item: Entity },
    /// Keep a friendly entity's flank (right-click on a friendly).
    Follow { target: Entity },
}

/// Player-facing command broadcast: one order for a set of selected units
/// (formation offsets are assigned by MOV5 at consumption). Consumed in Tick.
#[derive(Message, Debug)]
pub struct UnitCommand {
    pub order: Order,
    pub subjects: Vec<Entity>,
}

/// Free-roaming locomotion component (NOT for wave creeps on tracks).
/// MOV1 systems land here.
#[derive(Component, Debug)]
pub struct MoveAgent {
    pub velocity: Vec3,
    pub arrival_radius: f32,
}

/// Execution sub-state of the current order. MOV4 lands here.
#[derive(Component, Debug, Default, PartialEq)]
pub enum AgentOrderState {
    #[default]
    Idle,
    Moving,
    /// Attack-move pause: engaged with a target, route resumes after.
    Engaging,
    Chasing,
}

/// Navigation grid: same cell size as the build grid; blockers are occupied
/// buildables + authored scenery. MOV2 lands here.
#[derive(Resource, Debug, Default, Clone)]
pub struct NavGrid {
    pub cell: f32,
    /// Blocked cells in grid coordinates (deterministic order: BTreeSet).
    pub blocked: std::collections::BTreeSet<(i32, i32)>,
}

/// Map-bundle nav section (SEAM0d; optional, old maps default empty).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NavDef {
    #[serde(default)]
    pub blocked: Vec<[i32; 2]>,
    /// A* calls allowed per frame (MOV2 budget; WC3-style throttling).
    #[serde(default = "default_budget")]
    pub budget_per_frame: usize,
}

fn default_budget() -> usize {
    8
}

/// Bundle dispatch stub (SEAM0d): MOV2 fills this in; never edit
/// bundle/mod.rs for nav content.
pub fn apply_nav(world: &mut World, bundle: &crate::bundle::MapBundle) {
    let _ = (world, bundle);
    // TODO(MOV2): seed NavGrid from WorldDef build domain + NavDef.blocked.
}

pub mod prelude {
    pub use super::{AgentOrderState, MoveAgent, NavDef, NavGrid, Order, UnitCommand};
}
