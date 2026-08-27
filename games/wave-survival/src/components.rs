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

/// Enemy variant (card 10 EnemyVariants): assigned once at spawn time by
/// WaveSystem; decides that monster's hp/speed/mesh/color through the
/// multipliers below. Older systems stay untouched — they only ever see the
/// generic `Chasing` / `Hp` data these multipliers were baked into.
#[derive(Component, Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum MonsterKind {
    /// Baseline (red, 0.6^3): plain GDD numbers.
    Grunt,
    /// Fast but fragile (yellow, 0.45^3): speed x1.6, hp x0.5.
    Runner,
    /// Slow but tough (purple, 0.85^3): speed x0.6, hp x3.0.
    Tank,
}

impl MonsterKind {
    /// Chase-speed multiplier applied to the wave baseline (`wave_speed`).
    pub fn speed_mul(self) -> f32 {
        match self {
            MonsterKind::Grunt => 1.0,
            MonsterKind::Runner => 1.6,
            MonsterKind::Tank => 0.6,
        }
    }

    /// Hit-point multiplier applied to the wave baseline (`wave_hp`).
    pub fn hp_mul(self) -> f32 {
        match self {
            MonsterKind::Grunt => 1.0,
            MonsterKind::Runner => 0.5,
            MonsterKind::Tank => 3.0,
        }
    }

    /// Cube edge length for this variant's placeholder mesh.
    pub fn cube_size(self) -> f32 {
        match self {
            MonsterKind::Grunt => 0.6,
            MonsterKind::Runner => 0.45,
            MonsterKind::Tank => 0.85,
        }
    }

    /// Placeholder body color for this variant.
    pub fn color(self) -> Color {
        match self {
            MonsterKind::Grunt => Color::srgb(0.75, 0.2, 0.2), // red
            MonsterKind::Runner => Color::srgb(0.95, 0.85, 0.2), // yellow
            MonsterKind::Tank => Color::srgb(0.55, 0.25, 0.8), // purple
        }
    }
}

/// Hit points + invulnerability. Death/despawn is handled by CombatContact (card 5).
#[derive(Component)]
pub struct Hp {
    pub hp: f32,
    pub max: f32,
    pub invuln: f32,
}

impl Hp {
    /// Full-health constructor (max = hp, no invulnerability).
    pub fn full(amount: f32) -> Self {
        Self {
            hp: amount,
            max: amount,
            invuln: 0.0,
        }
    }
}

/// Melee state on the player: cooldown remaining (seconds) until the next slash.
#[derive(Component)]
pub struct Attack {
    pub cooldown: f32,
}

/// AoE nova state on the player: cooldown remaining (seconds) until the next
/// Shift blast (card 9 NovaSlash). Independent of [`Attack`] on purpose — the
/// two slashes throttle separately.
#[derive(Component)]
pub struct NovaAttack {
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

/// Golden pickup dropped on a monster kill; heals the player once armed & close.
#[derive(Component)]
pub struct Pickup {
    pub heal: f32,
    pub arm: f32,
}

// --- UI markers (card 7 GameStateUI) ---
#[derive(Component)]
pub struct UiHpFill;
#[derive(Component)]
pub struct UiHpText;
#[derive(Component)]
pub struct UiWaveText;
#[derive(Component)]
pub struct UiCooldownFill;
#[derive(Component)]
pub struct UiGameOver;

// --- Walk-state tracking (card 12 HeroPresentation) ---
/// Whether the owner moved this frame. Written by
/// `systems::player::update_walk_cycle` (logic side), read by the presentation
/// plugin to play/pause the walk cycle — review decision: walk only while
/// actually moving, never a standing moonwalk.
#[derive(Component)]
pub struct WalkCycle {
    pub playing: bool,
}

/// Owner's previous-frame position; lets `update_walk_cycle` detect movement.
#[derive(Component)]
pub struct PrevTranslation {
    pub v: Vec3,
}
