//! Combat: PlayerAttack (melee slash). Capability cards 2 + 29 (WeaponDefinitionTable).
//! Interface: Space (in) + player Attack/Transform/Heading/EquippedWeapon +
//!   monster Hp/Transform (out: Hp.hp, Visual.flash, Attack.cooldown, Heading).
//! Behavior: cooldown ticks down each frame; Space + cooldown ready slashes
//!   once; per-weapon numbers come from the `WeaponKind` definition table:
//!   damage falls off linearly from full (<= full_range) to zero (far_range)
//!   AND the target must sit within arc_deg/2 of the player's logical facing
//!   (Heading, card 15/18) — a fan, not a ring. Hit monsters flash.

use bevy::prelude::*;

use crate::components::{Attack, EquippedWeapon, Heading, Hp, Monster, Player, Visual};
use crate::resources::Balance;

/// Damage at horizontal distance `d` for max damage `max_dmg` and the
/// per-weapon falloff band `full..=far` (card 29: radii come from the
/// weapon table, not consts):
/// `d <= full` → full; `full..=far` → linear falloff to 0; `> far` → 0.
pub fn damage_at(d: f32, max_dmg: f32, full: f32, far: f32) -> f32 {
    if d <= full {
        max_dmg
    } else if d <= far {
        max_dmg * (far - d) / (far - full)
    } else {
        0.0
    }
}

/// Card 29: is the XZ direction `to` (monster − player) inside the swing arc
/// of `facing` (the player's logical `Heading` unit vector)? Co-located
/// targets (degenerate direction) always count as inside.
fn in_arc(facing: Vec2, to: Vec2, arc_deg: f32) -> bool {
    let len_sq = to.length_squared();
    if len_sq < 1e-8 {
        return true;
    }
    let cos_half = (arc_deg.to_radians() * 0.5).cos();
    facing.dot(to) / len_sq.sqrt() >= cos_half
}

/// Melee slash: tick cooldown, then slash once when Space is held and ready.
/// All numbers come from the equipped weapon's table row; the Balance scales
/// (card 11 F1, multiplier semantics since card 29) tune damage/cooldown live.
pub fn player_attack(
    keys: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    balance: Res<Balance>,
    mut player: Query<(&mut Attack, &Transform, &mut Heading, &EquippedWeapon), With<Player>>,
    mut monsters: Query<(&Transform, &mut Hp, &mut Visual), With<Monster>>,
) {
    let dt = time.delta_secs();

    // Tick cooldown and decide whether a slash fires this frame.
    let mut slash_origin = Vec3::ZERO;
    let mut slash_facing = Vec2::Y;
    let mut weapon = None;
    if let Some((mut attack, tf, heading, equipped)) = player.iter_mut().next() {
        let cooldown = equipped.0.cooldown() * balance.slash_cooldown_scale;
        attack.cooldown = (attack.cooldown - dt).max(0.0);
        if keys.pressed(KeyCode::Space) && attack.cooldown <= 0.0 {
            attack.cooldown = cooldown;
            slash_origin = tf.translation;
            slash_facing = heading.dir;
            weapon = Some(equipped.0);
        }
    }
    let Some(w) = weapon else {
        return;
    };

    // Card 29 (预声明漂移③): aim the logical facing at the nearest target
    // before resolving the fan — stationary slashing stays viable (the team's
    // "attack faces the nearest enemy" fight-QoL goal). derive_heading only
    // re-overwrites this while the player actually moves (card 18 hold rule),
    // so there is no steady-state writer conflict.
    let mut aim: Option<Vec2> = None;
    let mut best_d2 = f32::MAX;
    for (tf, _, _) in monsters.iter() {
        let off = Vec2::new(
            tf.translation.x - slash_origin.x,
            tf.translation.z - slash_origin.z,
        );
        let d2 = off.length_squared();
        if d2 > 1e-8 && d2 < best_d2 {
            best_d2 = d2;
            aim = Some(off / d2.sqrt());
        }
    }
    if let Some(dir) = aim {
        if let Some((_, _, mut heading, _)) = player.iter_mut().next() {
            heading.dir = dir;
        }
        slash_facing = dir;
    }

    for (tf, mut hp, mut visual) in &mut monsters {
        let offset = Vec2::new(
            tf.translation.x - slash_origin.x,
            tf.translation.z - slash_origin.z,
        );
        if !in_arc(slash_facing, offset, w.arc_deg()) {
            continue; // behind or outside the swing fan
        }
        let dmg = damage_at(
            offset.length(),
            w.damage() * balance.slash_damage_scale,
            w.full_range(),
            w.far_range(),
        );
        if dmg > 0.0 {
            hp.hp -= dmg;
            visual.flash = 1.0;
            info!(
                "[combat] {:?} hit monster at d={:.2}, dealt {dmg:.1}, hp now {:.1}",
                w,
                offset.length(),
                hp.hp
            );
        }
    }
}
