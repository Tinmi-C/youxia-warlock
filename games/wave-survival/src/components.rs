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
    /// Baseline. Card 19 skin: green_blob, family-green slot.
    Grunt,
    /// Fast but fragile: speed x1.6, hp x0.5. Card 19 skin: mushnub.
    Runner,
    /// Slow but tough: speed x0.6, hp x3.0. Card 19 skin: yeti.
    Tank,
    /// Card 19 elite variant (slow-but-tough flagship, GDD 变体派生):
    /// mushnub_evolved in the elite-red slot, scale = grunt x1.3.
    Elite,
}

impl MonsterKind {
    /// Chase-speed multiplier applied to the wave baseline (`wave_speed`).
    pub fn speed_mul(self) -> f32 {
        match self {
            MonsterKind::Grunt => 1.0,
            MonsterKind::Runner => 1.6,
            MonsterKind::Tank => 0.6,
            MonsterKind::Elite => 0.85,
        }
    }

    /// Hit-point multiplier applied to the wave baseline (`wave_hp`).
    pub fn hp_mul(self) -> f32 {
        match self {
            MonsterKind::Grunt => 1.0,
            MonsterKind::Runner => 0.5,
            MonsterKind::Tank => 3.0,
            MonsterKind::Elite => 2.0,
        }
    }

    /// Cube edge length for this variant's placeholder mesh (and collider ball
    /// diameter). Elite = grunt x1.3, matching its GDD scale-up.
    pub fn cube_size(self) -> f32 {
        match self {
            MonsterKind::Grunt => 0.6,
            MonsterKind::Runner => 0.45,
            MonsterKind::Tank => 0.85,
            MonsterKind::Elite => 0.78,
        }
    }

    /// Placeholder body color = card 19 palette slot (style bible §2):
    /// family green #7AA25C for base monsters, deep red #B03A2E for elites.
    /// The tint system blends this over the model's own material.
    pub fn color(self) -> Color {
        match self {
            MonsterKind::Grunt | MonsterKind::Runner | MonsterKind::Tank => {
                Color::srgb(0.478, 0.635, 0.361)
            } // #7AA25C
            MonsterKind::Elite => Color::srgb(0.690, 0.227, 0.180), // #B03A2E
        }
    }

    // --- card 19 enemy definition table (skin side) -------------------------
    // One row per kind: model file, walk-clip index inside that glTF (the four
    // Quaternius-set models all carry the same 9 clips; `walk` is index 7),
    // and wrapper scale derived from world-height parity with the old
    // monster.glb skins (grunt 0.60 / runner 0.378 / tank 1.062 world units,
    // elite = grunt x1.3 per GDD). New monster = new row here + a wave slot.

    /// glTF model that dresses this kind (path under assets/).
    pub fn model(self) -> &'static str {
        match self {
            MonsterKind::Grunt => "models/green_blob.glb",
            MonsterKind::Runner => "models/mushnub.glb",
            MonsterKind::Tank => "models/yeti.glb",
            MonsterKind::Elite => "models/mushnub_evolved.glb",
        }
    }

    /// Index of the walk-loop clip inside the kind's glTF animations.
    pub fn walk_clip(self) -> usize {
        match self {
            MonsterKind::Grunt | MonsterKind::Runner | MonsterKind::Tank | MonsterKind::Elite => 7, // clip order: attack|Dance|death|hit|idle|Jump|No|walk|Yes
        }
    }

    /// Index of the attack clip (card 22): the same 9-clip set across all four
    /// first-batch models, so one constant covers every kind for now — a new
    /// model with a different layout turns this into a real per-kind match.
    pub fn attack_clip(self) -> usize {
        match self {
            MonsterKind::Grunt | MonsterKind::Runner | MonsterKind::Tank | MonsterKind::Elite => 0,
        }
    }

    /// Index of the hit (flinch) clip (card 22), same shared-layout note.
    pub fn hit_clip(self) -> usize {
        match self {
            MonsterKind::Grunt | MonsterKind::Runner | MonsterKind::Tank | MonsterKind::Elite => 3,
        }
    }

    /// Wrapper scale for the kind's model (world-height parity, see above).
    pub fn wrapper_scale(self) -> f32 {
        match self {
            MonsterKind::Grunt => 0.667,  // 0.60 / 0.90
            MonsterKind::Runner => 0.350, // 0.378 / 1.08
            MonsterKind::Tank => 0.632,   // 1.062 / 1.68
            MonsterKind::Elite => 0.557,  // 0.78 / 1.40
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
/// Q blast (card 9 NovaSlash; card 26 moved the key Shift -> Q). Independent
/// of [`Attack`] on purpose — the two slashes throttle separately.
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

// --- HUD formalization (card 16 UiFormalization) ---
/// Violet fill of the Nova cooldown bar (mirrors the slash bar).
#[derive(Component)]
pub struct UiNovaFill;
/// Container row at top-center; children are alive-enemy pips (one per monster).
#[derive(Component)]
pub struct UiWavePips;
/// Fullscreen translucent overlay shown only while the game is paused.
#[derive(Component)]
pub struct UiPauseOverlay;

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

// --- Facing tracking (card 15 MonsterFacing) ---
/// Unit movement direction on the XZ plane (x = world +X, y = world +Z),
/// written by `systems::heading::derive_heading` (logic side) and turned into
/// wrapper yaw by the presentation plugin. Held while stationary to avoid
/// jitter; seeded towards the player so freshly spawned monsters face inward.
#[derive(Component)]
pub struct Heading {
    pub dir: Vec2,
}

// --- Weapon definition table (card 29 WeaponDefinitionTable) ---
/// One row per weapon: damage / falloff band / arc / cooldown. Mirrors the
/// card 19 `MonsterKind` method-table pattern — a new weapon is a new row
/// here (plus an asset in card 30), never a new branch in a system.
///
/// Numbers: `IronSword` row = the GDD melee values verbatim (m2 convention:
/// 34 damage, 0.9 full / 1.5 far falloff, 0.45s cooldown). `Glaive` row is a
/// new slow-reach design pending a GDD entry (decided on this card:
/// short-and-fast vs long-and-slow proves the table is data-driven).
#[derive(Component, Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub enum WeaponKind {
    /// GDD melee baseline: broad 120° arc, fast swing.
    #[default]
    IronSword,
    /// Slow reach: narrow 60° arc, longer falloff band, slower swing.
    Glaive,
}

impl WeaponKind {
    /// Full damage inside the full-radius band, before the Balance scale.
    pub fn damage(self) -> f32 {
        match self {
            WeaponKind::IronSword => 34.0,
            WeaponKind::Glaive => 22.0,
        }
    }

    /// Inner radius: damage is full at or below this distance.
    pub fn full_range(self) -> f32 {
        match self {
            WeaponKind::IronSword => 0.9,
            WeaponKind::Glaive => 1.4,
        }
    }

    /// Outer radius: linear falloff to zero between full and far, zero beyond.
    pub fn far_range(self) -> f32 {
        match self {
            WeaponKind::IronSword => 1.5,
            WeaponKind::Glaive => 1.9,
        }
    }

    /// Total horizontal arc of the swing, degrees. The hit test keeps targets
    /// within `arc / 2` of the player's logical facing (`Heading`).
    pub fn arc_deg(self) -> f32 {
        match self {
            WeaponKind::IronSword => 120.0,
            WeaponKind::Glaive => 60.0,
        }
    }

    /// Swing cooldown seconds, before the Balance scale.
    pub fn cooldown(self) -> f32 {
        match self {
            WeaponKind::IronSword => 0.45,
            WeaponKind::Glaive => 0.6,
        }
    }
}

/// The weapon currently in the player's hands (card 29). Exactly one per
/// player; swapping = replacing this component (a future pickup card).
#[derive(Component, Clone, Copy, Debug)]
pub struct EquippedWeapon(pub WeaponKind);

/// Presentation-side marker on the carried weapon's model child (card 30
/// WeaponVisual). Lives on the wrapper child that holds the weapon mesh;
/// `kind` mirrors the equipped row so the presentation plugin can swap the
/// mesh when the logical weapon changes.
#[derive(Component, Clone, Copy, Debug)]
pub struct WeaponVisual {
    pub kind: WeaponKind,
}
