//! RTS camera skeleton (SEAM2d, ADR-0018 Phase-1 CAM seam): edge-scroll
//! parameters + the minimap ping message. Camera panning/zoom/F1-focus
//! systems land with the CAM increment (they are game-UI layering on top
//! of the existing input seams); the vocabulary is pinned here so lib.rs
//! registration is complete before the freeze.

use bevy::prelude::*;

use crate::unit::TeamId;

/// Edge-scroll parameters (resource; a speed of 0.0 — the default —
/// disables the behavior, matching pre-SEAM2d camera-less apps exactly).
#[derive(Resource, Debug, Clone, Copy, PartialEq)]
pub struct EdgeScroll {
    /// Camera travel speed while the cursor rides a window edge
    /// (pixels of window per second of travel at 1:1 mapping).
    pub speed_px_per_sec: f32,
    /// Dead-zone width from the window border that counts as "on the
    /// edge".
    pub margin_px: f32,
}

impl Default for EdgeScroll {
    fn default() -> Self {
        Self { speed_px_per_sec: 0.0, margin_px: 0.0 }
    }
}

/// A minimap ping (alert marker at ground `pos`, attributed to `team`).
#[derive(Message, Debug, Clone, Copy)]
pub struct MinimapPing {
    pub pos: Vec2,
    pub team: TeamId,
}

/// Domain plugin (SEAM2d skeleton; systems land with CAM).
pub struct CameraRtsPlugin;

impl Plugin for CameraRtsPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<MinimapPing>().init_resource::<EdgeScroll>();
    }
}

pub mod prelude {
    pub use super::{CameraRtsPlugin, EdgeScroll, MinimapPing};
}
