//! Unified damage resolution (SK7, ADR-0011 layer 4): every damage source —
//! melee hits, projectile impacts, skill blasts — writes a `DamageRequest`
//! message; `resolve_damage` is the ONLY system in the game that subtracts
//! from `Enemy.hp`. One-axis armor resolves here, hit feedback (HitFlash)
//! lands here, and every application is published as `DamageDealt` with its
//! source for FX, kill attribution and the BAL contribution readout (SK11).
//!
//! WX1 seam (ADR-0012): warden now depends on warbase. The attribution
//! model (`DamageOrigin`) lives in the base; this module provides the
//! warden-side bridge. The full component-level migration (Enemy → base
//! Hp/Armor components) proceeds in WX2-WX6 paired increments.
//!
//! Frame semantics (the unified-damage-pipeline KB note): "simultaneous"
//! means same frame — the writers run earlier in Simulate, the resolver
//! drains everything they queued after them, one victim batch, no ordering
//! between requests.

use bevy::prelude::*;

use crate::components::{AttackType, Enemy};
use crate::systems::fx::{HitFlash, HIT_FLASH_SECS};

/// Who caused a piece of damage — the attribution seam for kill credit,
/// balance readouts and skill on-hit hooks. `CommanderSkill`/`TowerSkill`
/// carry the skill's string def id (WX2).
///
/// WX1 bridge: this maps onto `warbase::combat::DamageOrigin`:
/// - Tower(e)         ↔ DamageOrigin::Attack(e)
/// - TowerSkill{t, s} ↔ DamageOrigin::Skill { caster: t, skill: row(s) }
/// - CommanderSkill(s)↔ DamageOrigin::Skill { caster: commander, skill: row(s) }
/// - System           ↔ DamageOrigin::System
///
/// warden keeps its own enum until the full component migration (WX3+)
/// so the existing tests stay untouched. `row(s)` is the skill's position
/// in `resources::SKILL_ORDER` — the base's attribution stays index-shaped.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DamageSource {
    Tower(Entity),
    CommanderSkill(&'static str),
    TowerSkill { tower: Entity, skill: &'static str },
    /// Environment/misc (nothing uses it yet; leaks damage the BASE, not
    /// enemies, so they never appear here).
    System,
}

impl DamageSource {
    /// WX2 bridge: the base index for a skill id (`SKILL_ORDER` position).
    fn skill_row(skill: &str) -> usize {
        crate::resources::SKILL_ORDER
            .iter()
            .position(|s| *s == skill)
            .expect("skill id in SKILL_ORDER")
    }

    /// WX1 bridge: convert to the base's attribution type. `commander` is
    /// the singleton commander entity (callers that have it pass it in).
    pub fn to_origin(&self, commander: Option<Entity>) -> warbase::DamageOrigin {
        match self {
            DamageSource::Tower(e) => warbase::DamageOrigin::Attack(*e),
            DamageSource::CommanderSkill(skill) => warbase::DamageOrigin::Skill {
                caster: commander.unwrap_or(Entity::PLACEHOLDER),
                skill: Self::skill_row(skill),
            },
            DamageSource::TowerSkill { tower, skill } => warbase::DamageOrigin::Skill {
                caster: *tower,
                skill: Self::skill_row(skill),
            },
            DamageSource::System => warbase::DamageOrigin::System,
        }
    }
}

/// A pending application of damage. `amount` is PRE-armor; armor resolves
/// per victim in `resolve_damage`.
#[derive(Message, Debug, Clone, Copy)]
pub struct DamageRequest {
    pub victim: Entity,
    pub amount: f32,
    pub attack_type: AttackType,
    pub source: DamageSource,
    pub crit: bool,
}

/// One applied hit, published by `resolve_damage` for FX / attribution /
/// BAL. `amount` is post-armor (what was actually subtracted); `killed` is
/// set only on the hit that brought hp to <= 0.
#[derive(Message, Debug, Clone, Copy)]
pub struct DamageDealt {
    pub victim: Entity,
    pub amount: f32,
    pub attack_type: AttackType,
    pub source: DamageSource,
    pub crit: bool,
    pub killed: bool,
}

/// One-axis armor (§6), table-driven since WX6-4a: the warden semantics
/// (physical halved vs armored; magic/mixed bypass) ride the BASE's
/// attack-x-armor matrix — Normal/Fortified is the only 0.5 cell, every
/// other combo 1.0. The table becomes map data with warden.ron (WX6-6).
pub fn warden_armor_table() -> warbase::prelude::AttackArmorTable {
    let mut table = warbase::prelude::AttackArmorTable::default();
    table.mult[warbase::prelude::AttackType::Normal as usize]
        [warbase::prelude::ArmorType::Fortified as usize] = 0.5;
    table
}

/// Map warden's three attack channels onto base attack types (distinct rows
/// so readings/UI keep the three identities; only Normal meets the 0.5 cell).
pub fn to_base_attack(attack: AttackType) -> warbase::prelude::AttackType {
    match attack {
        AttackType::Physical => warbase::prelude::AttackType::Normal,
        AttackType::Magic => warbase::prelude::AttackType::Magic,
        AttackType::Mixed => warbase::prelude::AttackType::Piercing,
    }
}

/// Armor component for an enemy (physical armor => Fortified, else Light).
pub fn base_armor(physical_armor: bool) -> warbase::prelude::ArmorType {
    if physical_armor {
        warbase::prelude::ArmorType::Fortified
    } else {
        warbase::prelude::ArmorType::Light
    }
}

/// The single resolution point of every damage request (table-driven).
pub fn effective_damage(
    table: &warbase::prelude::AttackArmorTable,
    attack_type: AttackType,
    physical_armor: bool,
    base: f32,
) -> f32 {
    base * table.multiplier(
        to_base_attack(attack_type),
        base_armor(physical_armor),
    )
}

/// THE sole writer of enemy `Hp` (SK7; the component is the base's since
/// WX6-2). Drains the frame's DamageRequests: skips the already-dead (no
/// double-taps on corpses, no double kill messages), resolves armor,
/// subtracts, flashes, publishes DamageDealt. Kill rewards/despawn stay
/// where they were (`resolve_death`, Cleanup).
pub fn resolve_damage(
    mut commands: Commands,
    mut requests: MessageReader<DamageRequest>,
    mut dealt: MessageWriter<DamageDealt>,
    table: Res<warbase::prelude::AttackArmorTable>,
    mut enemies: Query<(&mut warbase::prelude::Hp, &Enemy)>,
) {
    for req in requests.read() {
        let Ok((mut hp, en)) = enemies.get_mut(req.victim) else {
            continue; // despawned between request and resolution
        };
        if hp.0 <= 0.0 {
            continue; // corpse: another request this frame already killed it
        }
        let applied = effective_damage(&table, req.attack_type, en.physical_armor, req.amount);
        let killed = hp.0 - applied <= 0.0;
        hp.0 -= applied;
        commands.entity(req.victim).insert(HitFlash {
            timer: HIT_FLASH_SECS,
            original: None,
        });
        dealt.write(DamageDealt {
            victim: req.victim,
            amount: applied,
            attack_type: req.attack_type,
            source: req.source,
            crit: req.crit,
            killed,
        });
        if killed {
            info!(
                "[dmg] {:#?} killed by {:?} for {applied:.1}",
                req.victim, req.source
            );
        }
    }
}
