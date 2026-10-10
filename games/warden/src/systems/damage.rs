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
/// carry the registry def id of the skill.
///
/// WX1 bridge: this maps onto `warbase::combat::DamageOrigin`:
/// - Tower(e)         ↔ DamageOrigin::Attack(e)
/// - TowerSkill{t, s} ↔ DamageOrigin::Skill { caster: t, skill: s }
/// - CommanderSkill(i)↔ DamageOrigin::Skill { caster: commander, skill: i }
/// - System           ↔ DamageOrigin::System
///
/// warden keeps its own enum until the full component migration (WX2+)
/// so all 153 existing tests stay untouched.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DamageSource {
    Tower(Entity),
    CommanderSkill(usize),
    TowerSkill { tower: Entity, skill: usize },
    /// Environment/misc (nothing uses it yet; leaks damage the BASE, not
    /// enemies, so they never appear here).
    System,
}

impl DamageSource {
    /// WX1 bridge: convert to the base's attribution type. `commander` is
    /// the singleton commander entity (callers that have it pass it in).
    pub fn to_origin(&self, commander: Option<Entity>) -> warbase::DamageOrigin {
        match self {
            DamageSource::Tower(e) => warbase::DamageOrigin::Attack(*e),
            DamageSource::CommanderSkill(slot) => {
                warbase::DamageOrigin::Skill {
                    caster: commander.unwrap_or(Entity::PLACEHOLDER),
                    skill: *slot,
                }
            }
            DamageSource::TowerSkill { tower, skill } => {
                warbase::DamageOrigin::Skill {
                    caster: *tower,
                    skill: *skill,
                }
            }
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

/// One-axis armor (§6): physical is halved vs armored enemies; magic and
/// mixed (穿甲) bypass it. Lives here since SK7 — the single resolution
/// point of every damage request.
pub fn effective_damage(attack_type: AttackType, physical_armor: bool, base: f32) -> f32 {
    if physical_armor && attack_type == AttackType::Physical {
        base * 0.5
    } else {
        base
    }
}

/// THE sole writer of `Enemy.hp` (SK7). Drains the frame's DamageRequests:
/// skips the already-dead (no double-taps on corpses, no double kill
/// messages), resolves armor, subtracts, flashes, publishes DamageDealt.
/// Kill rewards/despawn stay where they were (`resolve_death`, Cleanup).
pub fn resolve_damage(
    mut commands: Commands,
    mut requests: MessageReader<DamageRequest>,
    mut dealt: MessageWriter<DamageDealt>,
    mut enemies: Query<(Entity, &mut Enemy)>,
) {
    for req in requests.read() {
        let Ok((_, mut en)) = enemies.get_mut(req.victim) else {
            continue; // despawned between request and resolution
        };
        if en.hp <= 0.0 {
            continue; // corpse: another request this frame already killed it
        }
        let applied = effective_damage(req.attack_type, en.physical_armor, req.amount);
        let killed = en.hp - applied <= 0.0;
        en.hp -= applied;
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
