//! Spectator skeleton (SEAM2d, ADR-0018 — hole-11 "spectate column" seam):
//! the spectator-mode flag set. Observer cameras / full-map vision / UI
//! shells land post-Phase-2; the concept is pinned so replay's spectate
//! column and multiplayer observers have a stable seam.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// Spectator mode parameters (resource; disabled by default — every
/// existing app is a player app, zero drift).
#[derive(Resource, Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpectateDef {
    #[serde(default)]
    pub enabled: bool,
    /// Spectated player slot label (observers may lock onto one player's
    /// view; None-style empty string = free camera).
    #[serde(default)]
    pub follow: String,
}

/// Domain plugin (SEAM2d skeleton; observer systems land later).
pub struct SpectatePlugin;

impl Plugin for SpectatePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SpectateDef>();
    }
}

pub mod prelude {
    pub use super::{SpectateDef, SpectatePlugin};
}
