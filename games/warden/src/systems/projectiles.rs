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
use crate::systems::damage::DamageRequest;

/// Observability seam for the FX cards (FX2 spawns its impact feedback from
/// these) and for tests: one event per resolved projectile impact.
#[derive(Message, Debug)]
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
    enemies: Query<(Entity, &Transform, &Enemy)>,
) {
    for (eid, mut proj, mut tf) in &mut q {
        // SK12 贯穿箭: fly straight, pierce (damage once each) every enemy
        // within the contact width, expire when the flight length runs out.
        if let ProjectileOrigin::Linear { dir, remaining } = proj.origin {
            let step = (proj.speed * time.delta_secs()).min(remaining);
            tf.translation += dir * step;
            let remaining = remaining - step;
            let mut expired = remaining <= 0.0;
            let mut hit_any = false;
            for (ven, vtf, venemy) in enemies.iter() {
                if venemy.hp <= 0.0 || proj.pierced.contains(&ven) {
                    continue;
                }
                let dx = vtf.translation.x - tf.translation.x;
                let dz = vtf.translation.z - tf.translation.z;
                if dx * dx + dz * dz <= proj.aoe_radius * proj.aoe_radius {
                    proj.pierced.push(ven);
                    hit_any = true;
                    requests.write(DamageRequest {
                        victim: ven,
                        amount: proj.damage,
                        attack_type: proj.attack_type,
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
                .filter(|(_, _, en)| en.hp > 0.0)
                .map(|(_, etf, _)| etf.translation),
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

/// Resolve one projectile impact at `pos`: queue a DamageRequest per victim
/// (the unified pipeline resolves armor/hp/flash/kill credit, SK7) and apply
/// the slow payload here (a status effect, not damage — it stays on the hit).
fn resolve_impact(
    commands: &mut Commands,
    enemies: &Query<(Entity, &Transform, &Enemy)>,
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
    // Collect victims first (read-only pass; the damage lands in
    // resolve_damage later this frame).
    let mut victims: Vec<Entity> = Vec::new();
    if proj.aoe_radius > 0.0 {
        for (ven, vtf, _) in enemies.iter() {
            if vtf.translation.distance(pos) <= proj.aoe_radius {
                victims.push(ven);
            }
        }
    } else if let ProjectileOrigin::Homing { target } = proj.origin {
        // Fixed-point shots are always AOE; single-target only exists homing.
        victims.push(target);
    }
    for ven in victims {
        if let Ok((_, _, en)) = enemies.get(ven) {
            if en.hp <= 0.0 {
                continue; // resolve_damage would skip it anyway
            }
            requests.write(DamageRequest {
                victim: ven,
                amount: proj.damage,
                attack_type: proj.attack_type,
                source: proj.source,
                crit: proj.crit,
            });
            if proj.slow_factor > 0.0 {
                // Shared merge path (SK3 semantics): strongest factor wins.
                crate::systems::tower::apply_slow(
                    commands,
                    ven,
                    proj.slow_factor,
                    proj.slow_duration,
                );
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
