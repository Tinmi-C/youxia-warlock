//! Fusion tower signature actives (SK12, ADR-0011) — the framework's third
//! owner: each fusion tower ships with ONE signature Active (lv1, raised by
//! the shared paid-upgrade button). Fusing consumes the archer kit with the
//! ingredients, so the signature IS the "new mechanism" §7.2 promises:
//!   - 神射手 狙杀 deadeye (None): one sniper arrow at the HIGHEST-HP living
//!     target in range — doubling down on the tower's targeting identity.
//!     Damage = the aggregated normal-attack damage × a level multiplier.
//!   - 大法师 奥术爆轰 arcane_blast (Point): instant magic damage to every
//!     enemy in the blast radius (the nova scan pattern, damage flavour).
//!   - 魔弓手 贯穿箭 piercing_bolt (Point = direction): a Linear projectile
//!     that pierces every enemy along its path exactly once — mixed damage,
//!     echoing the armor-bypass identity.
//!   - 壁垒炮 震荡冲击 concussion (Point): instant damage + slow in radius
//!     (the shared slow-merge path), echoing the AOE+control identity.

use bevy::prelude::*;

use crate::components::{
    AttackType, EffectiveStats, Enemy, FusionKind, Projectile, ProjectileOrigin, SkillCaster, Tower,
};
use crate::resources::{SkillBalance, SkillDef, SkillKind, SkillTargetMode, PROJECTILE_SPEED};
use crate::systems::damage::{DamageRequest, DamageSource};
use crate::systems::skills::{learn_skill, SkillCast};
use crate::systems::tower::{projectile_visual, TowerFired};

/// Registry ids of the fusion signatures (SK12; rows after the archer kit —
/// see `resources::SKILL_ORDER`).
pub const SKILL_DEADEYE: &str = "deadeye";
pub const SKILL_ARCANE_BLAST: &str = "arcane_blast";
pub const SKILL_PIERCING_BOLT: &str = "piercing_bolt";
pub const SKILL_CONCUSSION: &str = "concussion";

/// The fusion rows appended to the production registry by
/// `skills::production_skill_defs` (single source of structure; the def's
/// `cooldown` mirrors the table's lv1 value — the cast gate reads the table).
pub fn fusion_skill_defs(balance: &SkillBalance) -> [SkillDef; 4] {
    let mk = |id: &'static str,
              kind: SkillKind,
              target: SkillTargetMode,
              max_level: u8,
              badge: char| SkillDef {
        id: id.to_string(),
        kind,
        targeting: target,
        max_level,
        cooldown: balance.cooldown(id, 1),
        badge: Some(badge),
        hotkey: None, // SK10 assigns the context hotkeys
    };
    [
        mk(
            SKILL_DEADEYE,
            SkillKind::Active,
            SkillTargetMode::None,
            3,
            'D',
        ),
        mk(
            SKILL_ARCANE_BLAST,
            SkillKind::Active,
            SkillTargetMode::Point,
            3,
            'A',
        ),
        mk(
            SKILL_PIERCING_BOLT,
            SkillKind::Active,
            SkillTargetMode::Point,
            3,
            'P',
        ),
        mk(
            SKILL_CONCUSSION,
            SkillKind::Active,
            SkillTargetMode::Point,
            3,
            'C',
        ),
    ]
}

/// The signature each fusion kind ships with (出厂自带 lv1, SK12).
pub fn fusion_signature(kind: FusionKind) -> &'static str {
    match kind {
        FusionKind::Marksman => SKILL_DEADEYE,
        FusionKind::Archmage => SKILL_ARCANE_BLAST,
        FusionKind::Hybrid => SKILL_PIERCING_BOLT,
        FusionKind::Bastion => SKILL_CONCUSSION,
    }
}

/// The fresh single-slot kit for a fusion result — attached by the fusion
/// spawn gateway (`tower::spawn_fused_tower`).
pub fn fusion_skill_caster(kind: FusionKind) -> SkillCaster {
    let mut caster = SkillCaster { slots: vec![] };
    learn_skill(&mut caster, fusion_signature(kind), 1);
    caster
}

/// Dispatch the confirmed signature casts (SK12). Instant radius effects
/// resolve here; the two arrow skills snapshot damage into projectiles and
/// land through the unified pipeline at impact.
#[allow(clippy::too_many_arguments)]
pub fn fusion_skill_effects(
    mut casts: MessageReader<SkillCast>,
    mut commands: Commands,
    balance: Res<SkillBalance>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut requests: MessageWriter<DamageRequest>,
    mut impacts: MessageWriter<crate::systems::projectiles::ProjectileImpact>,
    mut fired: MessageWriter<TowerFired>,
    towers: Query<(&Tower, &Transform, &EffectiveStats)>,
    // Without<Projectile> keeps this enemies read disjoint from the flight
    // system's projectile query writes (B0001).
    enemies: Query<(Entity, &Transform, &Enemy), Without<Projectile>>,
) {
    for cast in casts.read() {
        match cast.skill.as_str() {
            SKILL_DEADEYE => {
                let Ok((tower, ttf, estats)) = towers.get(cast.caster) else {
                    continue;
                };
                // Highest-HP living target in range — the tower's targeting
                // identity, not the nearest-first default.
                let pick = enemies
                    .iter()
                    .filter(|(_, etf, en)| {
                        en.hp > 0.0 && ttf.translation.distance(etf.translation) <= estats.range
                    })
                    .max_by(|(_, _, a), (_, _, b)| {
                        a.hp.partial_cmp(&b.hp).unwrap_or(std::cmp::Ordering::Equal)
                    });
                let Some((target, tpos, _)) = pick else {
                    continue;
                };
                let mult = balance.deadeye_mult(SKILL_DEADEYE, cast.level);
                let (mesh, color) = projectile_visual(tower, 0.0);
                let start = ttf.translation + Vec3::Y * 0.9;
                commands.spawn((
                    Projectile {
                        origin: ProjectileOrigin::Homing { target },
                        damage: estats.damage * mult,
                        attack_type: tower.attack_type,
                        source: DamageSource::TowerSkill {
                            tower: cast.caster,
                            skill: SKILL_DEADEYE,
                        },
                        crit: false,
                        aoe_radius: 0.0,
                        slow_factor: 0.0,
                        slow_duration: 0.0,
                        speed: PROJECTILE_SPEED,
                        last_target_pos: tpos.translation,
                        pierced: vec![],
                    },
                    Mesh3d(meshes.add(mesh.clone())),
                    MeshMaterial3d(materials.add(StandardMaterial {
                        base_color: color,
                        unlit: true,
                        ..default()
                    })),
                    Transform::from_translation(start),
                ));
                fired.write(TowerFired { tower: cast.caster });
                info!("[sk] deadeye lv{} -> highest-HP target (x{mult})", cast.level);
            }
            SKILL_ARCANE_BLAST => {
                let point = cast.point.unwrap_or_default();
                let radius = balance.radius(SKILL_ARCANE_BLAST, cast.level);
                let damage = balance.arcane_damage(SKILL_ARCANE_BLAST, cast.level);
                blast(
                    &mut requests,
                    &mut impacts,
                    &enemies,
                    point,
                    radius,
                    damage,
                    AttackType::Magic,
                    cast.caster,
                    SKILL_ARCANE_BLAST,
                );
                info!(
                    "[sk] arcane blast lv{} at ({:.1}, {:.1}) r{radius}",
                    cast.level, point.x, point.z
                );
            }
            SKILL_PIERCING_BOLT => {
                let Ok((tower, ttf, _)) = towers.get(cast.caster) else {
                    continue;
                };
                // The picked point only sets the flight direction.
                let point = cast.point.unwrap_or_default();
                let dir = Vec3::new(point.x - ttf.translation.x, 0.0, point.z - ttf.translation.z);
                if dir.length_squared() < 1e-6 {
                    continue;
                }
                let dir = dir.normalize();
                let damage = balance.pierce_damage(SKILL_PIERCING_BOLT, cast.level);
                let (mesh, color) = projectile_visual(tower, 0.0);
                let start = ttf.translation + Vec3::Y * 0.9;
                commands.spawn((
                    Projectile {
                        origin: ProjectileOrigin::Linear {
                            dir,
                            remaining: balance.pierce_length(SKILL_PIERCING_BOLT, cast.level),
                        },
                        damage,
                        attack_type: tower.attack_type, // hybrid: mixed, armor-piercing
                        source: DamageSource::TowerSkill {
                            tower: cast.caster,
                            skill: SKILL_PIERCING_BOLT,
                        },
                        crit: false,
                        // The bolt's contact width doubles as its "AOE".
                        aoe_radius: balance.pierce_width,
                        slow_factor: 0.0,
                        slow_duration: 0.0,
                        speed: PROJECTILE_SPEED * 1.4, // snappier than a lob
                        last_target_pos: start,
                        pierced: vec![],
                    },
                    Mesh3d(meshes.add(mesh.clone())),
                    MeshMaterial3d(materials.add(StandardMaterial {
                        base_color: color,
                        unlit: true,
                        ..default()
                    })),
                    Transform::from_translation(start),
                ));
                fired.write(TowerFired { tower: cast.caster });
                info!("[sk] piercing bolt lv{} toward ({:.1}, {:.1})", cast.level, point.x, point.z);
            }
            SKILL_CONCUSSION => {
                let point = cast.point.unwrap_or_default();
                let radius = balance.radius(SKILL_CONCUSSION, cast.level);
                let damage = balance.concussion_damage(SKILL_CONCUSSION, cast.level);
                let mut victims = blast(
                    &mut requests,
                    &mut impacts,
                    &enemies,
                    point,
                    radius,
                    damage,
                    AttackType::Physical,
                    cast.caster,
                    SKILL_CONCUSSION,
                );
                for ven in victims.drain(..) {
                    crate::systems::tower::apply_slow(
                        &mut commands,
                        ven,
                        balance.concussion_slow_factor,
                        balance.concussion_slow_duration,
                    );
                }
                info!(
                    "[sk] concussion lv{}/{} at ({:.1}, {:.1})",
                    cast.level, SKILL_CONCUSSION, point.x, point.z
                );
            }
            _ => {} // not a fusion signature
        }
    }
}

/// The shared instant-radius pattern (nova's scan + the unified damage
/// pipeline): queue a DamageRequest per living victim, return them so
/// callers can layer status effects (concussion's slow) on the same set.
/// Also emits one `ProjectileImpact` so the FX2 blast ring plays — instant
/// skills ride the same feedback seam as projectile detonations.
fn blast(
    requests: &mut MessageWriter<DamageRequest>,
    impacts: &mut MessageWriter<crate::systems::projectiles::ProjectileImpact>,
    enemies: &Query<(Entity, &Transform, &Enemy), Without<Projectile>>,
    point: Vec3,
    radius: f32,
    damage: f32,
    attack_type: AttackType,
    caster: Entity,
    skill: &'static str,
) -> Vec<Entity> {
    let mut victims = Vec::new();
    for (ven, vtf, venemy) in enemies.iter() {
        let dx = vtf.translation.x - point.x;
        let dz = vtf.translation.z - point.z;
        if venemy.hp > 0.0 && dx * dx + dz * dz <= radius * radius {
            requests.write(DamageRequest {
                victim: ven,
                amount: damage,
                attack_type,
                source: DamageSource::TowerSkill { tower: caster, skill },
                crit: false,
            });
            victims.push(ven);
        }
    }
    if !victims.is_empty() {
        impacts.write(crate::systems::projectiles::ProjectileImpact {
            position: point,
            aoe_radius: radius,
        });
    }
    victims
}
