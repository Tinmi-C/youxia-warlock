//! Resources = global singletons. Keep one writer per resource where possible.

use bevy::prelude::*;

/// Seconds of rest between waves (also the delay before wave 1).
pub const WAVE_BREAK: f32 = 3.0;

/// Wave progression. `n` = current wave (0 = none started yet); `timer` = seconds
/// until the next wave spawns (re-armed when a wave spawns, per GDD).
#[derive(Resource)]
pub struct Wave {
    pub n: u32,
    pub timer: f32,
}

impl Default for Wave {
    fn default() -> Self {
        Self { n: 0, timer: WAVE_BREAK }
    }
}

/// Example: game statistics consumed by the log dashboard / UI.
#[derive(Resource, Default)]
pub struct GameStats {
    pub kills: u32,
}
