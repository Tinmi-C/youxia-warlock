//! Environment: orthographic top-down camera (whole map visible, keyboard
//! build cursor), directional light. No mouse-pick math needed in v1.

use bevy::camera::ScalingMode;
use bevy::prelude::*;

pub fn spawn_camera(mut commands: Commands) {
    commands.spawn((
        Camera3d::default(),
        Projection::Orthographic(OrthographicProjection {
            scaling_mode: ScalingMode::Fixed { width: 34.0, height: 24.0 },
            ..OrthographicProjection::default_3d()
        }),
        // Above map center, looking straight down; screen-up = world -Z.
        Transform::from_xyz(0.0, 30.0, 0.0).looking_at(Vec3::ZERO, Vec3::NEG_Z),
    ));
    commands.spawn((
        Transform::from_rotation(Quat::from_euler(EulerRot::ZYX, 0.0, 0.9, -0.6)),
        DirectionalLight { shadow_maps_enabled: true, ..default() },
    ));
}

pub fn spawn_environment(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // Ground moved to visuals.rs (needs the map for the path ring).
    let _ = (commands, meshes, materials);
}
