//! Placement cursor: WASD steers a marker over the ground plane.
//! Capability card: CursorMove (docs/capability-cards.md).
//! Interface: keys (in) -> Transform.translation (out), constrained to XZ.
//! Behavior: direction = normalized(WASD sum); translation += dir * speed * dt.
//! Acceptance: straight move distance == speed × elapsed time; diagonal speed ==
//!             speed (not speed × sqrt(2)); y stays at the spawn height; paused
//!             state freezes movement.

use bevy::prelude::*;

use crate::components::PlacementCursor;

pub fn spawn_placement_cursor(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // UI2: the yellow cursor cube is hidden by default — the mouse flow (UI1)
    // replaced its visual role and players found it confusing. The entity and
    // movement stay (CursorMove card regression tests still pin them); the
    // legacy keyboard flow (E to place near the cursor) works without visuals.
    commands.spawn((
        PlacementCursor { speed: 8.0 },
        Mesh3d(meshes.add(Cuboid::new(0.9, 0.4, 0.9))),
        MeshMaterial3d(materials.add(Color::srgb(0.9, 0.7, 0.15))),
        Visibility::Hidden,
        Transform::from_xyz(0.0, 0.3, 0.0),
    ));
}

pub fn move_cursor(
    keys: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    mut q: Query<(&mut Transform, &PlacementCursor)>,
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
    for (mut tf, cursor) in &mut q {
        // Keep the cursor on the ground plane (XZ only).
        tf.translation.x += dir.x * cursor.speed * time.delta_secs();
        tf.translation.z += dir.z * cursor.speed * time.delta_secs();
    }
}
