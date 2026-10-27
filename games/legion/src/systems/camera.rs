//! Environment: orthographic top-down camera (the whole 30x16 arena visible)
//! and a directional light. No mouse-pick math lives here.

use bevy::camera::ScalingMode;
use bevy::prelude::*;

/// Marker for the single top-down ortho camera.
#[derive(Component, Debug)]
pub struct TopDownCamera;

/// The arena spans x in [-15, 15], z in [-9, 9] (see maps/arena.ron); show it
/// whole with a one-unit margin on any window aspect.
pub fn spawn_camera(mut commands: Commands) {
    commands.spawn((
        Camera3d::default(),
        TopDownCamera,
        Projection::Orthographic(OrthographicProjection {
            scaling_mode: ScalingMode::AutoMax { max_width: 32.0, max_height: 20.0 },
            ..OrthographicProjection::default_3d()
        }),
        // Above the arena center, looking straight down; screen-up = world -Z.
        Transform::from_xyz(0.0, 40.0, 0.0).looking_at(Vec3::ZERO, Vec3::NEG_Z),
    ));
    commands.spawn((
        Transform::from_rotation(Quat::from_euler(EulerRot::ZYX, 0.0, 0.9, -0.6)),
        DirectionalLight { shadow_maps_enabled: true, ..default() },
    ));
}
