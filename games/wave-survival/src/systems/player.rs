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
use bevy_rapier3d::prelude::{Collider, CollisionGroups, Group, RigidBody};

use crate::components::{Attack, Hp, NovaAttack, Player, PrevTranslation, Visual, WalkCycle};

pub fn spawn_player(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    commands.spawn((
        Player { speed: 5.0 },
        Attack { cooldown: 0.0 },
        NovaAttack { cooldown: 0.0 }, // card 9: independent Shift-nova throttle
        Hp::full(100.0),
        Visual { flash: 0.0 },
        RigidBody::KinematicPositionBased,
        Collider::ball(0.4),
        // Ghost player: no rapier contacts at all. Dynamic monsters must reach
        // the 0.40 bite distance instead of stacking on the collider surface;
        // player movement is position-driven and never relied on responses.
        CollisionGroups::new(Group::GROUP_2, Group::NONE),
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
    // South camera sits on -Z, which mirrors screen-left/right against world X:
    // flip A/D so "D" keeps meaning screen-right (visual-pass fix #1).
    for (key, axis) in [
        (KeyCode::KeyW, Vec3::Z),
        (KeyCode::KeyS, -Vec3::Z),
        (KeyCode::KeyA, Vec3::X),
        (KeyCode::KeyD, -Vec3::X),
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

// --- Walk-state tracking (card 12 HeroPresentation) ---

/// Squared distance below which the player counts as standing still. One frame
/// at 144 fps / speed 5.0 is ~0.035 units (~1.2e-3 squared), so this threshold
/// sits safely beneath any real movement step.
const WALK_MOVE_EPSILON_SQ: f32 = 1e-6;

/// Card 12: mark the player "moving" whenever its position changed since the
/// previous frame. Lives at the END of the Playing chain (after every
/// Transform writer). Components seed themselves via the missing-branch, so no
/// spawn-time change is needed; the frame right after an R-restart teleport may
/// report one spurious `playing` tick before positions re-align (cosmetic only).
pub fn update_walk_cycle(
    mut commands: Commands,
    mut q: Query<
        (
            Entity,
            &Transform,
            Option<&mut WalkCycle>,
            Option<&mut PrevTranslation>,
        ),
        With<Player>,
    >,
) {
    for (entity, tf, walking, prev) in &mut q {
        match (prev, walking) {
            (Some(mut prev), Some(mut walk)) => {
                let moved = prev.v.distance_squared(tf.translation) > WALK_MOVE_EPSILON_SQ;
                walk.playing = moved;
                prev.v = tf.translation;
            }
            _ => {
                commands.entity(entity).insert((
                    WalkCycle { playing: false },
                    PrevTranslation { v: tf.translation },
                ));
            }
        }
    }
}

/// Card 12/13: outside the Playing state nothing moves anyone, so hold every
/// walk flag down (player AND monsters — a paused field must not moonwalk) and
/// re-anchor the previous position (covers the R-restart teleport during
/// GameOver). Registered with `run_if(not(in_state(GameState::Playing)))`.
pub fn clear_walk_on_pause(
    mut q: Query<(&Transform, &mut WalkCycle, Option<&mut PrevTranslation>)>,
) {
    for (tf, mut walk, prev) in &mut q {
        walk.playing = false;
        if let Some(mut prev) = prev {
            prev.v = tf.translation;
        }
    }
}
