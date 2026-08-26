//! Player: WASD movement + melee attack state. Capability cards: PlayerMove, PlayerAttack.
//! Movement interface: keys (in) -> Transform.translation (out).
//!   direction = normalized(WASD sum); translation += direction * speed * dt.
//!   Acceptance: 1s straight move == speed (error < 1%); diagonal speed == speed,
//!               not speed * sqrt(2); paused state freezes movement.
//! Attack state: the `Attack` component (cooldown) is consumed by systems::combat::player_attack.
//! Health (card 5): `Hp` (hp/max/invuln) is consumed by systems::contact::contact_damage.
//! Physics (card 4): KinematicPositionBased rigid body + ball collider — rapier reads the
//!   Transform move_player writes, so contact detection can work in a later card.

use bevy::prelude::*;
use bevy_rapier3d::prelude::{Collider, RigidBody};

use crate::components::{Attack, Hp, Player, Visual};

pub fn spawn_player(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    commands.spawn((
        Player { speed: 5.0 },
        Attack { cooldown: 0.0 },
        Hp::full(100.0),
        Visual { flash: 0.0 },
        RigidBody::KinematicPositionBased,
        Collider::ball(0.4),
        Mesh3d(meshes.add(Cuboid::new(0.8, 0.8, 0.8))),
        MeshMaterial3d(materials.add(Color::srgb(0.9, 0.45, 0.2))),
        Transform::from_xyz(0.0, 0.5, 0.0),
    ));
}

pub fn move_player(
    keys: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    mut q: Query<(&mut Transform, &Player)>,
) {
    let mut dir = Vec3::ZERO;
    for (key, axis) in [
        (KeyCode::KeyW, Vec3::Z),
        (KeyCode::KeyS, -Vec3::Z),
        (KeyCode::KeyA, -Vec3::X),
        (KeyCode::KeyD, Vec3::X),
    ] {
        if keys.pressed(key) {
            dir += axis;
        }
    }
    if dir == Vec3::ZERO {
        return;
    }
    let dir = dir.normalize(); // keep diagonal speed equal to straight speed
    for (mut tf, player) in &mut q {
        tf.translation += dir * player.speed * time.delta_secs();
    }
}
