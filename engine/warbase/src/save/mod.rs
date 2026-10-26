//! Save/load skeleton (SEAM2d, ADR-0018 Phase-2 seam): the slot concept
//! for world-snapshot saves (a RON snapshot of the WORLD, distinct from
//! replay logs = input streams). Snapshotting lands with the Phase-2
//! increment; the slot vocabulary is pinned here so UI/game layers can
//! reference it against a frozen lib.rs.

use serde::{Deserialize, Serialize};

/// A named save slot (its string is the file/stem identity the snapshot
/// layer resolves; the base pins only the concept).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct SaveSlot(pub String);

/// Domain plugin (SEAM2d skeleton; snapshot systems land in Phase 2).
pub struct SavePlugin;

impl bevy::prelude::Plugin for SavePlugin {
    fn build(&self, _app: &mut bevy::prelude::App) {}
}

pub mod prelude {
    pub use super::{SavePlugin, SaveSlot};
}
