//! MMP domain acceptance (round-3 plan §Phase-1 MMP row): minimap ping
//! rendering + click routing + tooltip CJK seam, over the REAL rts-lab thin
//! layer (GamePlugin loads maps/arena.ron, so `MinimapModel` carries live
//! vision bounds — the coordinate assertions read them, no hardcoding).
//!
//! Numbering matches the task's acceptance list:
//! ① MinimapPing -> team-colored ripple entity appears, expires ~3s;
//! ② Alt+click = exactly one MinimapPing, no camera jump; plain click
//!    jumps the camera, no ping;
//! ③ 2 selected units + right-click -> exactly one UnitCommand::Move with
//!    both subjects and the converted panel->world `to`; empty selection
//!    right-click only jumps the camera;
//! ④ warbase `ui::UiFont` seam: Some(handle) -> the tooltip's font handle
//!    is the injected one; None -> the pre-change default source (zero
//!    drift), plus the pure ripple/color math.

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::input::ButtonInput;
use bevy::prelude::*;
use bevy::text::FontSource;
use bevy::window::PrimaryWindow;
use rts_lab::systems::{camera::TopDownCamera, minimap};
use warbase::prelude::*;
use warbase::testing::test_app;

// -- fixtures -------------------------------------------------------------------

/// Headless lab with the minimap systems under test (the VisualPlugin set,
/// wired headless: panel markers exist without a renderer; the exclusive
/// click router works on injected Window/ButtonInput state).
fn mmp_app() -> App {
    let mut app = test_app();
    app.add_plugins(rts_lab::plugins::game::GamePlugin);
    app.add_systems(
        Update,
        (
            minimap::refresh_minimap_view,
            (minimap::spawn_minimap_pings, minimap::animate_minimap_pings).chain(),
            minimap::minimap_clicks,
        ),
    );
    app.update(); // Startup settles: map + vision -> live MinimapModel bounds
    app
}

/// Panel fixture: 180x180 logical px centered at `center` (the same size the
/// real layout computes: 176 panel + 2x2 border).
fn stage_panel(app: &mut App, center: Vec2) {
    app.world_mut().spawn((
        minimap::MinimapPanel,
        Node::default(),
        GlobalTransform::from_translation(center.extend(0.0)),
        ComputedNode {
            size: Vec2::new(180.0, 180.0),
            inverse_scale_factor: 1.0,
            ..default()
        },
    ));
}

/// Camera fixture at an unmistakable position.
fn stage_camera(app: &mut App) {
    app.world_mut().spawn((
        TopDownCamera,
        Transform::from_xyz(999.0, 50.0, 999.0),
    ));
}

/// Inject a fresh primary window whose cursor sits at `cursor` (MinimalPlugins
/// has no window plugin; re-inserting per frame simulates the OS cursor
/// update and keeps each frame's cursor state explicit).
fn stage_window(app: &mut App, cursor: Vec2) {
    let mut old = app
        .world_mut()
        .query_filtered::<Entity, With<PrimaryWindow>>();
    if let Ok(entity) = old.single(app.world_mut()) {
        app.world_mut().entity_mut(entity).despawn();
    }
    let window = app.world_mut().spawn((PrimaryWindow, Window::default())).id();
    app.world_mut()
        .get_mut::<Window>(window)
        .unwrap()
        .set_cursor_position(Some(cursor));
}

/// One freshly pressed mouse button this frame (headless has no input plugin
/// to age the state — every helper re-inserts fresh resources per frame).
fn click(app: &mut App, button: MouseButton) {
    let mut buttons = ButtonInput::<MouseButton>::default();
    buttons.press(button);
    app.world_mut().insert_resource(buttons);
}

/// Advance `frames` updates with clean input state (no held/pressed buttons —
/// injected just_pressed states would otherwise re-fire every frame).
fn settle(app: &mut App, frames: usize) {
    for _ in 0..frames {
        app.world_mut().insert_resource(ButtonInput::<MouseButton>::default());
        app.world_mut().insert_resource(ButtonInput::<KeyCode>::default());
        app.update();
    }
}

/// Messages of `M` written across the click frame plus `frames` quiet ones,
/// read through one cursor (double-buffer drain discipline).
fn click_and_count<M: Message>(app: &mut App, button: MouseButton, frames: usize) -> usize {
    click(app, button);
    let mut cursor = MessageCursor::<M>::default();
    app.update(); // the click frame
    let mut total = cursor.read(app.world().resource::<Messages<M>>()).count();
    for _ in 0..frames {
        app.world_mut().insert_resource(ButtonInput::<MouseButton>::default());
        app.world_mut().insert_resource(ButtonInput::<KeyCode>::default());
        app.update();
        total += cursor.read(app.world().resource::<Messages<M>>()).count();
    }
    total
}

fn ripple_count(app: &mut App) -> usize {
    let mut q = app
        .world_mut()
        .query_filtered::<Entity, With<minimap::MinimapPingRipple>>();
    q.iter(app.world()).count()
}

fn camera_xz(app: &mut App) -> Vec2 {
    let mut q = app
        .world_mut()
        .query_filtered::<&Transform, With<TopDownCamera>>();
    let tf = q.single(app.world()).unwrap();
    Vec2::new(tf.translation.x, tf.translation.z)
}

/// The world point a panel click at `cursor` maps to (the fixture's rect is
/// known: center +/- 90px), through the LIVE MinimapModel bounds.
fn expected_world_point(app: &App, center: Vec2, cursor: Vec2) -> Vec2 {
    let bounds = app.world().resource::<MinimapModel>().map_bounds;
    assert!(!bounds.is_empty(), "fixture map must carry vision bounds");
    let rect_min = center - Vec2::splat(90.0);
    Vec2::new(
        bounds.min.x + (cursor.x - rect_min.x) / 180.0 * bounds.size().x,
        bounds.min.y + (cursor.y - rect_min.y) / 180.0 * bounds.size().y,
    )
}

// -- ① ping -> ripple --------------------------------------------------------

/// ① A MinimapPing spawns one team-colored ripple (enemy red / own gold from
/// the live UiTheme) that animates and despawns after the 3s lifetime
/// (two 1.5s waves), never before.
#[test]
fn minimap_ping_spawns_team_colored_ripple_and_expires() {
    let mut app = mmp_app();
    stage_panel(&mut app, Vec2::new(640.0, 360.0));
    let theme = app.world().resource::<warbase::ui::UiTheme>().clone();
    let bounds = app.world().resource::<MinimapModel>().map_bounds;

    // Both pings land in ONE frame, so both are exactly at age 0 when the
    // landing-frame assertions read them (full team color, seed ring size).
    app.world_mut().write_message(MinimapPing {
        pos: bounds.center(),
        team: TeamId(1), // enemy
    });
    app.world_mut().write_message(MinimapPing {
        pos: bounds.min + bounds.size() * 0.25,
        team: TeamId(0), // own
    });
    app.update();

    let mut enemy: Option<Color> = None;
    let mut own: Option<Color> = None;
    let mut q = app
        .world_mut()
        .query::<(&minimap::MinimapPingRipple, &BorderColor, &Node)>();
    for (ripple, border, node) in q.iter(app.world()) {
        assert_eq!(node.width, Val::Px(10.0), "ring starts at the seed size");
        assert_eq!(border.top, border.left, "uniform ring color");
        if ripple.team == TeamId(1) {
            enemy = Some(border.top);
        } else {
            own = Some(border.top);
        }
    }
    drop(q);
    assert_eq!(
        enemy.expect("enemy ripple exists"),
        theme.state_colors.enemy.color(),
        "enemy ping renders in the theme's enemy red"
    );
    assert_eq!(
        own.expect("own ripple exists"),
        theme.state_colors.gold.color(),
        "own ping renders in the theme's gold"
    );
    assert_eq!(ripple_count(&mut app), 2);

    // ~0.5s in: the animation advanced (ring grew, alpha faded below 1).
    settle(&mut app, 30);
    let mut q = app
        .world_mut()
        .query_filtered::<(&Node, &BorderColor), With<minimap::MinimapPingRipple>>();
    for (node, border) in q.iter(app.world()) {
        let Val::Px(w) = node.width else { panic!("px width") };
        assert!(w > 10.0 && w < 56.0, "ring growing, got {w}");
        let Color::Srgba(c) = border.top else { panic!("srgba border") };
        assert!(c.alpha < 1.0, "ring fading, got {}", c.alpha);
    }
    drop(q);
    assert_eq!(ripple_count(&mut app), 2, "still alive mid-life");

    // Past 3s virtual (spawn + ~3.03s): gone.
    settle(&mut app, 152);
    assert_eq!(ripple_count(&mut app), 0, "ripples despawn after 3s");
}

// -- ② Alt+click vs plain click ----------------------------------------------

/// ② Alt+left-click inside the panel writes EXACTLY one MinimapPing (own
/// team) and never moves the camera; a plain left-click jumps the camera and
/// writes no ping; an off-panel click does nothing.
#[test]
fn alt_click_pings_once_plain_click_jumps_camera() {
    let mut app = mmp_app();
    let center = Vec2::new(640.0, 360.0);
    stage_panel(&mut app, center);
    stage_camera(&mut app);

    // -- Alt + left click: one ping, camera untouched.
    let cursor = Vec2::new(600.0, 400.0);
    stage_window(&mut app, cursor);
    let mut keys = ButtonInput::<KeyCode>::default();
    keys.press(KeyCode::AltLeft);
    app.world_mut().insert_resource(keys);
    assert_eq!(
        click_and_count::<MinimapPing>(&mut app, MouseButton::Left, 3),
        1,
        "Alt+click writes exactly one MinimapPing"
    );
    assert_eq!(camera_xz(&mut app), Vec2::new(999.0, 999.0), "no camera jump on ping");
    // The ping landed as one ripple (the ① seam), at the local team.
    let mut q = app
        .world_mut()
        .query_filtered::<&minimap::MinimapPingRipple, ()>();
    let ripple = q.single(app.world()).unwrap();
    assert_eq!(ripple.team, minimap::LOCAL_TEAM, "ping attributed to the local team");
    drop(q);
    assert_eq!(ripple_count(&mut app), 1, "exactly one ripple, no duplicates");

    // -- Plain left click elsewhere in the panel: camera jump, no ping.
    let cursor2 = Vec2::new(700.0, 330.0);
    stage_window(&mut app, cursor2);
    assert_eq!(
        click_and_count::<MinimapPing>(&mut app, MouseButton::Left, 2),
        0,
        "plain click writes no ping"
    );
    let expected = expected_world_point(&app, center, cursor2);
    assert_eq!(camera_xz(&mut app), expected, "plain click jumps the camera");

    // -- Click outside the panel: nothing at all.
    stage_window(&mut app, Vec2::new(10.0, 10.0));
    assert_eq!(
        click_and_count::<MinimapPing>(&mut app, MouseButton::Left, 1),
        0,
        "off-panel click writes no ping"
    );
    assert_eq!(camera_xz(&mut app), expected, "off-panel click changes nothing");
}

// -- ③ right-click command landing ---------------------------------------------

/// ③ With two units selected, a right-click inside the panel issues EXACTLY
/// one UnitCommand::Move whose subjects are the selection and whose `to` is
/// the panel->world conversion of the click; with an empty selection a
/// right-click only jumps the camera.
#[test]
fn right_click_minimap_moves_selection_or_jumps_when_empty() {
    let mut app = mmp_app();
    let center = Vec2::new(640.0, 360.0);
    stage_panel(&mut app, center);
    stage_camera(&mut app);

    // Two selected units (subjects only — the router clones the selection).
    let a = app.world_mut().spawn_empty().id();
    let b = app.world_mut().spawn_empty().id();
    app.world_mut().resource_mut::<SelectionState>().entities = vec![a, b];

    let cursor = Vec2::new(610.0, 380.0);
    stage_window(&mut app, cursor);
    click(&mut app, MouseButton::Right);
    let mut messages = MessageCursor::<UnitCommand>::default();
    app.update(); // the click frame
    let commands: Vec<UnitCommand> = messages
        .read(app.world().resource::<Messages<UnitCommand>>())
        .cloned()
        .collect();
    assert_eq!(commands.len(), 1, "exactly one UnitCommand per right-click");
    let expected = expected_world_point(&app, center, cursor);
    assert_eq!(
        commands[0].order,
        Order::Move { to: Vec3::new(expected.x, 0.0, expected.y) },
        "move order lands on the converted world point (y=0 plane)"
    );
    assert_eq!(commands[0].subjects, vec![a, b], "whole selection is the subject");
    assert_eq!(camera_xz(&mut app), Vec2::new(999.0, 999.0), "order click never jumps");
    // One quiet frame first: the right-click's command is still inside the
    // two-frame read window here, and a fresh MessageCursor replays from
    // the buffer head — without this flush, the stale command would count
    // again below (bevy keeps messages readable for two frames).
    settle(&mut app, 1);
    assert_eq!(
        click_and_count::<UnitCommand>(&mut app, MouseButton::Left, 2),
        0,
        "quiet frames issue nothing further"
    );

    // Empty selection: the same right-click degrades to a camera jump.
    app.world_mut().resource_mut::<SelectionState>().entities.clear();
    let cursor2 = Vec2::new(730.0, 290.0);
    stage_window(&mut app, cursor2);
    assert_eq!(
        click_and_count::<UnitCommand>(&mut app, MouseButton::Right, 2),
        0,
        "empty selection issues no order"
    );
    let expected2 = expected_world_point(&app, center, cursor2);
    assert_eq!(camera_xz(&mut app), expected2, "empty-selection right-click jumps");
}

// -- ④ UiFont seam -------------------------------------------------------------

/// ④ The base tooltip reads the `warbase::ui::UiFont` seam: Some(handle) ->
/// the spawned tooltip's font source IS that handle; None (the default) ->
/// exactly the pre-seam default source (zero drift).
#[test]
fn tooltip_font_follows_uifont_resource_and_none_is_zero_drift() {
    fn hovered_slot(app: &mut App, tooltip: &str) {
        app.world_mut().spawn((
            Interaction::Hovered,
            warbase::ui::CommandSlotButton {
                action: warbase::ui::CommandAction::Sell,
                enabled: true,
                tooltip: tooltip.into(),
            },
        ));
    }
    fn tooltip_font(app: &mut App) -> FontSource {
        let mut q = app
            .world_mut()
            .query_filtered::<&TextFont, With<warbase::ui::TooltipText>>();
        q.single(app.world_mut()).expect("tooltip spawned").font.clone()
    }

    // Injected handle wins.
    let mut app = test_app(); // WarbasePlugin wires UiPlugin (default ui feature)
    let mut fonts = Assets::<Font>::default();
    let handle = fonts.add(Font::from_bytes(vec![1, 2, 3, 4])); // never rendered
    app.insert_resource(fonts);
    app.insert_resource(warbase::ui::UiFont(Some(handle.clone())));
    hovered_slot(&mut app, "传送 (Teleport)");
    app.update();
    app.update();
    assert_eq!(
        tooltip_font(&mut app),
        FontSource::Handle(handle.clone()),
        "Some(handle): the tooltip renders on the injected CJK face"
    );

    // None: exactly the pre-change default (zero drift).
    app.insert_resource(warbase::ui::UiFont(None));
    let mut q = app
        .world_mut()
        .query_filtered::<&mut warbase::ui::CommandSlotButton, ()>();
    let mut slot = q.single_mut(app.world_mut()).unwrap();
    slot.tooltip = "卖出 (Sell)".into(); // force a tooltip respawn
    drop(slot);
    app.update();
    app.update();
    assert_eq!(
        tooltip_font(&mut app),
        FontSource::default(),
        "None: the tooltip keeps bevy's default font source"
    );
    assert_eq!(
        tooltip_font(&mut app),
        TextFont::default().font,
        "None is digit-for-digit the pre-seam TextFont default"
    );
}

// -- pure math -------------------------------------------------------------------

/// The ripple timeline: seed ring + full alpha at wave starts, linear growth
/// and fade inside a wave, the second wave resetting both, lifetime bounds.
#[test]
fn ripple_at_two_waves_then_none() {
    assert_eq!(minimap::ripple_at(-0.1), None);
    assert_eq!(minimap::ripple_at(0.0), Some((10.0, 1.0)));
    // Mid first wave: p = 0.5 -> size 33, alpha 0.5.
    assert_eq!(minimap::ripple_at(0.75), Some((33.0, 0.5)));
    // Wave 2 restarts the sweep at 1.5s.
    assert_eq!(minimap::ripple_at(1.5), Some((10.0, 1.0)));
    assert_eq!(minimap::ripple_at(2.25), Some((33.0, 0.5)));
    assert_eq!(minimap::ripple_at(minimap::PING_LIFETIME_SECS), None);
    assert_eq!(minimap::PING_LIFETIME_SECS, 3.0);
}

/// Ping team colors come from the theme tokens (own gold / enemy red), so a
/// re-skin recolors pings with zero code change.
#[test]
fn ping_color_reads_theme_team_tokens() {
    let theme = warbase::ui::UiTheme::default();
    assert_eq!(
        minimap::ping_color(TeamId(0), &theme),
        theme.state_colors.gold.color(),
        "own team pings gold"
    );
    assert_eq!(
        minimap::ping_color(TeamId(1), &theme),
        theme.state_colors.enemy.color(),
        "enemy pings red"
    );
    // Default tokens are the brief's gold/enemy values.
    assert_eq!(minimap::ping_color(TeamId(0), &theme), Color::srgb(1.0, 0.84, 0.0));
    assert_eq!(minimap::ping_color(TeamId(1), &theme), Color::srgb(0.85, 0.2, 0.2));
}
