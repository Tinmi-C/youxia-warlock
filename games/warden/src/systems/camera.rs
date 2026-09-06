//! Scene: perspective camera and directional light for the arena.
//! A tower defense usually reads well from a high, slightly angled 3/4 view
//! over the build area; tune it later on a card, not now.

use bevy::prelude::*;

pub fn spawn_camera(mut commands: Commands) {
    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(0.0, 16.0, 20.0).looking_at(Vec3::new(0.0, 0.0, 0.0), Vec3::Y),
    ));
    commands.spawn((
        Transform::from_rotation(Quat::from_euler(EulerRot::ZYX, 0.0, 0.9, -0.6)),
        DirectionalLight {
            shadow_maps_enabled: true,
            ..default()
        },
    ));
}
