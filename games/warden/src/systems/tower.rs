//! Towers: auto-fire + targeting + one-axis armor, including the fused tower
//! behaviours (Marksman highest-HP priority, AOE blast, slow).
//! Capability cards: TO3 (fire — resolution moved into projectiles by PR1),
//! TO4 (targeting), TO5 (fusion result), TO6 (slow — on-hit since PR1).
//!
//! Hybrid attack model (requirements §5.1): ranged towers (archer / mage /
//! cannon and every fused tower) LAUNCH a homing projectile (damage snapshot
//! at fire, armor at impact, slow rides the hit); the melee shield tower
//! keeps its instant hit.

use bevy::prelude::*;

use crate::components::{
    AttackType, Enemy, FusionKind, Projectile, ProjectileOrigin, Slow, Tower, TowerKind,
};
use crate::systems::fx::{HitFlash, HIT_FLASH_SECS};
use crate::resources::{Boosts, TowerDef, PROJECTILE_SPEED};

/// Spawn a placed base tower of archetype `def` at world position `pos`
/// (DB1/DB2 debug scenarios; identical construction to a player placement —
/// the player-flow systems keep their own inline builds untouched).
/// Returns the spawned entity.
pub fn spawn_base_tower(
    commands: &mut Commands,
    def: &TowerDef,
    tower_index: usize,
    pos: Vec3,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
) -> bevy::ecs::entity::Entity {
    commands
        .spawn((
            Tower {
                tower_index,
                damage: def.damage,
                attack_speed: def.attack_speed,
                range: def.range,
                attack_type: def.attack_type,
                cooldown: 0.0,
                target: None,
                kind: TowerKind::Base(tower_index),
                aoe_radius: 0.0,
                slow_factor: 0.0,
                slow_duration: 0.0,
                invested: def.cost,
            },
            Mesh3d(meshes.add(Cuboid::new(1.0, 1.0, 1.0))),
            MeshMaterial3d(materials.add(match def.attack_type {
                AttackType::Physical => Color::srgb(0.7, 0.7, 0.8),
                AttackType::Magic => Color::srgb(0.5, 0.4, 0.9),
                AttackType::Mixed => Color::srgb(0.6, 0.9, 0.6),
            })),
            Transform::from_xyz(pos.x, 0.6, pos.z),
        ))
        .id()
}

/// Resolve one-axis armor: physical is halved vs armored enemies; magic and
/// mixed (穿甲) bypass it.
pub fn effective_damage(attack_type: AttackType, physical_armor: bool, base: f32) -> f32 {
    if physical_armor && attack_type == AttackType::Physical {
        base * 0.5
    } else {
        base
    }
}

/// The melee exception of the hybrid model (§5.1): the base shield tower.
fn is_melee(tower: &Tower) -> bool {
    matches!(tower.kind, TowerKind::Base(1))
}

pub fn tower_fire(
    time: Res<Time>,
    boost: Res<Boosts>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut towers: Query<(&mut Tower, &Transform)>,
    mut enemies: Query<(Entity, &Transform, &mut Enemy)>,
) {
    for (mut tower, ttf) in &mut towers {
        tower.cooldown -= time.delta_secs();

        let marksman = tower.kind == TowerKind::Fused(FusionKind::Marksman);

        // AC4 stat boosts (polish): a tower reads the multiplier of its
        // `tower_index` — for a base tower that is its own type; for a fused
        // tower it is the first ingredient's type, so fused towers also benefit.
        let idx = tower.tower_index;
        let dmg_mult = boost.damage_mult.get(idx).copied().unwrap_or(1.0);
        let speed_mult = boost.attack_speed_mult.get(idx).copied().unwrap_or(1.0);
        let range_mult = boost.range_mult.get(idx).copied().unwrap_or(1.0);
        let base_dmg = tower.damage * dmg_mult;
        let effective_range = tower.range * range_mult;

        // Pick target: nearest, or highest-HP for the Marksman.
        let mut best: Option<(Entity, f32, f32)> = None; // (eid, dist, hp)
        for (eid, etf, en) in &enemies {
            if en.hp <= 0.0 {
                continue;
            }
            let d = ttf.translation.distance(etf.translation);
            if d > effective_range {
                continue;
            }
            let better = match best {
                None => true,
                Some((_, bd, bhp)) => {
                    if marksman {
                        en.hp > bhp
                    } else {
                        d < bd
                    }
                }
            };
            if better {
                best = Some((eid, d, en.hp));
            }
        }
        let Some((target, _, _)) = best else {
            continue;
        };
        if tower.cooldown > 0.0 {
            continue;
        }

        if is_melee(&tower) {
            // Melee (shield tower): instant hit — damage + slow on the target
            // only (TO6 rework: the slow rides the hit, §5 original wording).
            // The slow goes through the shared merge path (SK3 semantics).
            if let Ok((_, _, mut en)) = enemies.get_mut(target) {
                let applied = effective_damage(tower.attack_type, en.physical_armor, base_dmg);
                en.hp -= applied;
                commands.entity(target).insert(HitFlash {
                    timer: HIT_FLASH_SECS,
                    original: None,
                });
                if tower.slow_factor > 0.0 {
                    apply_slow(
                        &mut commands,
                        target,
                        tower.slow_factor,
                        tower.slow_duration,
                    );
                }
                info!("[tower] melee hit dmg={applied:.1} enemy_hp={:.0}", en.hp);
            }
        } else {
            // Ranged: launch a homing projectile (§5.1) — damage and type are
            // snapshotted here; armor resolves at impact (projectiles.rs).
            let start = ttf.translation + Vec3::Y * 0.4;
            let (mesh, color) = projectile_visual(&tower, tower.aoe_radius);
            commands.spawn((
                Projectile {
                    origin: ProjectileOrigin::Homing { target },
                    damage: base_dmg,
                    attack_type: tower.attack_type,
                    aoe_radius: tower.aoe_radius,
                    slow_factor: tower.slow_factor,
                    slow_duration: tower.slow_duration,
                    speed: PROJECTILE_SPEED,
                    last_target_pos: enemies
                        .get(target)
                        .map(|(_, etf, _)| etf.translation)
                        .unwrap_or(start),
                },
                Mesh3d(meshes.add(mesh)),
                MeshMaterial3d(materials.add(StandardMaterial {
                    base_color: color,
                    unlit: true,
                    ..default()
                })),
                Transform::from_translation(start),
            ));
        }

        tower.cooldown = 1.0 / (tower.attack_speed * speed_mult);
    }
}

/// Round visuals (§5.1): arrow = grey-white sliver, magic round = purple
/// sphere, cannon round = orange sphere.
fn projectile_visual(tower: &Tower, aoe_radius: f32) -> (Mesh, Color) {
    match tower.attack_type {
        AttackType::Physical if aoe_radius > 0.0 => {
            (Mesh::from(Sphere::new(0.25)), Color::srgb(1.0, 0.6, 0.15))
        }
        AttackType::Physical => (
            Mesh::from(Cuboid::new(0.08, 0.08, 0.55)),
            Color::srgb(0.85, 0.85, 0.85),
        ),
        _ => (Mesh::from(Sphere::new(0.18)), Color::srgb(0.65, 0.4, 1.0)),
    }
}

/// Tick down slow timers on enemies and remove expired effects.
pub fn tick_slow(
    time: Res<Time>,
    mut commands: Commands,
    mut slowed: Query<(Entity, &mut Slow)>,
) {
    for (eid, mut slow) in &mut slowed {
        slow.timer -= time.delta_secs();
        if slow.timer <= 0.0 {
            commands.entity(eid).remove::<Slow>();
        }
    }
}

/// Deferred slow application with coexistence semantics (SK3, requirements
/// §17.2): when a slow is already running, keep the STRONGER factor (the
/// smaller multiplier — e.g. frost nova 0.5 vs bastion 0.7 keeps 0.5) and
/// refresh the timer to the new application's duration. Slows never stack
/// multiplicatively. A command so every application site (bastion hits, the
/// nova) shares one merge path without extra queries.
pub struct ApplySlow {
    pub entity: Entity,
    pub factor: f32,
    pub duration: f32,
}

impl Command for ApplySlow {
    type Out = ();
    fn apply(self, world: &mut World) {
        let merged = match world.get::<Slow>(self.entity) {
            Some(existing) => Slow {
                factor: existing.factor.min(self.factor),
                timer: self.duration,
            },
            None => Slow {
                factor: self.factor,
                timer: self.duration,
            },
        };
        world.entity_mut(self.entity).insert(merged);
    }
}

/// Queue a slow effect with the SK3 merge semantics (see `ApplySlow`).
pub fn apply_slow(commands: &mut Commands, entity: Entity, factor: f32, duration: f32) {
    commands.queue(ApplySlow {
        entity,
        factor,
        duration,
    });
}
