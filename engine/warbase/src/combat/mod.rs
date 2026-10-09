//! Combat (BX2): unified damage pipeline (DamageRequest -> resolve_damage is
//! the single writer of Hp, per ADR-0011 layer 4), targeting, cooldown fire,
//! homing projectiles. Attribution (DamageDealt.source) is free from the
//! pipeline — BAL readings and FX consume it.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::path::PathFollower;
use crate::unit::{AirUnit, Allegiance, Armor, AttackArmorTable, AttackDef, AttackType, Hp, MaxHp, TargetDomain, UnitId, UnitInstance};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TargetingPriority {
    /// Furthest along the path (highest (lap, progress)) — the loop-TD
    /// meaning of "most dangerous".
    First,
    Last,
    Strongest,
    LowestHp,
    Nearest,
}

/// Attack data copied onto the entity at spawn (defs stay in the registry,
/// kits travel with the unit).
#[derive(Component, Debug)]
pub struct AttackKit {
    pub def: AttackDef,
    pub priority: TargetingPriority,
}

#[derive(Component, Debug)]
pub struct AttackCooldown(pub f32);

#[derive(Component, Debug, Default)]
pub struct CurrentTarget(pub Option<Entity>);

#[derive(Component, Debug)]
pub struct Dead;

#[derive(Message, Debug)]
pub struct DamageRequest {
    pub target: Entity,
    pub source: Entity,
    pub amount: f32,
    pub attack_type: AttackType,
    pub crit: bool,
}

#[derive(Message, Debug, Clone, Copy)]
pub struct DamageDealt {
    pub target: Entity,
    pub source: Entity,
    pub amount: f32,
    pub crit: bool,
    pub killed: bool,
}

#[derive(Message, Debug)]
pub struct HealRequest {
    pub target: Entity,
    pub source: Entity,
    pub amount: f32,
}

/// Resolve: the only writer of "hp goes up" (kept beside resolve_damage so
/// the single-writer family lives in one place).
pub fn resolve_heals(
    mut requests: MessageReader<HealRequest>,
    mut units: Query<(&mut Hp, &MaxHp), Without<Dead>>,
) {
    for req in requests.read() {
        if let Ok((mut hp, max)) = units.get_mut(req.target) {
            if hp.0 > 0.0 {
                hp.0 = (hp.0 + req.amount).min(max.0);
            }
        }
    }
}

/// Periodic area heal for support units (numbers from the map bundle).
#[derive(Component, Debug)]
pub struct Healer {
    pub amount: f32,
    pub interval: f32,
    pub radius: f32,
    pub timer: f32,
}

pub fn healer_tick(
    time: Res<Time>,
    mut requests: MessageWriter<HealRequest>,
    mut healers: Query<(Entity, &Transform, &Allegiance, &mut Healer)>,
    allies: Query<(Entity, &Transform, &Allegiance, &Hp, &MaxHp), Without<Dead>>,
) {
    let dt = time.delta_secs();
    for (e, pos, side, mut healer) in &mut healers {
        healer.timer -= dt;
        if healer.timer > 0.0 {
            continue;
        }
        healer.timer = healer.interval;
        for (ae, apos, aside, hp, max) in &allies {
            if aside != side || hp.0 <= 0.0 || hp.0 >= max.0 {
                continue;
            }
            if pos.translation.distance(apos.translation) <= healer.radius {
                let _ = max;
                requests.write(HealRequest { target: ae, source: e, amount: healer.amount });
            }
        }
    }
}

#[derive(Message, Debug, Clone)]
pub struct UnitDied {
    pub entity: Entity,
    pub def: UnitId,
    pub gold: u32,
    pub wood: u32,
    pub xp: u32,
    pub killer: Option<Entity>,
}

/// Homing projectile as a pure logic entity; visuals are the game's business.
#[derive(Component, Debug)]
pub struct Projectile {
    pub target: Entity,
    pub source: Entity,
    pub speed: f32,
    pub damage: f32,
    pub attack_type: AttackType,
}

pub fn tick_cooldowns(time: Res<Time>, mut q: Query<&mut AttackCooldown>) {
    let dt = time.delta_secs();
    for mut cd in &mut q {
        cd.0 -= dt;
    }
}

/// Intent: every attacker re-picks its target each frame (no sticky locks in
/// v1 — cheap at our unit counts, trivially testable).
#[allow(clippy::type_complexity)]
pub fn pick_targets(
    mut attackers: Query<(&AttackKit, &Transform, &Allegiance, &mut CurrentTarget)>,
    enemies: Query<
        (
            Entity,
            &Transform,
            &Hp,
            &Allegiance,
            Has<AirUnit>,
            Option<&PathFollower>,
        ),
        Without<Dead>,
    >,
) {
    for (kit, pos, side, mut target) in &mut attackers {
        // (entity, hp, path rank, dist) for the current best candidate.
        let mut best: Option<(Entity, f32, (u32, f32), f32)> = None;
        for (e, epos, hp, eside, air, follower) in &enemies {
            if eside == side {
                continue;
            }
            if kit.def.targets == TargetDomain::Ground && air {
                continue;
            }
            if kit.def.targets == TargetDomain::Air && !air {
                continue;
            }
            let dist = pos.translation.distance(epos.translation);
            if dist > kit.def.range {
                continue;
            }
            let rank = follower.map(|f| (f.laps, f.progress)).unwrap_or((0, 0.0));
            let better = match (best, kit.priority) {
                (None, _) => true,
                (Some((_, bhp, brank, _)), TargetingPriority::First) => rank > brank,
                (Some((_, _, brank, _)), TargetingPriority::Last) => rank < brank,
                (Some((_, bhp, _, _)), TargetingPriority::Strongest) => hp.0 > bhp,
                (Some((_, bhp, _, _)), TargetingPriority::LowestHp) => hp.0 < bhp,
                (Some((_, _, _, bdist)), TargetingPriority::Nearest) => dist < bdist,
            };
            if better {
                best = Some((e, hp.0, rank, dist));
            }
        }
        target.0 = best.map(|(e, _, _, _)| e);
    }
}

/// Act: fire when the cooldown is ready. Instant hits emit DamageRequest
/// now; projectile attacks spawn a homing logic entity at the attacker.
#[allow(clippy::type_complexity)]
pub fn fire_attacks(
    mut commands: Commands,
    mut requests: MessageWriter<DamageRequest>,
    mut attackers: Query<
        (
            Entity,
            &AttackKit,
            &Transform,
            &Allegiance,
            &CurrentTarget,
            &mut AttackCooldown,
            Option<&crate::buff::EffectiveStats>,
        ),
        Without<Dead>,
    >,
    targets: Query<(&Hp, &Allegiance), Without<Dead>>,
) {
    for (entity, kit, pos, side, target, mut cd, eff) in &mut attackers {
        let Some(t) = target.0 else { continue };
        let Ok((hp, tside)) = targets.get(t) else { continue };
        if hp.0 <= 0.0 || tside == side || cd.0 > 0.0 {
            continue;
        }
        let eff = eff.copied().unwrap_or_else(crate::buff::EffectiveStats::neutral);
        let damage = kit.def.damage * eff.damage_mult.max(0.0);
        match &kit.def.projectile {
            None => {
                requests.write(DamageRequest {
                    target: t,
                    source: entity,
                    amount: damage,
                    attack_type: kit.def.attack_type,
                    crit: false,
                });
            }
            Some(proj) => {
                commands.spawn((
                    Projectile {
                        target: t,
                        source: entity,
                        speed: proj.speed,
                        damage,
                        attack_type: kit.def.attack_type,
                    },
                    *pos,
                ));
            }
        }
        cd.0 = kit.def.interval * eff.interval_mult.max(0.05);
    }
}

/// Act: move homing projectiles; on impact emit DamageRequest. Edge behavior
/// v1: target gone or dead mid-flight => fizzle (despawn, no damage).
pub fn move_projectiles(
    mut commands: Commands,
    mut requests: MessageWriter<DamageRequest>,
    time: Res<Time>,
    targets: Query<(&Transform, &Hp), (Without<Dead>, Without<Projectile>)>,
    mut projs: Query<(Entity, &mut Projectile, &mut Transform)>,
) {
    let dt = time.delta_secs();
    for (pe, mut proj, mut pos) in &mut projs {
        let Ok((tpos, hp)) = targets.get(proj.target) else {
            commands.entity(pe).despawn();
            continue;
        };
        if hp.0 <= 0.0 {
            commands.entity(pe).despawn();
            continue;
        }
        let to = tpos.translation - pos.translation;
        let step = proj.speed * dt;
        if to.length() <= step {
            requests.write(DamageRequest {
                target: proj.target,
                source: proj.source,
                amount: proj.damage,
                attack_type: proj.attack_type,
                crit: false,
            });
            commands.entity(pe).despawn();
        } else {
            pos.translation += to.normalize_or_zero() * step;
        }
    }
}

/// Resolve: the single writer of Hp. Applies the attack x armor multiplier,
/// emits DamageDealt (with attribution) and UnitDied, marks Dead.
pub fn resolve_damage(
    mut requests: MessageReader<DamageRequest>,
    mut dealt: MessageWriter<DamageDealt>,
    mut died: MessageWriter<UnitDied>,
    mut commands: Commands,
    registry: Res<crate::unit::UnitRegistry>,
    table: Res<AttackArmorTable>,
    mut units: Query<(&mut Hp, &Armor, &UnitInstance, Option<&crate::buff::EffectiveStats>)>,
) {
    for req in requests.read() {
        let Ok((mut hp, armor, inst, eff)) = units.get_mut(req.target) else {
            continue;
        };
        if hp.0 <= 0.0 {
            continue; // already dead this frame — no double bounty
        }
        let taken = eff
            .map(|e| e.damage_taken_mult.max(0.0))
            .unwrap_or(1.0);
        let mult = table.multiplier(req.attack_type, armor.0) * taken;
        let amount = req.amount * mult;
        hp.0 -= amount;
        let killed = hp.0 <= 0.0;
        dealt.write(DamageDealt {
            target: req.target,
            source: req.source,
            amount,
            crit: req.crit,
            killed,
        });
        if killed {
            commands.entity(req.target).insert(Dead);
            let (gold, wood, xp) = registry
                .get(&inst.def)
                .map(|d| (d.bounty_gold, d.bounty_wood, d.grant_xp))
                .unwrap_or((0, 0, 0));
            died.write(UnitDied {
                entity: req.target,
                def: inst.def.clone(),
                gold,
                wood,
                xp,
                killer: Some(req.source),
            });
        }
    }
}

pub fn cleanup_dead(mut commands: Commands, q: Query<Entity, With<Dead>>) {
    for e in &q {
        commands.entity(e).despawn();
    }
}
