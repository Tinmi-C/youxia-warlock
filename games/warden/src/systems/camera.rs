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
        // D1 晴昼要塞 lighting (docs/art-direction.md, 2026-09-20 re-tone):
        // morning sun — near-white, high angle (~48° above horizon, pitch
        // -0.84 rad) for short readable shadows; the dusk rig (warm low sun,
        // pitch -0.32) was rejected on live viewing. Sky/ambient live in
        // GamePlugin resources.
        Transform::from_rotation(Quat::from_euler(EulerRot::ZYX, 0.0, 0.9, -0.84)),
        DirectionalLight {
            color: Color::srgb(1.0, 0.96, 0.88),
            illuminance: 14_000.0,
            shadow_maps_enabled: true,
            ..default()
        },
    ));
}
