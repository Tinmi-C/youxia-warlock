//! SEL5 upcast regression (WC3 alignment, ADR-0016): warden's pick layer
//! now rides the warbase seam (`ActiveWorldPicker` + `PickAttempt` /
//! `PickResolved`). These tests pin the two acceptance clauses:
//! 1. the pick bridge emits the BASE message shape from raw pointer input
//!    (headless app + emulated window/camera, the same harness trick
//!    behavior.rs uses);
//! 2. the world-space hit math resolves the SAME slot/tower as the legacy
//!    screen-space math (pre-SEL5 code, kept verbatim below as the
//!    reference) for the same set of world coordinates.

use std::time::Duration;

use bevy::{prelude::*, state::app::StatesPlugin, time::TimeUpdateStrategy};

use warden::{
    components::{Tower, TowerKind, TowerSlot},
    plugins::{
        acquisition::AcquisitionPlugin, economy::EconomyPlugin, enemies::EnemiesPlugin,
        fx::FxPlugin, game::GamePlugin, map::MapPlugin, meta::MetaPlugin,
        projectiles::ProjectilesPlugin, skills::SkillsPlugin, towers::TowersPlugin, ui::UiPlugin,
        waves::WavesPlugin,
    },
    resources::{MetaSavePath, UiPrefsPath},
    states::GameState,
};
use warbase::prelude::{PickAttempt, PickButton, PickResolved};

/// Headless app: all gameplay plugins (no renderer/window) + fixed timestep.
/// Same composition as behavior.rs's `test_app` (copied so this file stays
/// an independent regression binary).
fn test_app() -> App {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, StatesPlugin))
        .init_state::<GameState>()
        .add_plugins((
            FxPlugin,
            GamePlugin,
            MapPlugin,
            TowersPlugin,
            EnemiesPlugin,
            WavesPlugin,
            EconomyPlugin,
            AcquisitionPlugin,
            MetaPlugin,
            ProjectilesPlugin,
            SkillsPlugin,
            UiPlugin,
        ))
        .insert_resource(MetaSavePath(
            std::env::temp_dir().join("warden_meta_pick_upcast_test.txt"),
        ))
        .insert_resource(UiPrefsPath(
            std::env::temp_dir().join("warden_ui_pick_upcast_test.txt"),
        ))
        .init_resource::<ButtonInput<KeyCode>>()
        .init_resource::<ButtonInput<bevy::input::mouse::MouseButton>>()
        .init_resource::<Assets<Mesh>>()
        .init_resource::<Assets<StandardMaterial>>()
        .init_resource::<Assets<bevy::gltf::Gltf>>()
        .init_resource::<Assets<AnimationGraph>>()
        .init_resource::<Assets<AnimationClip>>()
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(
            1.0 / 60.0,
        )));
    app
}

/// Give the headless app a live cursor path (window + computed camera), the
/// same fixture behavior.rs `enable_world_picking` provides.
fn enable_world_picking(app: &mut App, cursor: Vec2) {
    let w = app.world_mut().spawn(Window::default()).id();
    app.world_mut()
        .get_mut::<Window>(w)
        .expect("window")
        .set_cursor_position(Some(cursor));
    let size = UVec2::new(1280, 720);
    let mut cams = app
        .world_mut()
        .query_filtered::<(&mut Camera, &Projection, &Transform, &mut GlobalTransform), ()>();
    for (mut cam, proj, tf, mut gt) in cams.iter_mut(app.world_mut()) {
        cam.viewport = Some(Viewport {
            physical_size: size,
            ..default()
        });
        cam.computed.target_info = Some(RenderTargetInfo {
            physical_size: size,
            scale_factor: 1.0,
        });
        let mut p = proj.clone();
        p.update(1280.0, 720.0);
        cam.computed.clip_from_view = p.get_clip_from_view();
        *gt = GlobalTransform::from(*tf);
    }
}

use bevy::camera::{RenderTargetInfo, Viewport};

fn set_cursor(app: &mut App, pos: Vec2) {
    let mut q = app.world_mut().query_filtered::<&mut Window, ()>();
    for mut w in q.iter_mut(app.world_mut()) {
        w.set_cursor_position(Some(pos));
    }
}

fn click_left(app: &mut App) {
    app.world_mut()
        .resource_mut::<ButtonInput<bevy::input::mouse::MouseButton>>()
        .press(bevy::input::mouse::MouseButton::Left);
}

fn release_all_inputs(app: &mut App) {
    // NOTE: bevy 0.19 `clear()` only drops just_pressed/just_released —
    // `pressed` stays set, so a later `press()` would no-op. These tests
    // re-click, so fully reset.
    app.world_mut()
        .resource_mut::<ButtonInput<bevy::input::mouse::MouseButton>>()
        .reset_all();
}

/// Project a world point to screen (the inverse of the pick seam).
fn project(app: &mut App, point: Vec3) -> Vec2 {
    let world = app.world_mut();
    let mut cams = world.query::<(&Camera, &GlobalTransform)>();
    let (cam, tf) = cams.single(world).expect("one camera");
    cam.world_to_viewport(tf, point).expect("point projects")
}

/// Drain this frame's `PickAttempt`s (fresh cursor over the message store).
fn pick_attempts(app: &App) -> Vec<PickAttempt> {
    let msgs = app
        .world()
        .resource::<bevy::ecs::message::Messages<PickAttempt>>();
    let mut cursor = bevy::ecs::message::MessageCursor::default();
    cursor.read(msgs).cloned().collect()
}

/// Drain this frame's `PickResolved`s.
fn pick_resolved(app: &App) -> Vec<PickResolved> {
    let msgs = app
        .world()
        .resource::<bevy::ecs::message::Messages<PickResolved>>();
    let mut cursor = bevy::ecs::message::MessageCursor::default();
    cursor.read(msgs).cloned().collect()
}

// --- legacy reference math (pre-SEL5, kept VERBATIM from the old
// pointer.rs / command.rs — the comparison baseline) -------------------------

/// Legacy slot hit (command.rs `select_object` step 2): the nearest slot by
/// SCREEN distance within the 45 px slop.
fn legacy_slot_hit(app: &mut App, cursor: Vec2) -> Option<Entity> {
    let (cam, tf) = {
        let mut cams = app.world_mut().query::<(&Camera, &GlobalTransform)>();
        let (c, t) = cams.single(app.world()).expect("one camera");
        (c.clone(), *t)
    };
    let mut slots = app
        .world_mut()
        .query_filtered::<(Entity, &Transform), With<TowerSlot>>();
    let mut best: Option<(Entity, f32)> = None;
    for (eid, stf) in slots.iter(app.world()) {
        if let Ok(sp) = cam.world_to_viewport(&tf, stf.translation) {
            let d = sp.distance(cursor);
            if d <= 45.0 && best.map_or(true, |(_, bd)| d < bd) {
                best = Some((eid, d));
            }
        }
    }
    best.map(|(e, _)| e)
}

/// Legacy tower hit (command.rs `select_object` step 1): the nearest tower
/// by SCREEN distance (projected 3D centers) within the 45 px slop.
fn legacy_tower_hit(app: &mut App, cursor: Vec2) -> Option<Entity> {
    let (cam, tf) = {
        let mut cams = app.world_mut().query::<(&Camera, &GlobalTransform)>();
        let (c, t) = cams.single(app.world()).expect("one camera");
        (c.clone(), *t)
    };
    let mut towers = app
        .world_mut()
        .query_filtered::<(Entity, &Transform), With<Tower>>();
    let mut best: Option<(Entity, f32)> = None;
    for (eid, ttf) in towers.iter(app.world()) {
        if let Ok(sp) = cam.world_to_viewport(&tf, ttf.translation) {
            let d = sp.distance(cursor);
            if d <= 45.0 && best.map_or(true, |(_, bd)| d < bd) {
                best = Some((eid, d));
            }
        }
    }
    best.map(|(e, _)| e)
}

// --- fixture ------------------------------------------------------------------

/// Place a tower (y = 0.6, like the behavior.rs fixture) on the first free
/// slot, without touching the Selection state.
fn place_tower(app: &mut App, tower_index: usize) -> Entity {
    let slot = {
        let grid = app.world().resource::<warbase::prelude::BuildGrid>().clone();
        let mut q = app.world_mut().query_filtered::<(Entity, &TowerSlot), ()>();
        let mut free = q
            .iter(app.world())
            .filter(|(_, s)| {
                grid.check(warden::systems::map::slot_cell(s.index)).is_ok()
            })
            .map(|(e, _)| e);
        free.next().expect("a free slot")
    };
    let (slot_index, pos) = {
        let s = app.world().get::<TowerSlot>(slot).unwrap();
        (s.index, *app.world().get::<Transform>(slot).unwrap())
    };
    let def = app.world().resource::<warden::resources::TowerDefs>().list[tower_index].clone();
    let e = app
        .world_mut()
        .spawn((
            Tower {
                tower_index,
                damage: def.damage,
                attack_speed: def.attack_speed,
                range: def.range,
                attack_type: def.attack_type,
                cooldown: 0.0,
                target: None,
                kind: TowerKind::Base(tower_index),
                aoe_radius: 0.0,
                slow_factor: 0.0,
                slow_duration: 0.0,
                invested: 50,
            },
            Transform::from_xyz(pos.translation.x, 0.6, pos.translation.z),
        ))
        .id();
    app.world_mut()
        .resource_mut::<warbase::prelude::BuildGrid>()
        .place(warden::systems::map::slot_cell(slot_index), e);
    e
}

// --- tests --------------------------------------------------------------------

/// SEL5 acceptance ②: after a simulated pointer press, the warden app's
/// message store carries the BASE `PickAttempt` — world point on the ground
/// plane under the cursor, left button, single click — and a click over
/// blocking UI emits nothing.
#[test]
fn pick_bridge_emits_base_pick_attempt() {
    let mut app = test_app();
    app.update(); // startup: map, slots, camera, command bar
    enable_world_picking(&mut app, Vec2::new(640.0, 360.0));
    // Cursor exactly over slot 3's ground position (-6.0, 0.0).
    let cursor = project(&mut app, Vec3::new(-6.0, 0.0, 0.0));
    set_cursor(&mut app, cursor);
    click_left(&mut app);
    app.update();
    let attempts = pick_attempts(&app);
    assert_eq!(attempts.len(), 1, "exactly one PickAttempt per click");
    let a = attempts[0];
    assert_eq!(a.button, PickButton::Left);
    assert_eq!(a.click_count, 1);
    assert!(!a.ctrl && !a.shift);
    assert!(
        (a.world.x - (-6.0)).abs() < 1e-3 && (a.world.y - 0.0).abs() < 1e-3,
        "world point is the cursor ray on the y=0 ground plane, got {:?}",
        a.world
    );

    // Over blocking UI (the bottom bar reports the cursor): no further
    // attempts, even though headless just_pressed sticks (release first).
    release_all_inputs(&mut app);
    app.update();
    {
        let mut q = app
            .world_mut()
            .query_filtered::<Entity, With<warden::systems::command::CommandBarRoot>>();
        let bar = q.iter(app.world()).next().expect("command bar");
        app.world_mut().entity_mut(bar).insert(
            bevy::ui::RelativeCursorPosition {
                cursor_over: true,
                normalized: Some(Vec2::ZERO),
            },
        );
    }
    click_left(&mut app);
    app.update();
    assert!(
        pick_attempts(&app).is_empty(),
        "a click over blocking UI must not emit a world PickAttempt"
    );
}

/// SEL5 acceptance ①: for the same world coordinates, the upcast resolver
/// (world-space disc hits) picks the SAME slot as the legacy screen-space
/// math (projected slots, 45 px slop). Covers every slot center plus
/// near-field offsets — inside both metrics' hit ranges — and one far-field
/// point both must miss.
#[test]
fn slot_hits_match_legacy_screen_math() {
    let mut app = test_app();
    app.update();
    enable_world_picking(&mut app, Vec2::new(640.0, 360.0));
    release_all_inputs(&mut app); // no raw clicks: only injected attempts

    let centers: Vec<Vec2> = {
        let mut q = app.world_mut().query_filtered::<&Transform, With<TowerSlot>>();
        q.iter(app.world())
            .map(|t| Vec2::new(t.translation.x, t.translation.z))
            .collect()
    };
    assert_eq!(centers.len(), 8, "the warden arena has 8 slots");
    // Slot centers plus near-field offsets (well inside both the legacy
    // 45px slop and the 2.0-world disc; slots are >= 4.0 apart, so the
    // nearest is unambiguous in both metrics).
    let mut cases: Vec<Vec2> = Vec::new();
    for c in &centers {
        for off in [
            Vec2::ZERO,
            Vec2::new(1.0, 0.0),
            Vec2::new(-1.0, 0.0),
            Vec2::new(0.0, 1.0),
            Vec2::new(0.0, -1.0),
        ] {
            cases.push(*c + off);
        }
    }
    // One cursor across the loop: Messages live two frames, so per-case
    // fresh cursors would re-count the previous case's resolution.
    let mut resolved_seen = bevy::ecs::message::MessageCursor::<PickResolved>::default();
    for point in cases {
        // Legacy: cursor = projection of the same world point.
        let cursor = project(&mut app, Vec3::new(point.x, 0.0, point.y));
        let legacy = legacy_slot_hit(&mut app, cursor);
        // New: inject the base message at the same world point.
        app.world_mut().write_message(PickAttempt {
            world: point,
            button: PickButton::Left,
            click_count: 1,
            ctrl: false,
            shift: false,
        });
        app.update();
        let resolved: Vec<PickResolved> = {
            let msgs = app
                .world()
                .resource::<bevy::ecs::message::Messages<PickResolved>>();
            resolved_seen.read(msgs).cloned().collect()
        };
        assert_eq!(resolved.len(), 1);
        let new_slot = resolved[0]
            .cell
            .map(|(idx, _)| {
                let mut q = app
                    .world_mut()
                    .query_filtered::<Entity, With<TowerSlot>>();
                q.iter(app.world())
                    .find(|e| app.world().get::<TowerSlot>(*e).unwrap().index as i32 == idx)
                    .expect("cell names an existing slot")
            });
        assert_eq!(
            legacy, new_slot,
            "world point {point:?}: legacy screen math and upcast resolver must agree"
        );
    }

    // Far-field blank: the screen corner farthest from every projected slot
    // misses in both metrics (the blank-cancel contract, §16.3).
    let blank = {
        let (cam, tf) = {
            let mut cams = app.world_mut().query::<(&Camera, &GlobalTransform)>();
            let (c, t) = cams.single(app.world()).expect("one camera");
            (c.clone(), *t)
        };
        let mut pts = Vec::new();
        let mut slots = app.world_mut().query_filtered::<&Transform, With<TowerSlot>>();
        for t in slots.iter(app.world()) {
            if let Ok(sp) = cam.world_to_viewport(&tf, t.translation) {
                pts.push(sp);
            }
        }
        [Vec2::new(10.0, 710.0), Vec2::new(1270.0, 710.0), Vec2::new(10.0, 10.0), Vec2::new(1270.0, 10.0)]
            .into_iter()
            .max_by_key(|c| {
                pts.iter().map(|p| p.distance(*c) as u64).min().unwrap_or(u64::MAX)
            })
            .expect("a corner")
    };
    assert!(legacy_slot_hit(&mut app, blank).is_none(), "fixture is blank for legacy");
    set_cursor(&mut app, blank);
    click_left(&mut app);
    app.update();
    // Same persistent cursor as the loop: a fresh cursor would still see
    // the previous case's message (stores keep two frames).
    let resolved: Vec<PickResolved> = {
        let msgs = app
            .world()
            .resource::<bevy::ecs::message::Messages<PickResolved>>();
        resolved_seen.read(msgs).cloned().collect()
    };
    assert_eq!(resolved.len(), 1);
    assert!(
        resolved[0].cell.is_none(),
        "blank corner misses the upcast resolver too"
    );
}

/// SEL5 acceptance ① (towers): a click dead-centre on a tower's on-screen
/// body resolves the SAME tower in both metrics — the legacy math projects
/// the elevated (y = 0.6) tower centre, the upcast resolver disc-hits the
/// tower's ground (x, z) from the cursor's ground point.
#[test]
fn tower_hits_match_legacy_screen_math() {
    let mut app = test_app();
    app.update();
    enable_world_picking(&mut app, Vec2::new(640.0, 360.0));
    release_all_inputs(&mut app);
    let a = place_tower(&mut app, 0);
    let b = place_tower(&mut app, 0);
    assert_ne!(a, b);

    for tower in [a, b] {
        let center = app.world().get::<Transform>(tower).unwrap().translation;
        let cursor = project(&mut app, center);
        let legacy = legacy_tower_hit(&mut app, cursor);
        assert_eq!(legacy, Some(tower), "legacy hits the clicked tower");
        // New: cursor -> bridge -> resolver (the full chain, end to end).
        set_cursor(&mut app, cursor);
        click_left(&mut app);
        app.update();
        let resolved = pick_resolved(&app);
        assert_eq!(resolved.len(), 1);
        assert_eq!(
            resolved[0].entities.first().copied(),
            Some(tower),
            "upcast resolver hits the same tower"
        );
        // And the selection semantics consumed it (Tower selected).
        assert_eq!(
            app.world().resource::<warden::resources::Selection>(),
            &warden::resources::Selection::Tower(tower),
            "select_object consumed the resolved pick"
        );
        release_all_inputs(&mut app);
        app.update();
    }
}
