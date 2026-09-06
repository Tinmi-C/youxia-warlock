//! Enemies: spawn, movement along the path, leak (base damage), kill (gold),
//! and the healer aura (EN5).
//! Capability cards: EN1 (defs), EN2 (move/leak), EN3 (death/gold), EN5 (healer).

use bevy::prelude::*;

use crate::components::{Enemy, Healer, Slow};
use crate::resources::{BaseHp, Boosts, Economy, EnemyDefs, PathInfo, WaveState};

/// EN5 starting values (requirements §9 gives no numbers — tunable on the
/// balance card): heal 3 hp every second within 6 world units.
const HEALER_RADIUS: f32 = 6.0;
const HEALER_PERIOD: f32 = 1.0;
const HEALER_HEAL_PER_TICK: f32 = 3.0;

/// Spawn one enemy of archetype `index` at the path entry.
pub fn spawn_enemy(
    commands: &mut Commands,
    defs: &EnemyDefs,
    path: &PathInfo,
    index: usize,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
) {
    let Some(def) = defs.list.get(index) else {
        return;
    };
    let start = path.waypoints.first().copied().unwrap_or(Vec3::ZERO);
    let color = if def.healer {
        Color::srgb(0.25, 0.75, 0.35) // healer: green so the aura is observable
    } else if def.physical_armor {
        Color::srgb(0.55, 0.55, 0.6)
    } else {
        Color::srgb(0.8, 0.25, 0.25)
    };
    let eid = commands
        .spawn((
            Enemy {
                hp: def.hp,
                max_hp: def.hp,
                speed: def.speed,
                leak: def.leak,
                kill_gold: def.kill_gold,
                physical_armor: def.physical_armor,
                next_wp: 1,
            },
            Mesh3d(meshes.add(Cuboid::new(0.7, 0.7, 0.7))),
            MeshMaterial3d(materials.add(color)),
            Transform::from_xyz(start.x, 0.4, start.z),
        ))
        .id();
    if def.healer {
        commands.entity(eid).insert(Healer {
            radius: HEALER_RADIUS,
            period: HEALER_PERIOD,
            heal_per_tick: HEALER_HEAL_PER_TICK,
            tick_timer: HEALER_PERIOD,
        });
    }
}

/// Advance every live enemy toward its next waypoint. On reaching the last one
/// it leaks to the base, deducting HP and despawning.
pub fn move_enemy(
    time: Res<Time>,
    path: Res<PathInfo>,
    mut base: ResMut<BaseHp>,
    mut wave: ResMut<WaveState>,
    mut commands: Commands,
    mut enemies: Query<(Entity, &mut Enemy, &mut Transform)>,
    slowed: Query<&Slow>,
) {
    for (eid, mut enemy, mut tf) in &mut enemies {
        // Dead enemies are cleaned up in `resolve_death`; do not move them.
        if enemy.hp <= 0.0 {
            continue;
        }
        if enemy.next_wp >= path.waypoints.len() {
            // Reached the base.
            base.hp = base.hp.saturating_sub(enemy.leak);
            wave.active = wave.active.saturating_sub(1);
            commands.entity(eid).despawn();
            info!("[enemy] leaked, base hp={}", base.hp);
            continue;
        }
        let mut speed = enemy.speed;
        if let Ok(slow) = slowed.get(eid) {
            speed *= slow.factor;
        }
        let target = path.waypoints[enemy.next_wp];
        let mut dir = target - tf.translation;
        dir.y = 0.0;
        if dir.length() <= 0.05 {
            enemy.next_wp += 1;
            continue;
        }
        let step = (speed * time.delta_secs()).min(dir.length());
        tf.translation += dir.normalize() * step;
    }
}

/// Grant gold and despawn any enemy at or below 0 HP.
pub fn resolve_death(
    mut commands: Commands,
    boost: Res<Boosts>,
    mut economy: ResMut<Economy>,
    mut wave: ResMut<WaveState>,
    enemies: Query<(Entity, &Enemy)>,
) {
    for (eid, enemy) in &enemies {
        if enemy.hp <= 0.0 {
            let gain = (enemy.kill_gold as f32 * boost.kill_mult).round() as u32;
            economy.gold += gain;
            wave.active = wave.active.saturating_sub(1);
            commands.entity(eid).despawn();
            info!("[enemy] killed, gold={}", economy.gold);
        }
    }
}

/// EN5: healers periodically restore hp of nearby damaged allies (never
/// themselves). Overheal is clamped to max_hp.
pub fn heal_aura(
    time: Res<Time>,
    mut healers: Query<(Entity, &Transform, &mut Healer)>,
    mut enemies: Query<(Entity, &mut Enemy, &Transform)>,
) {
    for (healer_e, htf, mut healer) in &mut healers {
        healer.tick_timer -= time.delta_secs();
        if healer.tick_timer > 0.0 {
            continue;
        }
        healer.tick_timer = healer.period;
        for (eid, mut enemy, etf) in &mut enemies {
            if eid == healer_e || enemy.hp <= 0.0 || enemy.hp >= enemy.max_hp {
                continue;
            }
            if htf.translation.distance(etf.translation) <= healer.radius {
                enemy.hp = (enemy.hp + healer.heal_per_tick).min(enemy.max_hp);
                info!("[enemy] healer aura -> hp={:.0}", enemy.hp);
            }
        }
    }
}
