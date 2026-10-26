//! CAM domain acceptance (round-2 plan §3 / ADR-0018 Phase-1): edge-scroll
//! parameters + pure delta math, control-group double-tap jumps, F1 hero
//! focus, wheel zoom math, and the minimap ping round-trip. Layering under
//! test mirrors the module: message/state logic runs headless through
//! domain messages (`GroupCommand::Recall`) or an injected keyboard
//! resource; the window/camera-touching halves are exercised only through
//! spawned-camera systems and pure functions (no window exists headless).
//!
//! Numbering matches the acceptance list: ① group double-tap CameraJump;
//! ② F1 hero jump; ③ MinimapPing round-trip; ④ defaults idle + pure
//! math (four directions / zoom clamps); ⑤ rts-lab zero drift lives in
//! games/rts-lab/tests/behavior.rs (this file never touches game crates).

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::input::mouse::{MouseScrollUnit, MouseWheel};
use bevy::input::touch::TouchPhase;
use bevy::input::ButtonInput;
use bevy::prelude::*;
use warbase::prelude::*;
use warbase::testing::test_app;

// -- fixtures ------------------------------------------------------------------

fn unit_at(world: &mut World, x: f32, z: f32) -> Entity {
    world.spawn(Transform::from_xyz(x, 0.0, z)).id()
}

fn hero_at(world: &mut World, x: f32, z: f32, side: Allegiance) -> Entity {
    world
        .spawn((
            XpHolder {
                xp: 0,
                level: 1,
                points: 0,
                next_level_xp: 10,
                talents_taken: 0,
            },
            HeroStats { str: 1, agi: 1, int: 1 },
            side,
            Transform::from_xyz(x, 0.0, z),
        ))
        .id()
}

/// Inject ONE key tap as a fresh `ButtonInput` (MinimalPlugins has no
/// clear system, so a stale resource would keep `just_pressed` true every
/// frame — each frame re-inserts a clean state; see input/mod.rs SEL1a 5).
fn press(app: &mut App, code: KeyCode) {
    let mut keys = ButtonInput::<KeyCode>::default();
    keys.press(code);
    app.insert_resource(keys);
}

/// Clear the injected keyboard (no key pressed this frame).
fn release_keys(app: &mut App) {
    app.insert_resource(ButtonInput::<KeyCode>::default());
}

fn drain_jumps(app: &mut App, cursor: &mut MessageCursor<CameraJump>) -> Vec<CameraJump> {
    let msgs = app.world().resource::<Messages<CameraJump>>();
    cursor.read(msgs).copied().collect()
}

fn ortho_camera(world: &mut World, scale: f32) -> Entity {
    let transform = Transform::from_xyz(5.0, 40.0, 7.0);
    world
        .spawn((
            Camera::default(),
            Projection::Orthographic(OrthographicProjection {
                scale,
                ..OrthographicProjection::default_3d()
            }),
            transform,
            GlobalTransform::from(transform),
        ))
        .id()
}

fn wheel_lines(y: f32) -> MouseWheel {
    MouseWheel {
        unit: MouseScrollUnit::Line,
        x: 0.0,
        y,
        window: Entity::PLACEHOLDER,
        phase: TouchPhase::Moved,
    }
}

// -- ① group double-tap -> CameraJump -------------------------------------------

/// Two presses of the same digit key inside 0.3s -> exactly ONE CameraJump
/// at the group's FIRST slot member (slot order, not spawn order); a third
/// quick tap is a fresh pair and never fires a second jump.
#[test]
fn group_double_tap_jumps_to_first_member() {
    let mut app = test_app();
    let first = unit_at(app.world_mut(), 3.0, 4.0);
    let second = unit_at(app.world_mut(), 9.0, 9.0);
    // Slot vector order IS the jump order (Bind clones the index-sorted
    // selection in real play; direct mutation pins the contract): the
    // slot head here is `second`, deliberately NOT the lower spawn index.
    app.world_mut()
        .resource_mut::<ControlGroups>()
        .groups[0] = vec![second, first];

    let mut jumps = MessageCursor::<CameraJump>::default();
    press(&mut app, KeyCode::Digit1);
    app.update();
    release_keys(&mut app);
    app.update();
    press(&mut app, KeyCode::Digit1);
    app.update();
    release_keys(&mut app);
    app.update();
    let seen = drain_jumps(&mut app, &mut jumps);
    assert_eq!(seen.len(), 1, "one jump per double tap: {seen:?}");
    assert_eq!(seen[0].to, Vec2::new(9.0, 9.0), "to = first SLOT member");

    // The fired pair is consumed: a third tap within the window is a fresh
    // first tap, not another double.
    press(&mut app, KeyCode::Digit1);
    app.update();
    release_keys(&mut app);
    app.update();
    assert!(
        drain_jumps(&mut app, &mut jumps).is_empty(),
        "triple tap = one double, not two"
    );
}

/// Double-tapping a digit with an EMPTY group never jumps.
#[test]
fn group_double_tap_empty_group_never_jumps() {
    let mut app = test_app();
    let mut jumps = MessageCursor::<CameraJump>::default();
    press(&mut app, KeyCode::Digit2);
    app.update();
    release_keys(&mut app);
    app.update();
    press(&mut app, KeyCode::Digit2);
    app.update();
    release_keys(&mut app);
    app.update();
    assert!(
        drain_jumps(&mut app, &mut jumps).is_empty(),
        "empty group double tap: no jump"
    );
}

/// The double must be the SAME digit: 1 then 2 inside the window jumps
/// nowhere even though both groups are populated.
#[test]
fn group_double_tap_requires_same_digit() {
    let mut app = test_app();
    let a = unit_at(app.world_mut(), 1.0, 1.0);
    let b = unit_at(app.world_mut(), 2.0, 2.0);
    app.world_mut()
        .resource_mut::<ControlGroups>()
        .groups[0] = vec![a];
    app.world_mut()
        .resource_mut::<ControlGroups>()
        .groups[1] = vec![b];

    let mut jumps = MessageCursor::<CameraJump>::default();
    press(&mut app, KeyCode::Digit1);
    app.update();
    release_keys(&mut app);
    app.update();
    press(&mut app, KeyCode::Digit2);
    app.update();
    release_keys(&mut app);
    app.update();
    assert!(
        drain_jumps(&mut app, &mut jumps).is_empty(),
        "1-then-2 is not a double tap"
    );
}

/// Two taps of the same digit FAR apart (25 frames > 0.3s) never jump.
#[test]
fn group_double_tap_window_expires() {
    let mut app = test_app();
    let a = unit_at(app.world_mut(), 1.0, 1.0);
    app.world_mut()
        .resource_mut::<ControlGroups>()
        .groups[0] = vec![a];

    let mut jumps = MessageCursor::<CameraJump>::default();
    press(&mut app, KeyCode::Digit1);
    app.update();
    release_keys(&mut app);
    app.update();
    for _ in 0..25 {
        app.update(); // ~0.42s of virtual time > the 0.3s window
    }
    press(&mut app, KeyCode::Digit1);
    app.update();
    release_keys(&mut app);
    app.update();
    assert!(
        drain_jumps(&mut app, &mut jumps).is_empty(),
        "slow re-tap is two singles"
    );
}

/// Pure-message path (no keyboard at all): two `GroupCommand::Recall`s one
/// frame apart produce the jump — the double-tap logic lives downstream of
/// the digit translation seam, so it stays headless/drive-by-message.
#[test]
fn group_double_tap_consumes_recall_messages_directly() {
    let mut app = test_app();
    let a = unit_at(app.world_mut(), -2.0, 6.0);
    app.world_mut()
        .resource_mut::<ControlGroups>()
        .groups[2] = vec![a];

    let mut jumps = MessageCursor::<CameraJump>::default();
    app.world_mut().write_message(GroupCommand::Recall(3));
    app.update();
    app.world_mut().write_message(GroupCommand::Recall(3));
    app.update();
    let seen = drain_jumps(&mut app, &mut jumps);
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].to, Vec2::new(-2.0, 6.0));
}

// -- ② F1 -> CameraJump ----------------------------------------------------------

/// F1 with two own heroes on the field: exactly one jump, target = the
/// LOWER Entity index hero (D6 determinism).
#[test]
fn f1_jumps_to_lowest_index_own_hero() {
    let mut app = test_app();
    let h1 = hero_at(app.world_mut(), 1.0, 1.0, Allegiance::Defender);
    let h2 = hero_at(app.world_mut(), -7.0, 5.0, Allegiance::Defender);
    let expected = if h1.index() < h2.index() {
        Vec2::new(1.0, 1.0)
    } else {
        Vec2::new(-7.0, 5.0)
    };

    let mut jumps = MessageCursor::<CameraJump>::default();
    press(&mut app, KeyCode::F1);
    app.update();
    release_keys(&mut app);
    app.update();
    let seen = drain_jumps(&mut app, &mut jumps);
    assert_eq!(seen.len(), 1, "exactly one jump: {seen:?}");
    assert_eq!(seen[0].to, expected, "to = lowest-index own hero");
}

/// F1 with no heroes on the field: no jump.
#[test]
fn f1_without_heroes_never_jumps() {
    let mut app = test_app();
    let mut jumps = MessageCursor::<CameraJump>::default();
    press(&mut app, KeyCode::F1);
    app.update();
    release_keys(&mut app);
    app.update();
    assert!(drain_jumps(&mut app, &mut jumps).is_empty());
}

/// F1 is own-side AND full-hero only: an enemy XpHolder+HeroStats unit and
/// a friendly XpHolder-without-HeroStats unit both fail the query.
#[test]
fn f1_ignores_enemy_and_partial_heroes() {
    let mut app = test_app();
    hero_at(app.world_mut(), 4.0, 4.0, Allegiance::Attacker);
    app.world_mut().spawn((
        XpHolder {
            xp: 0,
            level: 1,
            points: 0,
            next_level_xp: 10,
            talents_taken: 0,
        },
        Allegiance::Defender,
        Transform::from_xyz(8.0, 0.0, 8.0),
    ));

    let mut jumps = MessageCursor::<CameraJump>::default();
    press(&mut app, KeyCode::F1);
    app.update();
    release_keys(&mut app);
    app.update();
    assert!(
        drain_jumps(&mut app, &mut jumps).is_empty(),
        "enemy heroes and half-heroes never qualify"
    );
}

// -- ③ MinimapPing round-trip ------------------------------------------------------

/// Written pings survive the message stream in order with pos AND team
/// intact (the base guarantees only these semantics; rendering is ui-side).
#[test]
fn minimap_ping_roundtrip_keeps_pos_and_team() {
    let mut app = test_app();
    let mut cursor = MessageCursor::<MinimapPing>::default();
    app.world_mut()
        .write_message(MinimapPing { pos: Vec2::new(1.0, 2.0), team: TeamId(0) });
    app.world_mut()
        .write_message(MinimapPing { pos: Vec2::new(3.5, -4.0), team: TeamId(2) });
    app.update();
    let seen: Vec<MinimapPing> = {
        let msgs = app.world().resource::<Messages<MinimapPing>>();
        cursor.read(msgs).copied().collect()
    };
    assert_eq!(seen.len(), 2, "both pings delivered");
    assert_eq!(seen[0].pos, Vec2::new(1.0, 2.0));
    assert_eq!(seen[0].team, TeamId(0), "team field round-trips");
    assert_eq!(seen[1].pos, Vec2::new(3.5, -4.0));
    assert_eq!(seen[1].team, TeamId(2));
}

// -- ④ defaults idle + pure math ---------------------------------------------------

/// Four-direction edge-scroll velocity: window-pixel space with +y DOWN
/// (cursor coordinates); corners combine both axes.
#[test]
fn edge_scroll_delta_four_directions() {
    let (margin, speed) = (10.0f32, 100.0f32);
    let window = Vec2::new(1000.0, 800.0);
    assert_eq!(
        edge_scroll_delta(margin, speed, Some(Vec2::new(5.0, 400.0)), window),
        Vec2::new(-100.0, 0.0),
        "left edge scrolls left"
    );
    assert_eq!(
        edge_scroll_delta(margin, speed, Some(Vec2::new(995.0, 400.0)), window),
        Vec2::new(100.0, 0.0),
        "right edge scrolls right"
    );
    assert_eq!(
        edge_scroll_delta(margin, speed, Some(Vec2::new(500.0, 5.0)), window),
        Vec2::new(0.0, -100.0),
        "top edge scrolls up (negative window-y)"
    );
    assert_eq!(
        edge_scroll_delta(margin, speed, Some(Vec2::new(500.0, 795.0)), window),
        Vec2::new(0.0, 100.0),
        "bottom edge scrolls down"
    );
    assert_eq!(
        edge_scroll_delta(margin, speed, Some(Vec2::new(5.0, 795.0)), window),
        Vec2::new(-100.0, 100.0),
        "corner combines both axes"
    );
}

/// Disabled/idle cases: absent cursor, dead-center cursor, speed 0, and a
/// zero margin (nothing is ever "on the edge") all yield zero velocity.
#[test]
fn edge_scroll_delta_disabled_cases() {
    let window = Vec2::new(1000.0, 800.0);
    assert_eq!(edge_scroll_delta(10.0, 100.0, None, window), Vec2::ZERO);
    assert_eq!(
        edge_scroll_delta(10.0, 100.0, Some(Vec2::new(500.0, 400.0)), window),
        Vec2::ZERO,
        "center cursor: no scroll"
    );
    assert_eq!(
        edge_scroll_delta(10.0, 0.0, Some(Vec2::new(5.0, 5.0)), window),
        Vec2::ZERO,
        "speed 0 disables"
    );
    assert_eq!(
        edge_scroll_delta(0.0, 100.0, Some(Vec2::new(0.0, 400.0)), window),
        Vec2::ZERO,
        "margin 0 disables (no dead zone)"
    );
}

/// Wheel-zoom math: multiplicative per wheel line (up = zoom IN = scale
/// shrinks), clamped to [zoom_min, zoom_max]; a zero step (the default) is
/// the identity.
#[test]
fn zoomed_scale_multiplicative_and_clamped() {
    let wide = CameraOpts { zoom_step: 1.1, zoom_min: 0.0, zoom_max: f32::INFINITY };
    assert!(
        (zoomed_scale(1.9, 1.0, &wide) - 1.9 / 1.1).abs() < 1e-5,
        "scroll up zooms in"
    );
    assert!(
        (zoomed_scale(1.9, -1.0, &wide) - 1.9 * 1.1).abs() < 1e-5,
        "scroll down zooms out"
    );
    let tight = CameraOpts { zoom_step: 1.1, zoom_min: 1.0, zoom_max: 2.0 };
    assert_eq!(zoomed_scale(1.05, 1.0, &tight), 1.0, "clamped at min");
    assert_eq!(zoomed_scale(1.9, -1.0, &tight), 2.0, "clamped at max");
    assert_eq!(zoomed_scale(1.9, 0.0, &tight), 1.9, "no wheel: unchanged");
    // Identity default: zoom_step 0 never scales (system stays idle).
    let off = CameraOpts::default();
    assert_eq!(zoomed_scale(1.9, 5.0, &off), 1.9, "default opts disable zoom");
}

/// Resource defaults pin the zero-drift contract: an app that never
/// configures the camera domain gets disabled everything, and the
/// registered systems idle safely headless (no window, no camera).
#[test]
fn camera_defaults_idle_headless() {
    let mut app = test_app();
    assert_eq!(*app.world().resource::<EdgeScroll>(), EdgeScroll::default());
    assert_eq!(
        *app.world().resource::<CameraOpts>(),
        CameraOpts { zoom_step: 0.0, zoom_min: 0.0, zoom_max: f32::INFINITY }
    );
    assert!(app.world().resource::<KeyTapTracker>().last.is_none());

    let mut jumps = MessageCursor::<CameraJump>::default();
    for _ in 0..10 {
        app.update();
    }
    assert!(
        drain_jumps(&mut app, &mut jumps).is_empty(),
        "idle defaults produce no jumps (and never panic headless)"
    );
}

// -- ui-half system checks (spawned cameras, still headless) ------------------------

/// The ui-half jump consumer: a `CameraJump` recenters the topmost ortho
/// camera on `to` (ground x/z) while KEEPING its height.
#[test]
fn jump_camera_recenters_topmost_ortho() {
    let mut app = test_app();
    let cam = ortho_camera(app.world_mut(), 1.0);

    app.world_mut().write_message(CameraJump { to: Vec2::new(1.0, 2.0) });
    app.update();
    assert_eq!(
        app.world().get::<Transform>(cam).unwrap().translation,
        Vec3::new(1.0, 40.0, 2.0),
        "x/z follow `to`; y (height) is preserved"
    );
}

/// The ui-half zoom consumer: Line-unit wheel steps recompute the ortho
/// scale through the same `zoomed_scale` math; Pixel-unit (trackpad)
/// scrolls are ignored (documented seam).
#[test]
fn wheel_zoom_rescales_ortho_projection() {
    let mut app = test_app();
    let cam = ortho_camera(app.world_mut(), 2.0);
    app.world_mut().insert_resource(CameraOpts {
        zoom_step: 1.1,
        zoom_min: 0.0,
        zoom_max: f32::INFINITY,
    });

    app.world_mut().write_message(wheel_lines(1.0));
    app.world_mut().write_message(wheel_lines(1.0));
    let mut pixel = wheel_lines(100.0);
    pixel.unit = MouseScrollUnit::Pixel;
    app.world_mut().write_message(pixel);
    app.update();
    let expected = 2.0 * 1.1f32.powf(-2.0);
    let scale = match app.world().get::<Projection>(cam).unwrap() {
        Projection::Orthographic(p) => p.scale,
        _ => panic!("camera lost its ortho projection"),
    };
    assert!(
        (scale - expected).abs() < 1e-5,
        "two line steps compound: {scale} vs {expected}"
    );

    // Default opts (step 0): the same wheel does nothing.
    app.world_mut().insert_resource(CameraOpts::default());
    app.world_mut().write_message(wheel_lines(1.0));
    app.update();
    let scale = match app.world().get::<Projection>(cam).unwrap() {
        Projection::Orthographic(p) => p.scale,
        _ => panic!("camera lost its ortho projection"),
    };
    assert_eq!(scale, expected, "disabled zoom_step is a hard no-op");
}
