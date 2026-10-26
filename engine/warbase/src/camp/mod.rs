//! Neutral-camp skeleton (SEAM2d, ADR-0018 Phase-1 MAP seam): guard camps
//! (radius, respawn, drops) as a map-bundle section. Guard/pursuit/leash
//! behavior and drop rolls land with the MAP increment; the section shape
//! is pinned here.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// Map-bundle camps section.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CampsDef {
    #[serde(default)]
    pub camps: Vec<CampDef>,
}

/// One authored neutral camp.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CampDef {
    pub id: String,
    /// Camp anchor, ground (x, z).
    pub pos: [f32; 2],
    /// Guard radius: intruders inside it trigger pursuit (leash rules are
    /// the MAP increment's).
    #[serde(default)]
    pub radius: f32,
    /// Seconds after a wiped camp before it respawns.
    #[serde(default)]
    pub respawn_secs: f32,
    /// Drop table ids granted on wipe (per-map tables resolve them).
    #[serde(default)]
    pub drops: Vec<String>,
}

/// Runtime marker of a live camp anchor (attached by the MAP increment).
#[derive(Component, Debug)]
pub struct Camp {
    pub def: String,
}

/// Bundle dispatch (SEAM0d symmetry; MAP consumes it later).
pub fn apply_camps(world: &mut World, bundle: &crate::bundle::MapBundle) {
    let _ = (world, &bundle.camps.camps);
}

/// Domain plugin (SEAM2d skeleton; systems land with MAP).
pub struct CampPlugin;

impl Plugin for CampPlugin {
    fn build(&self, _app: &mut App) {}
}

pub mod prelude {
    pub use super::{apply_camps, Camp, CampDef, CampPlugin, CampsDef};
}
