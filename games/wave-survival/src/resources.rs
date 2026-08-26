//! Resources = global singletons. Keep one writer per resource where possible.

use bevy::prelude::*;

/// Example: game statistics consumed by the log dashboard / UI.
#[derive(Resource, Default)]
pub struct GameStats {
    pub kills: u32,
}
