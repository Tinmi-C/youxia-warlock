//! Components = pure data (nouns). A new mechanism = a new component + a new
//! system; existing systems stay untouched (capability-card rule).

use bevy::prelude::*;

/// Movable marker + movement tuning. Speed unit: world units per second.
#[derive(Component)]
pub struct Player {
    pub speed: f32,
}

/// Monster tag: marks entities the melee slash can hit (and later, enemies).
#[derive(Component)]
pub struct Monster;

/// Hit points. Death/despawn is a later card (CombatContact); PlayerAttack only subtracts.
#[derive(Component)]
pub struct Hp {
    pub hp: f32,
}

/// Melee state on the player: cooldown remaining (seconds) until the next slash.
#[derive(Component)]
pub struct Attack {
    pub cooldown: f32,
}

/// Visual feedback state (m2 convention): `flash` > 0 means "tint white".
#[derive(Component)]
pub struct Visual {
    pub flash: f32,
}

/// Chase tuning on a monster: moves toward the player at `speed` units/sec.
/// Written by WaveSystem (per-wave speed); consumed by EnemyChase (card 4).
#[derive(Component)]
pub struct Chasing {
    pub speed: f32,
}
