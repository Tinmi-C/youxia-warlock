//! Input layer (WC3 alignment, ADR-0016; ui feature): converts pointer
//! events into world-space `PickAttempt`s. The logic stays in `selection`
//! (headless-testable); this module owns screen->world math and raw input
//! bridging only. SEL1b lands the camera implementations here (bevy_picking
//! spike SEL1a decides the raw-event source).

use bevy::prelude::*;

/// Screen -> world mapping seam. Games with unusual cameras swap the
/// resource; the base consumes only `PickAttempt` downstream.
pub trait WorldPicker: Send + Sync + 'static {
    /// Map a pointer position (window pixel coords) to world XY. None when
    /// the pointer is off the playable plane.
    fn screen_to_world(&self, world: &World, screen: Vec2) -> Option<Vec2>;
}

/// Test/default implementation: identity mapping (screen == world XY).
/// Handy for headless harnesses driving picks directly.
#[derive(Resource, Debug, Default)]
pub struct IdentityPicker;

impl WorldPicker for IdentityPicker {
    fn screen_to_world(&self, _world: &World, screen: Vec2) -> Option<Vec2> {
        Some(screen)
    }
}

// TODO(SEL1b): OrthoTopDownPicker (linear unprojection of the ortho camera)
// + pointer-event bridge emitting selection::PickAttempt. UI panels must
// block world picks (hit-test the ui tree first).

/// Domain systems (ui feature; registered once by WarbasePlugin).
pub struct InputPlugin;

impl Plugin for InputPlugin {
    fn build(&self, _app: &mut App) {
        // TODO(SEL1b): pointer bridge before WarbaseSet::Tick.
    }
}

pub mod prelude {
    pub use super::{IdentityPicker, InputPlugin, WorldPicker};
}
