//! Archer tower skill kit (SK9, ADR-0011) — the first tower-owned skill
//! set, exercising all three SkillKind paths:
//!   - 鹰眼 eagle_eye (Passive): crit chance/multiplier as StatMods, synced
//!     idempotently every frame (self-heals across level-ups);
//!   - 多重射击 multishot (Active, no aiming): one extra arrow at each of
//!     the nearest N distinct targets in range (N by level);
//!   - 火焰箭 fire_arrows (Autocast): a timed Damage StatMod for 8s; when
//!     the slot's autocast toggle is on and the tower has a target, the
//!     trigger system re-casts it on its own as soon as the CD is ready.
//! Skills belong to the ENTITY: fusing the archer away consumes them with
//! it (consistent with fusion being "trade units for a slot + new role").

use bevy::prelude::*;
use rand::RngExt; // rand 0.10: random_bool lives on RngExt

use crate::components::{
    EffectiveStats, Enemy, ModSource, Projectile, ProjectileOrigin, SkillCaster, StatMod, StatMods,
    Tower,
};
use crate::resources::{
    RunRng, SkillBalance, SkillDef, SkillKind, SkillTargetMode, StatKind, PROJECTILE_SPEED,
};
use crate::systems::damage::{skill_row, DamageOrigin};
use crate::systems::skills::{learn_skill, CastSkill, SkillCast};
use crate::systems::stats::set_stat_mod;
use crate::systems::tower::{projectile_visual, TowerFired};

/// Registry ids of the archer kit (SK9; rows after the commander's — see
/// `resources::SKILL_ORDER`).
pub const SKILL_EAGLE_EYE: &str = "eagle_eye";
pub const SKILL_MULTISHOT: &str = "multishot";
pub const SKILL_FIRE_ARROWS: &str = "fire_arrows";

/// The archer rows appended to the production registry by
/// `skills::production_skill_defs` (single source of structure; the def's
/// `cooldown` mirrors the table's lv1 value — the cast gate reads the table).
pub fn archer_skill_defs(balance: &SkillBalance) -> [SkillDef; 3] {
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
            SKILL_EAGLE_EYE,
            SkillKind::Passive,
            SkillTargetMode::None,
            3,
            'E',
        ),
        mk(
            SKILL_MULTISHOT,
            SkillKind::Active,
            SkillTargetMode::None,
            3,
            'M',
        ),
        mk(
            SKILL_FIRE_ARROWS,
            SkillKind::Autocast,
            SkillTargetMode::None,
            3,
            'F',
        ),
    ]
}

/// The fresh archer kit at level 1 (SK9 出厂自带): attached by
/// `tower::spawn_base_tower` for the base archer, reused by tests.
pub fn archer_skill_caster() -> SkillCaster {
    let mut caster = SkillCaster { slots: vec![] };
    learn_skill(&mut caster, SKILL_EAGLE_EYE, 1);
    learn_skill(&mut caster, SKILL_MULTISHOT, 1);
    learn_skill(&mut caster, SKILL_FIRE_ARROWS, 1);
    caster
}

/// Sync the eagle-eye passive into the tower's StatMods (SK9): idempotent —
/// re-asserts both crit entries every frame from the CURRENT slot level, so
/// spawn, level-ups and any future re-learn are all covered without hooks.
/// Runs before `stats::aggregate_stats` so the same frame folds the values.
pub fn sync_archer_passives(
    mut commands: Commands,
    balance: Res<SkillBalance>,
    mut casters: Query<(Entity, &SkillCaster, Option<&mut StatMods>)>,
) {
    for (eid, caster, mods) in &mut casters {
        let Some(slot) = caster.slots.iter().find(|s| s.skill == SKILL_EAGLE_EYE) else {
            continue;
        };
        let source = ModSource::Skill {
            caster: eid,
            def: SKILL_EAGLE_EYE,
        };
        let entries = [
            StatMod {
                source,
                kind: StatKind::CritChance,
                add: balance.crit_chance(SKILL_EAGLE_EYE, slot.level),
                mult: 1.0,
                remaining: None,
            },
            StatMod {
                source,
                kind: StatKind::CritMultiplier,
                add: balance.crit_multiplier_add(SKILL_EAGLE_EYE, slot.level),
                mult: 1.0,
                remaining: None,
            },
        ];
        if let Some(mut mods) = mods {
            for entry in entries {
                set_stat_mod(&mut mods, entry);
            }
        } else {
            commands.entity(eid).insert(StatMods(entries.to_vec()));
        }
    }
}

/// The archer kit's confirmed casts (SK9): multishot launches one homing
/// arrow per target; fire arrows is a timed damage StatMod on the tower.
pub fn archer_skill_effects(
    mut casts: MessageReader<SkillCast>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut fired: MessageWriter<TowerFired>,
    towers: Query<(&Tower, &Transform, &EffectiveStats)>,
    mut mods: Query<&mut StatMods>,
    enemies: Query<(Entity, &Transform, &Enemy, &warbase::prelude::Hp)>,
    balance: Res<SkillBalance>,
) {
    for cast in casts.read() {
        match cast.skill.as_str() {
            SKILL_MULTISHOT => {
                let Ok((tower, ttf, estats)) = towers.get(cast.caster) else {
                    continue;
                };
                // Nearest-first, distinct, living targets within range.
                let mut candidates: Vec<(f32, Entity, Vec3)> = enemies
                    .iter()
                    .filter(|(_, etf, _, hp)| {
                        hp.0 > 0.0 && ttf.translation.distance(etf.translation) <= estats.range
                    })
                    .map(|(e, etf, _, _)| (ttf.translation.distance(etf.translation), e, etf.translation))
                    .collect();
                candidates.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
                let want = balance.multishot_targets(SKILL_MULTISHOT, cast.level) as usize;
                let start = ttf.translation + Vec3::Y * 0.9; // bow height (ART12)
                let (mesh, color) = projectile_visual(tower, 0.0);
                let arrows = candidates.len().min(want);
                for (_, target, tpos) in candidates.into_iter().take(want) {
                    commands.spawn((
                        Projectile {
                            origin: ProjectileOrigin::Homing { target },
                            damage: estats.damage,
                            attack_type: tower.attack_type,
                            source: DamageOrigin::Skill {
                                caster: cast.caster,
                                skill: skill_row(SKILL_MULTISHOT),
                            },
                            crit: false,
                            aoe_radius: 0.0,
                            slow_factor: 0.0,
                            slow_duration: 0.0,
                            speed: PROJECTILE_SPEED,
                            last_target_pos: tpos,
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
                }
                if arrows > 0 {
                    // One Throw clip for the volley (the fire-moment seam).
                    fired.write(TowerFired { tower: cast.caster });
                    info!("[sk] multishot lv{} -> {arrows} arrows", cast.level);
                }
            }
            SKILL_FIRE_ARROWS => {
                let bonus = balance.fire_arrow_bonus(SKILL_FIRE_ARROWS, cast.level);
                let duration = balance.fire_arrow_duration(SKILL_FIRE_ARROWS, cast.level);
                let Ok(mut mods) = mods.get_mut(cast.caster) else {
                    continue;
                };
                set_stat_mod(
                    &mut mods,
                    StatMod {
                        source: ModSource::Skill {
                            caster: cast.caster,
                            def: SKILL_FIRE_ARROWS,
                        },
                        kind: StatKind::Damage,
                        add: bonus,
                        mult: 1.0,
                        remaining: Some(duration),
                    },
                );
                info!("[sk] fire arrows lv{}: +{bonus} damage for {duration}s", cast.level);
            }
            _ => {} // not an archer skill
        }
    }
}

/// Autocast trigger for fire arrows (SK9): when the toggle is ON, the slot
/// is ready and the tower currently has a target (acquired by this frame's
/// tower_fire), queue the cast — the framework takes it from there (CD,
/// confirmation, effect). Recast interval therefore ≈ the skill cooldown.
pub fn archer_autocast_fire_arrows(
    towers: Query<(Entity, &Tower, &SkillCaster)>,
    mut requests: MessageWriter<CastSkill>,
) {
    for (eid, tower, caster) in &towers {
        if tower.target.is_none() {
            continue;
        }
        for (slot_idx, slot) in caster.slots.iter().enumerate() {
            if slot.skill == SKILL_FIRE_ARROWS && slot.autocast_on && slot.cooldown <= 0.0 {
                requests.write(CastSkill {
                    caster: eid,
                    slot: slot_idx,
                });
            }
        }
    }
}

/// Roll a critical hit for one shot (SK9): crit_chance comes from the
/// aggregated EffectiveStats (eagle eye folded in); a crit multiplies the
/// snapshot damage by crit_multiplier. Exposed for tower_fire's melee and
/// ranged branches alike.
pub fn roll_crit(rng: &mut RunRng, estats: &EffectiveStats) -> (bool, f32) {
    if estats.crit_chance > 0.0 && rng.0.random_bool(estats.crit_chance as f64) {
        (true, estats.damage * estats.crit_multiplier)
    } else {
        (false, estats.damage)
    }
}
