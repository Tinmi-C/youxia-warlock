//! Environment: camera, directional light, ground.

use bevy::prelude::*;

pub fn spawn_camera(mut commands: Commands) {
    // South-elevated three-quarter view. The humanoid glTF models face +Z, so
    // viewing from -Z reads them back-to-camera and W (+Z) walks into the
    // screen instead of into the lens (first visual-pass feedback: input felt
    // mirrored when the camera sat on +Z staring at the character's face).
    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(0.0, 8.0, -9.5).looking_at(Vec3::new(0.0, 0.4, 0.0), Vec3::Y),
    ));
    commands.spawn((
        Transform::from_rotation(Quat::from_euler(EulerRot::ZYX, 0.0, 0.9, -0.6)),
        DirectionalLight {
            shadow_maps_enabled: true,
            ..default()
        },
    ));
}

pub fn spawn_environment(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    commands.spawn((
        Mesh3d(meshes.add(Plane3d::default().mesh().size(40.0, 40.0))),
        MeshMaterial3d(materials.add(Color::srgb(0.22, 0.27, 0.22))),
    ));
}
