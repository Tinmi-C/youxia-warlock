//! Unit domain model (BX1): defs live in the map bundle, instances are ECS
//! component combinations — no hard-coded tower/creep classes. Composition
//! decides what a unit is: PathFollower => walks, XpHolder => hero tower,
//! AttackKit => fires.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub type UnitId = String;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AttackType {
    Normal,
    Piercing,
    Siege,
    Magic,
    Hero,
}

impl AttackType {
    pub const ALL: [AttackType; 5] = [
        AttackType::Normal,
        AttackType::Piercing,
        AttackType::Siege,
        AttackType::Magic,
        AttackType::Hero,
    ];
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ArmorType {
    Light,
    Medium,
    Heavy,
    Fortified,
    Hero,
    Divine,
}

impl ArmorType {
    pub const ALL: [ArmorType; 6] = [
        ArmorType::Light,
        ArmorType::Medium,
        ArmorType::Heavy,
        ArmorType::Fortified,
        ArmorType::Hero,
        ArmorType::Divine,
    ];
}

/// Attack x armor multiplier matrix shipped with each map bundle. The base
/// ships the structure and the lookup only — never the numbers.
#[derive(Debug, Clone, Resource, Serialize, Deserialize)]
pub struct AttackArmorTable {
    /// Row-major `[attack_type][armor_type]`, aligned with the enum orders.
    pub mult: [[f32; 6]; 5],
}

impl Default for AttackArmorTable {
    fn default() -> Self {
        Self { mult: [[1.0; 6]; 5] }
    }
}

impl AttackArmorTable {
    /// Unknown combos fall back to 1.0 (no multiplier) — decided and pinned
    /// by UT-AA-02; bundles ship the full matrix anyway.
    pub fn multiplier(&self, attack: AttackType, armor: ArmorType) -> f32 {
        self.mult[attack as usize][armor as usize]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TargetDomain {
    Ground,
    Air,
    Both,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UnitTrait {
    Fast,
    Healer,
    AuraBearer,
    Air,
    Resistant,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaseAttributes {
    pub hp: f32,
    pub move_speed: f32,
    pub body_radius: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjDef {
    pub speed: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttackDef {
    pub damage: f32,
    pub interval: f32,
    pub range: f32,
    pub attack_type: AttackType,
    pub targets: TargetDomain,
    pub projectile: Option<ProjDef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnitDef {
    pub id: UnitId,
    pub attrs: BaseAttributes,
    #[serde(default)]
    pub attack: Option<AttackDef>,
    pub armor_type: ArmorType,
    /// Build cost (towers); 0 for creeps. Numbers ship in the map bundle.
    #[serde(default)]
    pub cost_gold: u32,
    #[serde(default)]
    pub cost_wood: u32,
    #[serde(default)]
    pub bounty_gold: u32,
    #[serde(default)]
    pub bounty_wood: u32,
    #[serde(default)]
    pub grant_xp: u32,
    #[serde(default)]
    pub traits: Vec<UnitTrait>,
}

#[derive(Resource, Debug, Default, Clone)]
pub struct UnitRegistry {
    pub defs: HashMap<UnitId, UnitDef>,
}

impl UnitRegistry {
    pub fn get(&self, id: &str) -> Option<&UnitDef> {
        self.defs.get(id)
    }
}

// ---------------------------------------------------------------------------
// Instance components
// ---------------------------------------------------------------------------

/// Current health. Single writer: combat::resolve_damage (WarbaseSet::Resolve).
#[derive(Component, Debug)]
pub struct Hp(pub f32);

#[derive(Component, Debug)]
pub struct MaxHp(pub f32);

#[derive(Component, Debug)]
pub struct Armor(pub ArmorType);

#[derive(Component, Debug)]
pub struct UnitInstance {
    pub def: UnitId,
    pub level: u8,
}

/// Hero-tower growth holder: eats kill xp, banks skill points.
#[derive(Component, Debug)]
pub struct XpHolder {
    pub xp: u32,
    pub level: u8,
    pub points: u8,
    pub next_level_xp: u32,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub enum Allegiance {
    /// Defending side (towers and friends).
    Defender,
    /// Attacking side (creeps).
    Attacker,
}

/// Present on units with UnitTrait::Air; Ground-only attacks skip them.
#[derive(Component, Debug)]
pub struct AirUnit;

/// Spawn a unit instance from its def; `hp_scale` multiplies hp/max-hp
/// (per-wave scaling ships in the wave table). Returns None for unknown ids.
/// Extra behaviour comes from composition (caller inserts PathFollower,
/// XpHolder, ...).
pub fn spawn_unit(
    world: &mut World,
    registry: &UnitRegistry,
    id: &str,
    pos: Vec3,
    side: Allegiance,
    hp_scale: f32,
) -> Option<Entity> {
    let def = registry.get(id)?.clone();
    let mut e = world.spawn((
        UnitInstance { def: def.id.clone(), level: 1 },
        Hp(def.attrs.hp * hp_scale),
        MaxHp(def.attrs.hp * hp_scale),
        Armor(def.armor_type),
        side,
        Transform::from_translation(pos),
    ));
    if def.traits.contains(&UnitTrait::Air) {
        e.insert(AirUnit);
    }
    if def.attrs.move_speed > 0.0 {
        e.insert(crate::path::MoveSpeed(def.attrs.move_speed));
    }
    if let Some(atk) = def.attack {
        e.insert(crate::combat::AttackKit {
            def: atk,
            priority: crate::combat::TargetingPriority::First,
        })
        .insert(crate::combat::AttackCooldown(0.0))
        .insert(crate::combat::CurrentTarget::default());
    }
    Some(e.id())
}

/// Commands-based variant of [`spawn_unit`] for use inside systems (the
/// wave director). Keep the component set in sync with the World version.
pub fn spawn_unit_deferred(
    commands: &mut Commands,
    registry: &UnitRegistry,
    id: &str,
    pos: Vec3,
    side: Allegiance,
    hp_scale: f32,
) -> Option<Entity> {
    let def = registry.get(id)?.clone();
    let mut ec = commands.spawn((
        UnitInstance { def: def.id.clone(), level: 1 },
        Hp(def.attrs.hp * hp_scale),
        MaxHp(def.attrs.hp * hp_scale),
        Armor(def.armor_type),
        side,
        Transform::from_translation(pos),
    ));
    if def.traits.contains(&UnitTrait::Air) {
        ec.insert(AirUnit);
    }
    if def.attrs.move_speed > 0.0 {
        ec.insert(crate::path::MoveSpeed(def.attrs.move_speed));
    }
    if let Some(atk) = def.attack {
        ec.insert(crate::combat::AttackKit {
            def: atk,
            priority: crate::combat::TargetingPriority::First,
        })
        .insert(crate::combat::AttackCooldown(0.0))
        .insert(crate::combat::CurrentTarget::default());
    }
    Some(ec.id())
}
