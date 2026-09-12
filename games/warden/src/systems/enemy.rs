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

/// requirements §9 anchor: speed 1.0 should cross the entry->base path in
/// ~20 seconds. The per-point unit rate is derived from the actual path length
/// at spawn time, so changing the map keeps the anchor meaningful.
const ANCHOR_TRAVERSE_SECS: f32 = 20.0;

/// The archetype base color (healer green / armored steel / ordinary red).
/// Shared with the FX domain (FX3 tinting restores to this).
pub fn enemy_base_color(def: &crate::resources::EnemyDef) -> Color {
    if def.healer {
        Color::srgb(0.25, 0.75, 0.35) // healer: green so the aura is observable
    } else if def.physical_armor {
        Color::srgb(0.55, 0.55, 0.6)
    } else {
        Color::srgb(0.8, 0.25, 0.25)
    }
}

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
    spawn_enemy_at(
        commands,
        def,
        index,
        start,
        1,
        None,
        path_units_per_point(path),
        meshes,
        materials,
    );
}

/// World units per speed point for this path (§9 anchor: speed 1.0 crosses
/// the whole path in ~20 s, derived from the actual path length at spawn).
pub fn path_units_per_point(path: &PathInfo) -> f32 {
    let path_len: f32 = path
        .waypoints
        .windows(2)
        .map(|w| w[0].distance(w[1]))
        .sum();
    path_len / ANCHOR_TRAVERSE_SECS
}

/// Spawn one enemy at an explicit position (debug scenarios, DB2): same
/// construction as `spawn_enemy`, plus overrides for the spawn point, the
/// first waypoint to head toward and the starting HP (残血 setups for FX).
pub fn spawn_enemy_at(
    commands: &mut Commands,
    def: &crate::resources::EnemyDef,
    index: usize,
    pos: Vec3,
    next_wp: usize,
    hp_override: Option<f32>,
    units_per_point: f32,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
) {
    let color = enemy_base_color(def);
    let hp = hp_override.unwrap_or(def.hp);
    let eid = commands
        .spawn((
            Enemy {
                hp,
                max_hp: def.hp,
                speed: def.speed * units_per_point,
                leak: def.leak,
                kill_gold: def.kill_gold,
                physical_armor: def.physical_armor,
                def_index: index,
                next_wp,
            },
            Mesh3d(meshes.add(Cuboid::new(0.7, 0.7, 0.7))),
            MeshMaterial3d(materials.add(color)),
            Transform::from_xyz(pos.x, 0.4, pos.z),
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

/// Grant gold and despawn any enemy at or below 0 HP. FX2: a pure-visual
/// death shell spawns alongside — the gold/despawn logic and timing are
/// untouched.
pub fn resolve_death(
    mut commands: Commands,
    boost: Res<Boosts>,
    mut economy: ResMut<Economy>,
    mut wave: ResMut<WaveState>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    enemies: Query<(Entity, &Enemy)>,
    visuals: Query<(&Transform, &MeshMaterial3d<StandardMaterial>)>,
) {
    for (eid, enemy) in &enemies {
        if enemy.hp <= 0.0 {
            let gain = (enemy.kill_gold as f32 * boost.kill_mult).round() as u32;
            economy.gold += gain;
            wave.active = wave.active.saturating_sub(1);
            if let Ok((tf, mat)) = visuals.get(eid) {
                if let Some(color) = materials.get(&mat.0).map(|m| m.base_color) {
                    crate::systems::fx::spawn_death_shell(
                        &mut commands, &mut meshes, &mut materials, tf, color,
                    );
                }
            }
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
