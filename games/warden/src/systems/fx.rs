//! Combat presentation (FX1–FX3): health bars, hit/death feedback, status
//! visualization. Pure visuals on top of the existing logic — kill gold,
//! despawn timing and every rule stays untouched (requirements §5.1 batch 2).
//!
//!   - FX1: a bar appears above an enemy only once it is damaged (Defender's
//!     Quest convention — no full-health spam); width == hp/max_hp; elites
//!     and bosses get a wider, golden bar.
//!   - FX2: hit flash (<0.15 s), AOE blast rings from `ProjectileImpact`
//!     (<0.5 s), death shells via an OnRemove observer (~0.4 s fade).
//!   - FX3: slowed enemies tint blue while `Slow` lasts; armored enemies
//!     carry a shield badge from spawn; healers keep their green.

use bevy::prelude::*;

use crate::components::{Enemy, Slow};
use crate::resources::EnemyDefs;
use crate::systems::enemy::enemy_base_color;
use crate::systems::projectiles::ProjectileImpact;

// --- FX1: health bars ---------------------------------------------------------

/// Marker on an enemy's health bar (a child entity of the enemy).
#[derive(Component)]
pub struct HealthBar {
    pub enemy: Entity,
}

/// Bar width (world units) for ordinary enemies; elites/bosses are wider.
const BAR_WIDTH: f32 = 0.9;
const BAR_WIDTH_BIG: f32 = 1.4;

/// Maintain health bars: hidden at full hp, width == ratio, wider + golden
/// for elites (4) and bosses (5).
pub fn refresh_health_bars(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    enemies: Query<(Entity, &Enemy)>,
    mut bars: Query<(&HealthBar, &mut Transform, &mut Visibility)>,
) {
    // Ensure every damaged enemy has a bar.
    for (eid, enemy) in &enemies {
        if enemy.hp >= enemy.max_hp || enemy.hp <= 0.0 {
            continue;
        }
        if !bars.iter().any(|(b, _, _)| b.enemy == eid) {
            let big = matches!(enemy.def_index, 4 | 5);
            commands.entity(eid).with_children(|p| {
                p.spawn((
                    HealthBar { enemy: eid },
                    Mesh3d(meshes.add(Cuboid::new(1.0, 0.07, 0.05))),
                    MeshMaterial3d(materials.add(StandardMaterial {
                        base_color: if big {
                            Color::srgb(1.0, 0.8, 0.25)
                        } else {
                            Color::srgb(0.3, 0.9, 0.35)
                        },
                        unlit: true,
                        ..default()
                    })),
                    Visibility::Hidden,
                    Transform::from_xyz(0.0, 0.9, 0.0),
                ));
            });
        }
    }
    // Sync visibility + width.
    for (bar, mut tf, mut vis) in &mut bars {
        let Ok((_, enemy)) = enemies.get(bar.enemy) else {
            continue;
        };
        let damaged = enemy.hp < enemy.max_hp && enemy.hp > 0.0;
        *vis = if damaged { Visibility::Visible } else { Visibility::Hidden };
        if damaged {
            let base = if matches!(enemy.def_index, 4 | 5) {
                BAR_WIDTH_BIG
            } else {
                BAR_WIDTH
            };
            tf.scale.x = (enemy.hp / enemy.max_hp).max(0.01) * base;
        }
    }
}

// --- FX2: hit flash, blast rings, death shells --------------------------------

/// A white flash on a damaged enemy; expires back to the archetype color.
#[derive(Component)]
pub struct HitFlash {
    pub timer: f32,
    pub original: Option<Color>,
}

const HIT_FLASH_SECS: f32 = 0.15;
const BLAST_RING_SECS: f32 = 0.5;
const DEATH_SHELL_SECS: f32 = 0.4;

/// Tick hit flashes: capture the original color once, flash white, restore.
pub fn tick_hit_flash(
    time: Res<Time>,
    mut commands: Commands,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut q: Query<(Entity, &mut HitFlash, &MeshMaterial3d<StandardMaterial>)>,
) {
    for (eid, mut flash, mat) in &mut q {
        flash.timer -= time.delta_secs();
        if let Some(mut m) = materials.get_mut(&mat.0) {
            if flash.original.is_none() {
                flash.original = Some(m.base_color);
                m.base_color = Color::srgb(1.0, 1.0, 0.95);
            }
            if flash.timer <= 0.0 {
                if let Some(orig) = flash.original {
                    m.base_color = orig;
                }
                commands.entity(eid).remove::<HitFlash>();
            }
        }
    }
}

/// Marker on an expanding AOE blast ring.
#[derive(Component)]
pub struct BlastRing {
    pub timer: f32,
    pub max_radius: f32,
}

/// FX2: on every AOE impact, spawn an expanding ring at the blast point.
pub fn spawn_blast_rings(
    mut impacts: MessageReader<ProjectileImpact>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for impact in impacts.read() {
        if impact.aoe_radius <= 0.0 {
            continue;
        }
        commands.spawn((
            BlastRing {
                timer: BLAST_RING_SECS,
                max_radius: impact.aoe_radius,
            },
            Mesh3d(meshes.add(Circle::new(1.0))),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: Color::srgba(1.0, 0.7, 0.25, 0.6),
                unlit: true,
                alpha_mode: AlphaMode::Blend,
                ..default()
            })),
            Visibility::Visible,
            Transform::from_xyz(impact.position.x, 0.1, impact.position.z)
                .with_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)),
        ));
    }
}

/// Expand + fade blast rings, despawn after BLAST_RING_SECS.
pub fn tick_blast_rings(
    time: Res<Time>,
    mut commands: Commands,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut q: Query<(Entity, &mut BlastRing, &mut Transform, &MeshMaterial3d<StandardMaterial>)>,
) {
    for (eid, mut ring, mut tf, mat) in &mut q {
        ring.timer -= time.delta_secs();
        let t = 1.0 - (ring.timer / BLAST_RING_SECS).clamp(0.0, 1.0); // 0→1
        let r = ring.max_radius * (0.2 + 0.8 * t);
        tf.scale = Vec3::splat(r);
        if let Some(mut m) = materials.get_mut(&mat.0) {
            m.base_color.set_alpha(0.6 * (1.0 - t));
        }
        if ring.timer <= 0.0 {
            commands.entity(eid).despawn();
        }
    }
}

/// Marker on a death shell (pure visual; the logic despawn already happened).
#[derive(Component)]
pub struct DeathShell {
    pub timer: f32,
}

/// FX2: spawn a dissolving shell for a dying enemy. Called from
/// `enemy::resolve_death` right before the despawn — the gold/despawn logic
/// and timing stay untouched; this only adds the visual.
pub fn spawn_death_shell(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    tf: &Transform,
    color: Color,
) {
    commands.spawn((
        DeathShell {
            timer: DEATH_SHELL_SECS,
        },
        Mesh3d(meshes.add(Cuboid::new(0.7, 0.7, 0.7))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: color,
            unlit: true,
            alpha_mode: AlphaMode::Blend,
            ..default()
        })),
        Visibility::Visible,
        Transform::from_translation(tf.translation),
    ));
}

/// Shrink + fade death shells, despawn after DEATH_SHELL_SECS.
pub fn tick_death_shells(
    time: Res<Time>,
    mut commands: Commands,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut q: Query<(Entity, &mut DeathShell, &mut Transform, &MeshMaterial3d<StandardMaterial>)>,
) {
    for (eid, mut shell, mut tf, mat) in &mut q {
        shell.timer -= time.delta_secs();
        let t = (shell.timer / DEATH_SHELL_SECS).clamp(0.0, 1.0); // 1→0
        tf.scale = Vec3::splat(t.max(0.05));
        if let Some(mut m) = materials.get_mut(&mat.0) {
            m.base_color.set_alpha(t);
        }
        if shell.timer <= 0.0 {
            commands.entity(eid).despawn();
        }
    }
}

// --- FX3: status visualization --------------------------------------------------

/// The blue tint of a slowed enemy.
fn slow_tint() -> Color {
    Color::srgb(0.35, 0.55, 1.0)
}

/// Marker + flat plate on armored (shield) enemies, visible from spawn.
#[derive(Component)]
pub struct ShieldBadge;

/// FX3: slowed enemies tint blue while `Slow` lasts and restore the archetype
/// color when it expires. Runs after damage so it wins the steady state.
pub fn tint_slowed(
    mut materials: ResMut<Assets<StandardMaterial>>,
    defs: Res<EnemyDefs>,
    q: Query<(&Enemy, &MeshMaterial3d<StandardMaterial>, Has<Slow>, Has<HitFlash>)>,
) {
    for (enemy, mat, slowed, flashing) in &q {
        if flashing {
            continue; // the hit flash owns the color while it lasts
        }
        let Some(mut m) = materials.get_mut(&mat.0) else {
            continue;
        };
        let want = if slowed {
            slow_tint()
        } else {
            defs.list
                .get(enemy.def_index)
                .map(enemy_base_color)
                .unwrap_or_else(|| Color::srgb(0.8, 0.25, 0.25))
        };
        // Avoid fighting the hit flash: only write when actually different.
        if m.base_color != want {
            m.base_color = want;
        }
    }
}

/// FX3: armored enemies carry a shield badge from spawn.
pub fn attach_shield_badges(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    enemies: Query<(Entity, &Enemy), Added<Enemy>>,
    badged: Query<&ShieldBadge>,
) {
    for (eid, enemy) in &enemies {
        if !enemy.physical_armor || badged.get(eid).is_ok() {
            continue;
        }
        commands.entity(eid).with_children(|p| {
            p.spawn((
                ShieldBadge,
                Mesh3d(meshes.add(Cuboid::new(0.85, 0.12, 0.85))),
                MeshMaterial3d(materials.add(StandardMaterial {
                    base_color: Color::srgb(0.45, 0.7, 0.95),
                    unlit: true,
                    ..default()
                })),
                Visibility::Visible,
                Transform::from_xyz(0.0, 0.42, 0.0),
            ));
        });
    }
}
