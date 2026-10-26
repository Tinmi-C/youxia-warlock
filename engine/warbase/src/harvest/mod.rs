//! Harvest skeleton (SEAM2d, ADR-0018 Phase-1 HARV seam): resource
//! gathering types + map-bundle section, pinned now so the parallel lines
//! build against them while lib.rs/bundle freeze. Mechanisms (load/decay/
//! deposit cycles, upkeep) land with the HARV increment — this module only
//! nails the vocabulary. No game concepts, no numbers (ADR-0012): every
//! value ships in the map bundle.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// Map-bundle harvest section: the resource deposits on the map.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct HarvestDef {
    #[serde(default)]
    pub deposits: Vec<MineDef>,
}

/// One authored resource deposit (a gold mine / lumber patch; the id names
/// the resource kind, content stays game-side).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MineDef {
    pub id: String,
    /// Ground (x, z) — hole-1 XZ convention.
    pub pos: [f32; 2],
    /// Extractable units before depletion (HARV decides the per-trip cut).
    #[serde(default)]
    pub capacity: u32,
}

/// Runtime state of a live deposit (attached by the HARV increment).
#[derive(Component, Debug)]
pub struct Mine {
    pub deposit: String,
    pub remaining: u32,
}

/// Bundle dispatch (SEAM0d symmetry): HARV consumes the section later; the
/// placeholder only proves the seam (assemble calls it unconditionally).
pub fn apply_harvest(world: &mut World, bundle: &crate::bundle::MapBundle) {
    let _ = (world, &bundle.harvest.deposits);
}

/// Domain plugin (SEAM2d skeleton; systems land with HARV).
pub struct HarvestPlugin;

impl Plugin for HarvestPlugin {
    fn build(&self, _app: &mut App) {}
}

pub mod prelude {
    pub use super::{apply_harvest, HarvestDef, HarvestPlugin, Mine, MineDef};
}
