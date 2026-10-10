//! Combat presentation (FX1–FX3): health bars, hit/death feedback, status
//! visualization. Pure visuals on top of the existing logic — kill gold,
//! despawn timing and every rule stays untouched (requirements §5.1 batch 2).
//!
//! WX4 (ADR-0012): the game-agnostic primitives — hit flash, blast rings,
//! death shells — now come from `warbase::fx` (re-exported below under their
//! historical warden paths). What stays warden-side by design:
//!   - FX1 health bars: the data source is the warden `Enemy` god-component
//!     (hp/max_hp live inside it); they switch to the base's Hp/MaxHp-driven
//!     bars with the WX6 component split (one-line swap then).
//!   - FX3 status visuals: slow tint restores the archetype color from
//!     `EnemyDefs`, the shield badge reads `physical_armor` — game identity.
//!   - The `ProjectileImpact` observability message stays warden's (tests
//!     and multiple writers); `forward_blast_feedback` bridges it into the
//!     base's `FxBlast` so the base ring system renders it.

use bevy::prelude::*;

use crate::components::{Enemy, Slow};
use crate::resources::EnemyDefs;
use crate::systems::enemy::enemy_base_color;
use crate::systems::projectiles::ProjectileImpact;

// Base FX primitives under their historical warden paths (WX4).
pub use warbase::fx::spawn_death_shell;
pub use warbase::fx::{BlastRing, DeathShell, HitFlash, HIT_FLASH_SECS};

// --- FX1: health bars (warden-local until the WX6 Enemy split) -------------------

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

// --- FX2 bridge: warden impacts -> base blast rings (WX4) ------------------------

/// Forward warden's `ProjectileImpact` observability message into the base's
/// `FxBlast` so `warbase::fx::spawn_blast_rings` renders the feedback. Runs
/// chained before the base spawner in Observe.
pub fn forward_blast_feedback(
    mut impacts: MessageReader<ProjectileImpact>,
    mut blasts: MessageWriter<warbase::fx::FxBlast>,
) {
    for impact in impacts.read() {
        blasts.write(warbase::fx::FxBlast {
            position: impact.position,
            radius: impact.aoe_radius,
        });
    }
}

// --- FX3: status visualization (game identity, stays warden) ---------------------

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
