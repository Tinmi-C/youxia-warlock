//! Towers: auto-fire + targeting + one-axis armor, including the fused tower
//! behaviours (Marksman highest-HP priority, AOE blast, slow).
//! Capability cards: TO3 (fire), TO4 (targeting), TO5 (fusion result), TO6 (slow).

use bevy::prelude::*;

use crate::components::{AttackType, Enemy, FusionKind, Slow, Tower, TowerKind};
use crate::resources::Boosts;

/// Resolve one-axis armor: physical is halved vs armored enemies; magic and
/// mixed (穿甲) bypass it.
fn effective_damage(attack_type: AttackType, physical_armor: bool, base: f32) -> f32 {
    if physical_armor && attack_type == AttackType::Physical {
        base * 0.5
    } else {
        base
    }
}

pub fn tower_fire(
    time: Res<Time>,
    boost: Res<Boosts>,
    mut commands: Commands,
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

        // Single-target damage (released before the AOE / slow passes).
        if let Ok((_, _, mut en)) = enemies.get_mut(target) {
            let applied = effective_damage(tower.attack_type, en.physical_armor, base_dmg);
            en.hp -= applied;
            info!("[tower] fired dmg={applied:.1} enemy_hp={:.0}", en.hp);
        }

        // AOE: hit every enemy within the blast radius of the target.
        if tower.aoe_radius > 0.0 {
            let target_pos = enemies.get(target).map(|(_, etf, _)| etf.translation).ok();
            if let Some(tp) = target_pos {
                // Collect the affected entities first (can't mutate while iterating).
                let mut hits: Vec<Entity> = Vec::new();
                for (eid, etf, _) in &enemies {
                    if eid != target && etf.translation.distance(tp) <= tower.aoe_radius {
                        hits.push(eid);
                    }
                }
                for eid in hits {
                    if let Ok((_, _, mut en)) = enemies.get_mut(eid) {
                        let applied =
                            effective_damage(tower.attack_type, en.physical_armor, base_dmg);
                        en.hp -= applied;
                    }
                }
            }
        }

        // Slow: refresh a slow effect on every enemy within tower range.
        if tower.slow_factor > 0.0 {
            for (eid, etf, _) in &enemies {
                if etf.translation.distance(ttf.translation) <= effective_range {
                    commands.entity(eid).insert(Slow {
                        timer: tower.slow_duration,
                        factor: tower.slow_factor,
                    });
                }
            }
        }

        tower.cooldown = 1.0 / (tower.attack_speed * speed_mult);
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
