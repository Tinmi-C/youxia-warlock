//! Scene: perspective camera and directional light for the arena.
//! A tower defense usually reads well from a high, slightly angled 3/4 view
//! over the build area; tune it later on a card, not now.

use bevy::prelude::*;

pub fn spawn_camera(mut commands: Commands) {
    // Framing: pulled back far enough that the NEAREST slot row (z = +11)
    // stays above the 120 px bottom command bar and the whole L-map
    // (x −13.5…12.5, z −11…11 + base) fits the 45° fov with margin — the
    // old (0, 16, 20) pose dropped the near row behind the bar (found via
    // projection math + screenshot probe, 2026-09-18). The 0.8 height :
    // distance ratio keeps the same 3/4 viewing angle; the framing is
    // pinned by `camera_framing_covers_all_slots` (tests/behavior.rs).
    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(0.0, 22.0, 27.5).looking_at(Vec3::new(0.0, 0.0, 0.0), Vec3::Y),
    ));
    commands.spawn((
        Transform::from_rotation(Quat::from_euler(EulerRot::ZYX, 0.0, 0.9, -0.6)),
        DirectionalLight {
            shadow_maps_enabled: true,
            ..default()
        },
    ));
}
