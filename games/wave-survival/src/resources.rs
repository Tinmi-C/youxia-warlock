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
        Self {
            n: 0,
            timer: WAVE_BREAK,
        }
    }
}

/// Example: game statistics consumed by the log dashboard / UI.
#[derive(Resource, Default)]
pub struct GameStats {
    pub kills: u32,
}

/// Hot-tunable combat numbers (card 11 EguiTunePanel; multiplier semantics
/// since card 29). Defaults are neutral (1.0 = table values verbatim); the F1
/// panel edits these live at run time and gameplay systems read them every
/// frame. Weapon base numbers live in `components::WeaponKind` (card 29).
#[derive(Resource, Debug, Clone)]
pub struct Balance {
    /// Melee damage multiplier on the equipped weapon's table damage.
    pub slash_damage_scale: f32,
    /// Melee cooldown multiplier on the equipped weapon's table cooldown.
    pub slash_cooldown_scale: f32,
    /// Nova blast radius (default: GDD Nova 半径 1.6).
    pub nova_radius: f32,
    /// Nova flat damage inside the circle (default: GDD 60).
    pub nova_damage: f32,
    /// Nova cooldown seconds (default: GDD 5).
    pub nova_cooldown: f32,
    /// Contact-bite damage (default: m2 CONTACT_DAMAGE 15).
    pub contact_damage: f32,
}

impl Default for Balance {
    fn default() -> Self {
        Self {
            slash_damage_scale: 1.0,
            slash_cooldown_scale: 1.0,
            nova_radius: crate::systems::nova::NOVA_RADIUS,
            nova_damage: crate::systems::nova::NOVA_DAMAGE,
            nova_cooldown: crate::systems::nova::NOVA_COOLDOWN,
            contact_damage: crate::systems::contact::CONTACT_DAMAGE,
        }
    }
}
