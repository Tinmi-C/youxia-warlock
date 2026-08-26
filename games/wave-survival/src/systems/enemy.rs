//! Enemy: chase the player. Capability card 4 (docs/capability-cards.md).
//! Interface: player Transform + monster Chasing.speed (in) -> monster Velocity.linear (out).
//! Behavior: each frame, a chasing monster's velocity points at the player on the XZ
//!   plane at exactly Chasing.speed (Y stays 0: no flying/falling).
//! Physics (rapier): monsters are KinematicVelocityBased rigid bodies (velocity-driven,
//!   no gravity); the player is KinematicPositionBased (moved by move_player, read by rapier).

use bevy::prelude::*;
use bevy_rapier3d::prelude::Velocity;

use crate::components::{Chasing, Player};

/// Steer each chasing monster's velocity toward the player.
pub fn enemy_chase(
    player: Query<&Transform, With<Player>>,
    mut monsters: Query<(&Transform, &Chasing, &mut Velocity)>,
) {
    let Ok(player_tf) = player.single() else {
        return;
    };
    let target = player_tf.translation;

    for (tf, chasing, mut vel) in &mut monsters {
        let delta = target - tf.translation;
        // Chase on the XZ plane (Y stays 0).
        let dir = Vec3::new(delta.x, 0.0, delta.z);
        let dir = if dir.length_squared() > 1e-6 {
            dir.normalize()
        } else {
            Vec3::ZERO // already at the player: stop (avoids jitter at zero distance)
        };
        vel.linear = dir * chasing.speed;
    }
}
