//! Environment: orthographic top-down camera (whole arena visible — the base
//! `OrthoTopDownPicker` needs exactly this shape for pointer->world math) and
//! a directional light. No mouse-pick math lives here; picking is the base's.
//!
//! CAM upcast (round-2 plan §3): all camera BEHAVIOR now comes from the base
//! camera domain (`warbase::CameraRtsPlugin`, wired by `WarbasePlugin`) —
//! edge scroll, wheel zoom, control-group double-tap and F1 jumps. This file
//! keeps only the initial rig (placement/projection below) and authors the
//! lab's camera-domain option numbers (ADR-0012: numbers live game-side;
//! base defaults stay identity/off, so untouched apps drift zero).

use bevy::camera::ScalingMode;
use bevy::prelude::*;
use warbase::prelude::{CameraOpts, EdgeScroll};

/// Marker for the single top-down ortho camera (minimap click-to-camera).
#[derive(Component, Debug)]
pub struct TopDownCamera;

/// The arena is a 24x24 grid centered on the origin (see maps/arena.ron);
/// show it whole with a one-cell margin on any window aspect.
pub fn spawn_camera(mut commands: Commands) {
    // Base camera-domain options for the acceptance lab: edge scroll and
    // wheel zoom ON (the wheel clamp range keeps the 26-unit AutoMax frame
    // readable from half-extent to roughly 3x out).
    commands.insert_resource(EdgeScroll { speed_px_per_sec: 480.0, margin_px: 24.0 });
    commands.insert_resource(CameraOpts { zoom_step: 1.1, zoom_min: 0.5, zoom_max: 3.0 });
    commands.spawn((
        Camera3d::default(),
        TopDownCamera,
        Projection::Orthographic(OrthographicProjection {
            scaling_mode: ScalingMode::AutoMax { max_width: 26.0, max_height: 26.0 },
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
