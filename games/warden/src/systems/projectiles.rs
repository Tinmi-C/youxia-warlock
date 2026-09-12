//! Projectiles (PR1/PR2, requirements §5.1): homing rule-layer rounds.
//! Ranged towers launch; the round flies toward the target's current position
//! each frame (never misses), then resolves at impact — single-target hits
//! only the target, AOE blasts everything around the impact point (armor is
//! resolved per victim at hit time; damage was snapshotted at launch). The
//! slow payload rides the hit (TO6 rework). The melee shield tower keeps its
//! instant hit (hybrid model, §5.1).

use bevy::prelude::*;

use crate::components::{Enemy, Projectile, Slow};
use crate::systems::fx::{HitFlash, HIT_FLASH_SECS};
use crate::resources::{WavePhase, WaveState};
use crate::systems::tower::effective_damage;

/// Observability seam for the FX cards (FX2 spawns its impact feedback from
/// these) and for tests: one event per resolved projectile impact.
#[derive(Message, Debug)]
pub struct ProjectileImpact {
    pub position: Vec3,
    pub aoe_radius: f32,
}

/// Fly every projectile toward its target and resolve impacts (PR1).
/// A target that died mid-flight: single-target rounds despawn harmlessly;
/// AOE rounds detonate at the target's last known position (PR2).
pub fn fly_projectiles(
    time: Res<Time>,
    mut commands: Commands,
    mut impacts: MessageWriter<ProjectileImpact>,
    // Without<Enemy> keeps the Transform write (projectiles) disjoint from
    // the enemies' Transform read — B0001 otherwise.
    mut q: Query<(Entity, &mut Projectile, &mut Transform), Without<Enemy>>,
    mut enemies: Query<(Entity, &Transform, &mut Enemy)>,
) {
    for (eid, mut proj, mut tf) in &mut q {
        // Steer toward the target's *current* position (homing, §5.1).
        let target_pos = enemies
            .get(proj.target)
            .ok()
            .filter(|(_, _, en)| en.hp > 0.0)
            .map(|(_, etf, _)| etf.translation);
        let Some(pos) = target_pos else {
            // Target gone mid-flight (PR2).
            if proj.aoe_radius > 0.0 {
                // AOE rounds still detonate at the last known position.
                let detonate_at = proj.last_target_pos;
                resolve_impact(&mut commands, &mut enemies, &mut impacts, eid, &mut proj, detonate_at);
            } else {
                // Overkill waste is a real tactical cost (§5.1).
                commands.entity(eid).despawn();
            }
            continue;
        };
        proj.last_target_pos = pos;
        let step = proj.speed * time.delta_secs();
        if tf.translation.distance(pos) <= step {
            resolve_impact(&mut commands, &mut enemies, &mut impacts, eid, &mut proj, pos);
        } else {
            let dir = (pos - tf.translation).normalize();
            tf.translation += dir * step;
        }
    }
}

/// Resolve one projectile impact at `pos`: damage (+slow payload) to the
/// single target, or to everything within the AOE radius of the blast.
fn resolve_impact(
    commands: &mut Commands,
    enemies: &mut Query<(Entity, &Transform, &mut Enemy)>,
    impacts: &mut MessageWriter<ProjectileImpact>,
    proj_eid: Entity,
    proj: &mut Projectile,
    pos: Vec3,
) {
    impacts.write(ProjectileImpact {
        position: pos,
        aoe_radius: proj.aoe_radius,
    });
    // Collect victims first (can't mutate while iterating the same query).
    let mut victims: Vec<Entity> = Vec::new();
    if proj.aoe_radius > 0.0 {
        for (ven, vtf, _) in enemies.iter() {
            if vtf.translation.distance(pos) <= proj.aoe_radius {
                victims.push(ven);
            }
        }
    } else {
        victims.push(proj.target);
    }
    for ven in victims {
        if let Ok((_, _, mut en)) = enemies.get_mut(ven) {
            let applied = effective_damage(proj.attack_type, en.physical_armor, proj.damage);
            en.hp -= applied;
            commands.entity(ven).insert(HitFlash {
                timer: HIT_FLASH_SECS,
                original: None,
            });
            if proj.slow_factor > 0.0 {
                commands.entity(ven).insert(Slow {
                    timer: proj.slow_duration,
                    factor: proj.slow_factor,
                });
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
