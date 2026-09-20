//! Components = pure data (nouns). A new mechanism = a new component + a new
//! system; existing systems stay untouched (capability-card rule).

use bevy::prelude::*;

/// Attack channel. The MVP only resolves ONE axis: physical armor (see
/// `docs/requirements.md` §6). "Mixed" deals physical *and* magic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttackType {
    Physical,
    Magic,
    Mixed,
}

/// The "new mechanism" a fused tower gains (requirements §7.3). Fusion is the
/// only way to obtain these; the shop never sells them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FusionKind {
    /// 神射手: +40% range, prioritises the highest-HP target.
    Marksman,
    /// 大法师: AOE blast.
    Archmage,
    /// 魔弓手: mixed (physical+magic) damage, bypasses armor.
    Hybrid,
    /// 壁垒炮: AOE blast + 30% slow.
    Bastion,
}

/// A tower's archetype. Base(`i`) is one of the 4 purchasable towers (index into
/// `TowerDefs`); `Fused(k)` is a fusion result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TowerKind {
    Base(usize),
    Fused(FusionKind),
}

/// A live enemy instance walking the path. Values come from `EnemyDefs`
/// (resources.rs) at spawn; this is the mutable runtime copy.
#[derive(Component)]
pub struct Enemy {
    pub hp: f32,
    pub max_hp: f32,
    pub speed: f32,
    pub leak: u32,          // base HP lost when this enemy reaches the base
    pub kill_gold: u32,     // gold earned when this enemy dies
    pub physical_armor: bool, // shield enemy: physical damage -50%
    pub def_index: usize,   // archetype index into EnemyDefs (AC3 drop rules)
    pub next_wp: usize,     // index of the next waypoint to head toward
}

/// A placed tower. Values come from `TowerDefs` (base) or a fusion recipe at
/// placement / fusion.
#[derive(Component)]
pub struct Tower {
    pub tower_index: usize, // base index 0..3 (only meaningful for Base)
    pub damage: f32,
    pub attack_speed: f32,
    pub range: f32,
    pub attack_type: AttackType,
    pub cooldown: f32, // seconds until it can fire again
    pub target: Option<Entity>,
    pub kind: TowerKind,
    /// >0 => on fire, hit every enemy within this radius of the target (AOE).
    pub aoe_radius: f32,
    /// 0 => no slow; else 0 < factor <= 1 speed multiplier for `slow_duration` s.
    pub slow_factor: f32,
    pub slow_duration: f32,
    /// Total gold invested in this tower (TO7, requirements §16.5): the build
    /// cost for a base tower; ingredient costs + fusion fee for a fused one.
    /// Selling refunds `invested × 70%` floored.
    pub invested: u32,
}

/// ART2: the ballista weapon mounted on an archer tower. An independent
/// child entity of the tower — later cards rotate it toward the current
/// target and animate the shot procedurally (the model ships no animation;
/// its string is separate geometry in a 767-piece assembly).
#[derive(Component)]
pub struct BallistaMount;

/// A temporary slow effect on an enemy (e.g. from the shield / bastion tower).
#[derive(Component)]
pub struct Slow {
    pub timer: f32,
    pub factor: f32,
}

/// Who launched a projectile and where it heads. Towers fire HOMING rounds
/// locked on an enemy (§5.1); skill shots (SK2) fly to a FIXED ground point
/// — the origin seam the skill system reuses (§17.2 复用投射物：发起方=技能).
#[derive(Debug, Clone, Copy)]
pub enum ProjectileOrigin {
    /// Track the target's current position every frame (never misses; PR1).
    Homing { target: Entity },
    /// Fly straight to this world point and resolve there (skill shots).
    Fixed { point: Vec3 },
}

/// A rule-layer projectile (PR1, requirements §5.1): a homing round locked on
/// its target — flies toward the target's *current* position every frame, so
/// it never misses. Damage and attack type are snapshotted at launch; armor
/// resolves at impact. Built as a generic entity (origin-agnostic: towers
/// home, skills fly to a fixed point — `ProjectileOrigin`) so the skill
/// system reuses it directly.
#[derive(Component)]
pub struct Projectile {
    pub origin: ProjectileOrigin,
    /// Damage snapshot taken at launch (post-boost, pre-armor).
    pub damage: f32,
    pub attack_type: AttackType,
    /// 0 => single-target (hits only the target); >0 => AOE: everything within
    /// this radius of the impact point takes damage (armor per-enemy).
    pub aoe_radius: f32,
    /// On-hit slow payload (0 => none): applied to every victim on impact
    /// (TO6 rework: slow rides the hit, §5 original wording).
    pub slow_factor: f32,
    pub slow_duration: f32,
    /// Speed in world units per second (§5.1 starting value, §14 tuning row).
    pub speed: f32,
    /// Last known target position — the AOE detonation anchor when a homing
    /// target dies mid-flight (PR2); for fixed-point shots, the point itself.
    pub last_target_pos: Vec3,
}

/// Healer aura (EN5): periodically restores hp of nearby damaged allies.
/// Attached at spawn to enemies whose `EnemyDef::healer` is true.
#[derive(Component)]
pub struct Healer {
    pub radius: f32,
    pub period: f32, // seconds between heal ticks
    pub heal_per_tick: f32,
    pub tick_timer: f32,
}

/// One of the 8 buildable cells beside the path.
#[derive(Component)]
pub struct TowerSlot {
    pub index: usize,
    pub occupied: bool,
}

/// Marks the base entity (the path endpoint; HP lives in the `BaseHp` resource).
#[derive(Component)]
pub struct BaseMarker;

/// ART3 marker: the arena ground top face. Purely visual (the cliff skirt is
/// unmarked); gameplay never reads the terrain — pins only assert existence
/// and which material branch (textured vs flat fallback) was taken.
#[derive(Component)]
pub struct MapGround;
