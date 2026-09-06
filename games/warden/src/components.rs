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

/// The movable tower-placement cursor (WASD steers it over the ground plane).
#[derive(Component)]
pub struct PlacementCursor {
    pub speed: f32,
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
}

/// A temporary slow effect on an enemy (e.g. from the shield / bastion tower).
#[derive(Component)]
pub struct Slow {
    pub timer: f32,
    pub factor: f32,
}

/// One of the 8 buildable cells beside the path.
#[derive(Component)]
pub struct TowerSlot {
    pub index: usize,
    pub occupied: bool,
}

/// A bevy_ui shop button that selects a base tower type to build.
#[derive(Component)]
pub struct ShopButton {
    pub tower_index: usize,
}

/// Marks the base entity (the path endpoint; HP lives in the `BaseHp` resource).
#[derive(Component)]
pub struct BaseMarker;
