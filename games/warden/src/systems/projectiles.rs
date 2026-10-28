//! Projectiles (PR1/PR2, requirements §5.1): rule-layer rounds. Towers fire
//! HOMING rounds — the round flies toward the target's current position each
//! frame (never misses); skill shots (SK2) fly to a FIXED ground point. Both
//! resolve at impact — single-target hits only the target, AOE blasts
//! everything around the impact point (armor is resolved per victim at hit
//! time; damage was snapshotted at launch). The slow payload rides the hit
//! (TO6 rework). The melee shield tower keeps its instant hit (hybrid model,
//! §5.1). SK12 adds LINEAR rounds: the piercing bolt flies a straight length
//! and damages every enemy it passes through exactly once.

use bevy::prelude::*;

use crate::components::{Enemy, Projectile, ProjectileOrigin};
use crate::resources::{WavePhase, WaveState};
use crate::systems::damage::{to_base_attack, DamageRequest};

/// Observability seam for the FX cards (FX2 spawns its impact feedback from
/// these) and for tests: one event per resolved projectile impact.
#[derive(Message, Debug, Clone, Copy)]
pub struct ProjectileImpact {
    pub position: Vec3,
    pub aoe_radius: f32,
}

/// Fly every projectile toward its destination and resolve impacts (PR1).
/// A homing target that died mid-flight: single-target rounds despawn
/// harmlessly; AOE rounds detonate at the target's last known position (PR2).
/// Fixed-point skill shots always reach their point. Since SK7 impacts only
/// QUEUE DamageRequests — the subtraction happens in
/// `systems::damage::resolve_damage`.
pub fn fly_projectiles(
    time: Res<Time>,
    mut commands: Commands,
    mut impacts: MessageWriter<ProjectileImpact>,
    mut requests: MessageWriter<DamageRequest>,
    // Without<Enemy> keeps the Transform write (projectiles) disjoint from
    // the enemies' Transform read — B0001 otherwise.
    mut q: Query<(Entity, &mut Projectile, &mut Transform), Without<Enemy>>,
    enemies: Query<(Entity, &Transform, &Enemy, &warbase::prelude::Hp)>,
) {
    for (eid, mut proj, mut tf) in &mut q {
        // SK12 贯穿箭: fly straight, pierce (damage once each) every enemy
        // within the contact width, expire when the flight length runs out.
        if let ProjectileOrigin::Linear { dir, remaining } = proj.origin {
            let step = (proj.speed * time.delta_secs()).min(remaining);
            tf.translation += dir * step;
            let remaining = remaining - step;
            let expired = remaining <= 0.0;
            let mut hit_any = false;
            for (ven, vtf, _venemy, vhp) in enemies.iter() {
                if vhp.0 <= 0.0 || proj.pierced.contains(&ven) {
                    continue;
                }
                let dx = vtf.translation.x - tf.translation.x;
                let dz = vtf.translation.z - tf.translation.z;
                if dx * dx + dz * dz <= proj.aoe_radius * proj.aoe_radius {
                    proj.pierced.push(ven);
                    hit_any = true;
                    requests.write(DamageRequest {
                        target: ven,
                        amount: proj.damage,
                        attack_type: to_base_attack(proj.attack_type),
                        source: proj.source,
                        crit: proj.crit,
                    });
                }
            }
            if hit_any || expired {
                impacts.write(ProjectileImpact {
                    position: tf.translation,
                    aoe_radius: 0.0, // pierce feedback is per-victim, not a blast
                });
            }
            if expired {
                proj.origin = ProjectileOrigin::Linear { dir, remaining: 0.0 };
                commands.entity(eid).despawn();
            } else {
                proj.origin = ProjectileOrigin::Linear { dir, remaining };
            }
            continue;
        }
        // Steer toward the destination: the homing target's *current*
        // position (§5.1) or the fixed skill point (SK2). Linear rounds
        // were handled (and skipped) above.
        let target_pos = match proj.origin {
            ProjectileOrigin::Homing { target } => enemies
                .get(target)
                .ok()
                .filter(|(_, _, _, hp)| hp.0 > 0.0)
                .map(|(_, etf, _, _)| etf.translation),
            ProjectileOrigin::Fixed { point } => Some(point),
            ProjectileOrigin::Linear { .. } => continue,
        };
        let Some(pos) = target_pos else {
            // Homing target gone mid-flight (PR2).
            if proj.aoe_radius > 0.0 {
                // AOE rounds still detonate at the last known position.
                let detonate_at = proj.last_target_pos;
                resolve_impact(
                    &mut commands,
                    &enemies,
                    &mut impacts,
                    &mut requests,
                    eid,
                    &mut proj,
                    detonate_at,
                );
            } else {
                // Overkill waste is a real tactical cost (§5.1).
                commands.entity(eid).despawn();
            }
            continue;
        };
        proj.last_target_pos = pos;
        let step = proj.speed * time.delta_secs();
        if tf.translation.distance(pos) <= step {
            resolve_impact(
                &mut commands,
                &enemies,
                &mut impacts,
                &mut requests,
                eid,
                &mut proj,
                pos,
            );
        } else {
            let dir = (pos - tf.translation).normalize();
            tf.translation += dir * step;
        }
    }
}

/// Roster v2 splash command: resolve an AOE impact's victims through the
/// BASE's splash seam (`warbase::area_targets` — Hostility oracle, dead/
/// friendly exclusion, deterministic Entity order) and queue their
/// DamageRequests + slow payloads. A Command (the `ApplySlow` family) so
/// the base helper's `&mut World` fits inside the system-driven flight
/// path; the queue applies at the chain's sync point, still upstream of
/// `resolve_damage` in the same frame.
pub struct SplashHit {
    pub center: Vec3,
    pub radius: f32,
    pub damage: f32,
    pub attack_type: crate::components::AttackType,
    pub source: crate::systems::damage::DamageOrigin,
    pub crit: bool,
    pub slow_factor: f32,
    pub slow_duration: f32,
}

impl Command for SplashHit {
    type Out = ();
    fn apply(self, world: &mut World) {
        for victim in warbase::prelude::area_targets(
            world,
            self.center,
            self.radius,
            warbase::prelude::Allegiance::Defender,
        ) {
            // Zero-hp victims are already excluded by area_targets; the
            // resolver re-checks anyway (SK7 single writer).
            world.write_message(DamageRequest {
                target: victim,
                amount: self.damage,
                attack_type: to_base_attack(self.attack_type),
                source: self.source,
                crit: self.crit,
            });
            if self.slow_factor > 0.0 {
                // Shared merge path (SK3 semantics): strongest factor wins.
                let merged = match world.get::<crate::components::Slow>(victim) {
                    Some(existing) => crate::components::Slow {
                        factor: existing.factor.min(self.slow_factor),
                        timer: self.slow_duration,
                    },
                    None => crate::components::Slow {
                        factor: self.slow_factor,
                        timer: self.slow_duration,
                    },
                };
                world.entity_mut(victim).insert(merged);
            }
        }
    }
}

/// Resolve one projectile impact at `pos`: single-target rounds queue their
/// one DamageRequest inline (the unified pipeline resolves armor/hp/flash/
/// kill credit, SK7); AOE rounds go through the base splash seam
/// (`SplashHit` → `area_targets`, roster v2). The slow payload rides the
/// hit either way (a status effect, not damage — it stays on the hit).
fn resolve_impact(
    commands: &mut Commands,
    enemies: &Query<(Entity, &Transform, &Enemy, &warbase::prelude::Hp)>,
    impacts: &mut MessageWriter<ProjectileImpact>,
    requests: &mut MessageWriter<DamageRequest>,
    proj_eid: Entity,
    proj: &mut Projectile,
    pos: Vec3,
) {
    impacts.write(ProjectileImpact {
        position: pos,
        aoe_radius: proj.aoe_radius,
    });
    if proj.aoe_radius > 0.0 {
        // Roster v2: victims via the base's splash seam (deferred command —
        // applies before resolve_damage in the same frame's chain).
        commands.queue(SplashHit {
            center: pos,
            radius: proj.aoe_radius,
            damage: proj.damage,
            attack_type: proj.attack_type,
            source: proj.source,
            crit: proj.crit,
            slow_factor: proj.slow_factor,
            slow_duration: proj.slow_duration,
        });
    } else if let ProjectileOrigin::Homing { target } = proj.origin {
        // Fixed-point shots are always AOE; single-target only exists homing.
        if let Ok((_, _, _, hp)) = enemies.get(target) {
            if hp.0 > 0.0 {
                requests.write(DamageRequest {
                    target,
                    amount: proj.damage,
                    attack_type: to_base_attack(proj.attack_type),
                    source: proj.source,
                    crit: proj.crit,
                });
                if proj.slow_factor > 0.0 {
                    // Shared merge path (SK3 semantics): strongest wins.
                    crate::systems::tower::apply_slow(
                        commands,
                        target,
                        proj.slow_factor,
                        proj.slow_duration,
                    );
                }
            }
        }
    }
    commands.entity(proj_eid).despawn();
}

/// Wave-end clear (PR2, §5.1): the frame combat resolves back to
/// intermission, every projectile still in flight is removed. Detected via a
/// phase-transition cache (no event ordering constraints).
pub fn clear_projectiles_on_wave_end(
    wave: Res<WaveState>,
    projectiles: Query<Entity, With<Projectile>>,
    mut commands: Commands,
    mut prev_phase: Local<Option<WavePhase>>,
) {
    if *prev_phase == Some(WavePhase::Combat) && wave.phase == WavePhase::Intermission {
        for e in &projectiles {
            commands.entity(e).despawn();
        }
    }
    *prev_phase = Some(wave.phase);
}
