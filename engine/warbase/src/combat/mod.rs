//! Combat (BX2): unified damage pipeline (DamageRequest -> resolve_damage is
//! the single writer of Hp, per ADR-0011 layer 4), targeting, cooldown fire,
//! homing projectiles. Attribution (DamageDealt.source) is free from the
//! pipeline — BAL readings and FX consume it.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::movement::UnitCommand;
use crate::path::PathFollower;
use crate::skill::Stunned;
use crate::unit::{AirUnit, Allegiance, Armor, ArmorType, AttackArmorTable, AttackDef, AttackType, Hp, Hostility, MaxHp, TargetDomain, UnitId, UnitInstance};

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

/// In-flight attack windup (SKVOC, plan §3): a hit committed at the damage
/// point but not yet delivered. Carries its own target (retargeting never
/// diverts a committed swing). Cancelled — no damage, no cooldown burn —
/// when the target is lost/dead or the ATTACKER gets stunned; the slot
/// cooldown starts only at the damage point (phase 1 of `fire_attacks`).
#[derive(Component, Debug)]
pub struct AttackWindup {
    pub remaining: f32,
    pub target: Entity,
}

/// Post-hit backswing lock (SKVOC): while present the unit cannot START a
/// new windup (the damage itself already resolved). Released early by any
/// fresh `UnitCommand` naming the unit — the micro-retreat cancel; see
/// `fire_attacks` phase 0 for the pinned semantics.
#[derive(Component, Debug)]
pub struct AttackLock {
    pub remaining: f32,
}

#[derive(Message, Debug, Clone, Copy)]
pub struct DamageRequest {
    pub target: Entity,
    pub source: DamageOrigin,
    pub amount: f32,
    pub attack_type: AttackType,
    pub crit: bool,
}

#[derive(Message, Debug, Clone, Copy)]
pub struct DamageDealt {
    pub target: Entity,
    pub source: DamageOrigin,
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
    hostility: Res<Hostility>,
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
            if !hostility.allied(*side, *aside) || hp.0 <= 0.0 || hp.0 >= max.0 {
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
    pub killer: Option<DamageOrigin>,
}

/// Who caused a piece of damage — the attribution seam for kill credit,
/// balance readouts and skill on-hit hooks. `CommanderSkill`/`TowerSkill`
/// carry the registry def id of the skill (WX1: lifted from warden's
/// DamageSource so BAL attribution survives the migration).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DamageOrigin {
    /// A basic-attack hit from a unit.
    Attack(Entity),
    /// A hero/commander skill cast by that entity.
    Skill { caster: Entity, skill: usize },
    /// Environment / system damage.
    System,
}

/// Homing projectile as a pure logic entity; visuals are the game's business.
#[derive(Component, Debug)]
pub struct Projectile {
    pub target: Entity,
    pub source: DamageOrigin,
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

/// Fold an attack def's base range with the attacker's aggregated
/// `RangeMult` mods (SEAM3b): effective range = def.range x
/// max(range_mult, 0). `None` eff = no `EffectiveStats` yet = neutral 1.0
/// (byte-for-byte the legacy range). The ONE read side is `pick_targets`
/// (fire validates the target it was handed, not the distance); games and
/// panels reuse this for display so the two can never drift.
pub fn attack_range(def: &AttackDef, eff: Option<&crate::buff::EffectiveStats>) -> f32 {
    def.range * eff.map_or(1.0, |e| e.range_mult.max(0.0))
}

/// Intent: every attacker re-picks its target each frame (no sticky locks in
/// v1 — cheap at our unit counts, trivially testable).
#[allow(clippy::type_complexity)]
pub fn pick_targets(
    hostility: Res<Hostility>,
    mut attackers: Query<(&AttackKit, &Transform, &Allegiance, &mut CurrentTarget, Option<&crate::buff::EffectiveStats>)>,
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
    for (kit, pos, side, mut target, eff) in &mut attackers {
        // SEAM3b: range-affix mods (StatKind::RangeMult) multiply the def's
        // base range here — the single targeting read side.
        let range = attack_range(&kit.def, eff);
        // (entity, hp, path rank, dist) for the current best candidate.
        let mut best: Option<(Entity, f32, (u32, f32), f32)> = None;
        for (e, epos, hp, eside, air, follower) in &enemies {
            if !hostility.hostile(*side, *eside) {
                continue;
            }
            if kit.def.targets == TargetDomain::Ground && air {
                continue;
            }
            if kit.def.targets == TargetDomain::Air && !air {
                continue;
            }
            let dist = pos.translation.distance(epos.translation);
            if dist > range {
                continue;
            }
            let rank = follower.map(|f| (f.laps, f.progress)).unwrap_or((0, 0.0));
            let better = match (best, kit.priority) {
                (None, _) => true,
                (Some((_, _bhp, brank, _)), TargetingPriority::First) => rank > brank,
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

/// SKVOC damage-point emitter, shared by the windup-completion path and the
/// legacy immediate path: instant hits write a DamageRequest now; projectile
/// attacks spawn a homing logic entity at the attacker.
fn emit_hit(
    commands: &mut Commands,
    requests: &mut MessageWriter<DamageRequest>,
    attacker: Entity,
    def: &AttackDef,
    pos: &Transform,
    target: Entity,
    damage: f32,
) {
    match &def.projectile {
        None => {
            requests.write(DamageRequest {
                target,
                source: DamageOrigin::Attack(attacker),
                amount: damage,
                attack_type: def.attack_type,
                crit: false,
            });
        }
        Some(proj) => {
            commands.spawn((
                Projectile {
                    target,
                    source: DamageOrigin::Attack(attacker),
                    speed: proj.speed,
                    damage,
                    attack_type: def.attack_type,
                },
                *pos,
            ));
        }
    }
}

/// Act: fire when the cooldown is ready. SKVOC (plan §3) two-phased shape:
///
/// Phase 0 — backswing cancel (minimal pinned semantics): ANY fresh
/// `UnitCommand` naming an `AttackLock`ed unit releases the lock — orders
/// are the only way commanded movement starts, so "order received" IS the
/// "wants to move" signal. Watching `AgentOrderState` for the same signal
/// is useless: `move_agents` rewrites it unconditionally every frame, so
/// its change tick never distinguishes a real Move. Removal is deferred,
/// so the released unit's next windup can start the FOLLOWING frame.
///
/// Phase 1 — advance in-flight `AttackWindup`s and `AttackLock`s. A
/// stunned attacker cancels its windup (no damage, no cooldown burn) and
/// freezes its lock timer; a lost/dead target cancels the windup the same
/// way. On the damage point the hit emits exactly as the legacy code did
/// (via `emit_hit`), the cooldown starts from that moment, and a
/// backswing > 0 attaches `AttackLock`.
///
/// Phase 2 — the legacy loop: cooldown ready + valid live hostile target
/// -> fire NOW when `windup_secs == 0` (byte-for-byte the pre-SKVOC
/// behavior; defaults keep every existing def identical), or attach an
/// `AttackWindup` when `windup_secs > 0` (this frame counts as the first
/// windup frame). A pending windup or an unexpired lock defers the next
/// swing; a stunned attacker enters nothing.
#[allow(clippy::type_complexity)]
pub fn fire_attacks(
    time: Res<Time>,
    hostility: Res<Hostility>,
    mut commands: Commands,
    mut requests: MessageWriter<DamageRequest>,
    mut orders: MessageReader<UnitCommand>,
    mut attackers: Query<
        (
            Entity,
            &AttackKit,
            &Transform,
            &Allegiance,
            &CurrentTarget,
            &mut AttackCooldown,
            Option<&crate::buff::EffectiveStats>,
            Option<&mut AttackWindup>,
            Option<&mut AttackLock>,
            Option<&Stunned>,
        ),
        Without<Dead>,
    >,
    targets: Query<(&Hp, &Allegiance, Option<&Transform>), Without<Dead>>,
    // HIGHLAND (Phase-2): high-ground attack-bonus lookup (identity on flat
    // maps — see TerrainGrid::attack_multiplier; D5: fog never enters here).
    terrain: Res<crate::vision::TerrainGrid>,
) {
    let dt = time.delta_secs();
    // Same half-frame completion tolerance as the skill pipeline (exact
    // frame counts at fixed steps, f32-accumulation-proof).
    let half_frame = dt * 0.5;

    // Phase 0 — order-cancel of backswing locks (see doc comment).
    for cmd in orders.read() {
        for &subject in &cmd.subjects {
            let locked = attackers.get(subject).is_ok_and(|a| a.8.is_some());
            if locked {
                commands.entity(subject).remove::<AttackLock>();
            }
        }
    }

    // Phase 1 — advance committed swings and backswing locks.
    for (entity, kit, pos, _side, _target, mut cd, eff, windup, lock, stunned) in &mut attackers {
        // Lock timer: frozen while stunned; natural expiry is removal.
        if let Some(mut l) = lock {
            if stunned.is_none() {
                l.remaining = (l.remaining - dt).max(0.0);
                if l.remaining <= half_frame {
                    commands.entity(entity).remove::<AttackLock>();
                }
            }
        }
        let Some(mut w) = windup else { continue };
        if stunned.is_some() {
            commands.entity(entity).remove::<AttackWindup>();
            continue;
        }
        // Damage point requires a living target: lost/despawned/zero-hp
        // targets cancel the committed swing (no damage, no cooldown).
        let target_lost = match targets.get(w.target) {
            Ok((hp, _side, _tpos)) => hp.0 <= 0.0,
            Err(_) => true,
        };
        if target_lost {
            commands.entity(entity).remove::<AttackWindup>();
            continue;
        }
        w.remaining -= dt;
        if w.remaining > half_frame {
            continue;
        }
        // Damage point: identical hit + cooldown semantics to the legacy
        // fire, just arriving at the windup's end instead of instantly.
        let eff = eff.copied().unwrap_or_else(crate::buff::EffectiveStats::neutral);
        // HIGHLAND: x(1+bonus) when the attacker's cell is high and the
        // target's is not, measured at FIRE time (launch position — the
        // physically right reading for projectiles and instant hits alike).
        let tpos = targets
            .get(w.target)
            .ok()
            .and_then(|(_, _, tp)| tp.map(|t| t.translation))
            .unwrap_or(pos.translation);
        let damage = kit.def.damage * eff.damage_mult.max(0.0)
            * terrain.attack_multiplier(pos.translation, tpos);
        emit_hit(&mut commands, &mut requests, entity, &kit.def, pos, w.target, damage);
        cd.0 = kit.def.interval * eff.interval_mult.max(0.05);
        commands.entity(entity).remove::<AttackWindup>();
        if kit.def.backswing_secs > 0.0 {
            commands.entity(entity).insert(AttackLock {
                // The fire frame counts as the first locked frame.
                remaining: (kit.def.backswing_secs - dt).max(0.0),
            });
        }
    }

    // Phase 2 — start new attacks (legacy loop + SKVOC gates).
    for (entity, kit, pos, side, target, mut cd, eff, windup, lock, stunned) in &mut attackers {
        // One attack phase at a time: a pending windup or an unexpired
        // backswing lock defers the next swing; stun freezes everything.
        if stunned.is_some() || windup.is_some() || lock.is_some() {
            continue;
        }
        let Some(t) = target.0 else { continue };
        let Ok((hp, tside, tpos)) = targets.get(t) else { continue };
        if hp.0 <= 0.0 || !hostility.hostile(*side, *tside) || cd.0 > 0.0 {
            continue;
        }
        if kit.def.windup_secs > 0.0 {
            // Committed swing: hit lands at the damage point (phase 1).
            commands.entity(entity).insert(AttackWindup {
                // This frame counts as the first windup frame (the same
                // convention as `Casting::elapsed`).
                remaining: (kit.def.windup_secs - dt).max(0.0),
                target: t,
            });
            continue;
        }
        // windup == 0: the exact legacy immediate fire.
        let eff = eff.copied().unwrap_or_else(crate::buff::EffectiveStats::neutral);
        // HIGHLAND: same fire-time bonus as phase 1 (see the comment there).
        let damage = kit.def.damage * eff.damage_mult.max(0.0)
            * terrain.attack_multiplier(
                pos.translation,
                tpos.map(|tp| tp.translation).unwrap_or(pos.translation),
            );
        emit_hit(&mut commands, &mut requests, entity, &kit.def, pos, t, damage);
        cd.0 = kit.def.interval * eff.interval_mult.max(0.05);
        if kit.def.backswing_secs > 0.0 {
            commands.entity(entity).insert(AttackLock {
                remaining: (kit.def.backswing_secs - dt).max(0.0),
            });
        }
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
    for (pe, proj, mut pos) in &mut projs {
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
    // Armor/UnitInstance are Option-tolerant: bare test entities (no armor
    // row, no def) resolve at full damage with zero bounty instead of being
    // silently skipped — a hit must never vanish for a missing component.
    mut units: Query<(
        &mut Hp,
        Option<&Armor>,
        Option<&UnitInstance>,
        Option<&crate::buff::EffectiveStats>,
    )>,
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
        let armor = armor.map(|a| a.0).unwrap_or(ArmorType::Light);
        let mult = table.multiplier(req.attack_type, armor) * taken;
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
            let def = inst
                .map(|i| i.def.clone())
                .unwrap_or_else(|| "unknown".into());
            let (gold, wood, xp) = registry
                .get(&def)
                .map(|d| (d.bounty_gold, d.bounty_wood, d.grant_xp))
                .unwrap_or((0, 0, 0));
            died.write(UnitDied {
                entity: req.target,
                def,
                gold,
                wood,
                xp,
                killer: Some(req.source),
            });
        }
    }
}

/// HERO revive exemption (the ONLY revive-driven change in combat, plan
/// §3): a hero carrying `hero::Reviving` keeps its `Dead` marker (dead
/// semantics ARE the revive window's invisibility/invulnerability — every
/// targeting/attack/heal query already excludes `Dead`) and must NOT be
/// despawned; the hero domain resurrects it when the timer expires. Every
/// other Dead entity despawns exactly as before.
pub fn cleanup_dead(mut commands: Commands, q: Query<Entity, (With<Dead>, Without<crate::hero::Reviving>)>) {
    for e in &q {
        commands.entity(e).despawn();
    }
}
