//! Vision & fog of war (WC3 alignment, ADR-0016): two-layer grid (visible /
//! explored) fed by VisionSource carriers. Rule D5: player fog constrains
//! the interaction layer only (manual cast targets, selection, rendering) —
//! towers and autocast are unaffected (they carry their own VisionSource).
//!
//! Scheduling: grid update runs in Tick (FOG1). SEAM0b pins the types.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// Grants vision of `radius` world units around the carrier (towers, free
/// agents, buildings). Creeps may carry it for team-shared vision (MP1+).
#[derive(Component, Debug)]
pub struct VisionSource {
    pub radius: f32,
}

/// Present on entities currently outside every friendly vision source
/// (rendering dims/hides via the fx layer; interaction layers reject).
#[derive(Component, Debug)]
pub struct InFog;

/// Two-layer fog grid over the map (grid coords; explored only grows).
/// FOG1 fills the update system; cell layout matches the build grid.
#[derive(Resource, Debug, Default, Clone)]
pub struct VisionGrid {
    pub cell: f32,
    pub cols: i32,
    pub rows: i32,
    pub origin: Vec2,
    pub visible: std::collections::BTreeSet<(i32, i32)>,
    pub explored: std::collections::BTreeSet<(i32, i32)>,
}

/// Map-bundle vision section (SEAM0d; optional, old maps default).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VisionDef {
    /// Fog grid cell size in world units.
    #[serde(default = "default_cell")]
    pub cell: f32,
}

fn default_cell() -> f32 {
    1.0
}

/// Struct-level serde default (a whole missing section) must agree with the
/// field-level default above — both mean "one world unit per fog cell".
impl Default for VisionDef {
    fn default() -> Self {
        Self { cell: default_cell() }
    }
}

/// Bundle dispatch stub (SEAM0d): FOG1 fills this in.
pub fn apply_vision(world: &mut World, bundle: &crate::bundle::MapBundle) {
    let _ = (world, bundle);
    // TODO(FOG1): size VisionGrid from the build grid bounds + VisionDef.
}

pub mod prelude {
    pub use super::{InFog, VisionDef, VisionGrid, VisionSource};
}
