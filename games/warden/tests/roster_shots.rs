//! WARDEN-ROSTER offscreen evidence: the ART10 string/bolt machine back in
//! REAL play on the roster-v2 ballista cannon (`docs/roster-v2.md` §5/A11).
//!
//! Same lock-proof producer as `tests/ui_shots.rs` (bevy `headless_renderer`
//! pattern — no window, no display server): place ONE cannon through the
//! player placement gateway, start a REAL wave through the director, and
//! capture two visually distinct string states:
//!   01-ballista-drawn   — string latched at rest, bolt nocked (aiming)
//!   02-ballista-redraw  — mid-redraw after a shot (string pulled back)
//! Both captures assert the wired `BallistaString` exists (the REAL v3
//! surgery `string` node from the loaded weapon scene — this is the layer
//! the headless behavior test cannot reach) and the non-black GPU guard.
//!
//! Run explicitly:
//!   cargo test -p warden --test roster_shots -- --ignored --nocapture

use bevy::app::AppExit;
use bevy::camera::RenderTarget;
use bevy::ecs::observer::On;
use bevy::prelude::*;
use bevy::render::view::screenshot::{save_to_disk, Screenshot, ScreenshotCaptured};
use bevy::render::RenderPlugin;
use bevy::ui::IsDefaultUiCamera;
use bevy::window::{ExitCondition, WindowPlugin};
use bevy::winit::WinitPlugin;
use std::time::{Duration, Instant};
use warden::components::{BallistaString, StringPhase, Tower};
use warden::plugins::acquisition::AcquisitionPlugin;
use warden::plugins::economy::EconomyPlugin;
use warden::plugins::enemies::EnemiesPlugin;
use warden::plugins::fx::FxPlugin;
use warden::plugins::game::GamePlugin;
use warden::plugins::map::MapPlugin;
use warden::plugins::meta::MetaPlugin;
use warden::plugins::projectiles::ProjectilesPlugin;
use warden::plugins::skills::SkillsPlugin;
use warden::plugins::towers::TowersPlugin;
use warden::plugins::ui::UiPlugin;
use warden::plugins::waves::WavesPlugin;
use warden::resources::{DebugRun, Economy};
use warden::systems::debug::{free_slots_in_order, place_towers};
use warbase::prelude::WaveDirector;

#[derive(Resource)]
struct OffscreenTarget(Handle<Image>);

#[derive(Resource, Default)]
struct CapturedMax(u8);

/// The two evidence shots (roster-v2.md §8: no new model demands — the
/// composition is the already-locked tower_arrow v1 + weapon_ballista v3).
#[test]
#[ignore = "GPU offscreen render (evidence capture only) — run with --ignored"]
fn offscreen_ballista_cannon_evidence() {
    let mut app = headless_warden();
    app.finish();
    app.cleanup();
    run_frames(&mut app, Duration::from_millis(1500)); // startup settles

    // Stage: debug mark (no meta payout), a budget, ONE cannon on the first
    // free slot (nearest the path entry — in the fight from frame one).
    app.world_mut().resource_mut::<DebugRun>().marked = true;
    app.world_mut().resource_mut::<Economy>().gold = 500;
    let slots: Vec<(Entity, Vec3)> =
        free_slots_in_order(app.world_mut()).into_iter().take(1).collect();
    assert!(!slots.is_empty(), "the map must offer a free build slot");
    place_towers(app.world_mut(), 3, &slots); // 3 = the ballista cannon def
    let tower = {
        let mut q = app.world_mut().query_filtered::<Entity, With<Tower>>();
        q.iter(app.world()).next().expect("staging placed the cannon")
    };

    // The real weapon scene must load and the string machine must wire —
    // THIS is the assertion the headless behavior test cannot make.
    let mount = {
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            let wired = {
                let mut q = app
                    .world_mut()
                    .query_filtered::<&BallistaString, ()>();
                q.iter(app.world()).count()
            };
            if wired > 0 {
                break;
            }
            if Instant::now() > deadline {
                panic!("the real ballista scene never wired its string node");
            }
            app.update();
        }
        let mut q = app
            .world_mut()
            .query_filtered::<Entity, With<BallistaString>>();
        q.single(app.world()).expect("exactly one string machine")
    };

    // Real combat through the REAL start channel: a StartWave message (the
    // player's Space press) — `WaveDirector::force_begin` is a debug seam
    // with an empty ledger (instant clear), not a real begin.
    app.world_mut()
        .resource_mut::<bevy::ecs::message::Messages<warbase::prelude::StartWave>>()
        .write(warbase::prelude::StartWave);

    // Frame the ballista CLOSE: the string states are the evidence, and
    // the game camera's wide shot renders them a few pixels tall. Park the
    // camera on a three-quarter close orbit of the tower (presentation
    // only — the world keeps simulating underneath).
    {
        let mut cams = app
            .world_mut()
            .query_filtered::<&mut Transform, With<Camera3d>>();
        let tower_pos = app.world().get::<Transform>(tower).unwrap().translation;
        for mut tf in cams.iter_mut(app.world_mut()) {
            *tf = Transform::from_xyz(tower_pos.x + 5.5, 4.2, tower_pos.z + 6.5)
                .looking_at(tower_pos + Vec3::Y * 1.6, Vec3::Y);
        }
    }

    let target = app.world().resource::<OffscreenTarget>().0.clone();
    let dir = format!("{}/docs/img/roster-v2", env!("CARGO_MANIFEST_DIR"));
    std::fs::create_dir_all(&dir).unwrap();
    let shot = |app: &mut App, stem: &str| {
        let path = std::path::Path::new(&dir).join(format!("{stem}.png"));
        *app.world_mut().resource_mut::<CapturedMax>() = CapturedMax(0);
        app.world_mut()
            .spawn(Screenshot::image(target.clone()))
            .observe(save_to_disk(path.clone()))
            .observe(record_captured_max);
        wait_for_capture(app, &path);
        let max = app.world().resource::<CapturedMax>().0;
        assert!(
            max > 24,
            "{stem}: capture is black (max sample {max}) — offscreen render broke"
        );
        println!("[roster-shots] {stem} ok (max sample {max})");
    };

    // Shot A — Drawn (bolt nocked, aiming): wait for the phase, settle a
    // few frames so the pose is stable, capture.
    wait_phase(&mut app, mount, |p| *p == StringPhase::Drawn);
    run_frames(&mut app, Duration::from_millis(250));
    assert_eq!(
        app.world().get::<BallistaString>(mount).unwrap().phase,
        StringPhase::Drawn,
        "shot A staged on the Drawn latch"
    );
    shot(&mut app, "01-ballista-drawn");

    // Shot B — Drawing (post-shot redraw): a fire already happened by then
    // (the phase only enters Drawing after a snap).
    wait_phase(&mut app, mount, |p| matches!(p, StringPhase::Drawing(_)));
    run_frames(&mut app, Duration::from_millis(250));
    assert!(
        matches!(
            app.world().get::<BallistaString>(mount).unwrap().phase,
            StringPhase::Drawing(_)
        ),
        "shot B staged mid-redraw"
    );
    shot(&mut app, "02-ballista-redraw");

    println!(
        "[roster-shots] evidence: {}/01-ballista-drawn.png + 02-ballista-redraw.png",
        dir
    );
    let _ = tower;
    let _ = app.world_mut().write_message(AppExit::Success);
}

/// Advance until the mount's string phase satisfies `want` (deadline 30s —
/// scene loads and wave pacing are wall-clock).
fn wait_phase(app: &mut App, mount: Entity, want: impl Fn(&StringPhase) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(30);
    while Instant::now() < deadline {
        app.update();
        if let Some(w) = app.world().get::<BallistaString>(mount) {
            if want(&w.phase) {
                return;
            }
        }
    }
    panic!("the string phase was never reached before the deadline");
}

fn headless_warden() -> App {
    let mut app = App::new();
    app.insert_resource(ClearColor(Color::srgb(0.08, 0.09, 0.12)))
        .insert_resource(GlobalAmbientLight {
            color: Color::WHITE,
            brightness: 800.0,
            ..default()
        })
        .add_plugins(
            DefaultPlugins
                .set(WindowPlugin {
                    primary_window: None,
                    exit_condition: ExitCondition::DontExit,
                    ..default()
                })
                .set(RenderPlugin {
                    synchronous_pipeline_compilation: true,
                    ..default()
                })
                .set(bevy::log::LogPlugin {
                    filter: "wgpu=error,naga=warn,icu_provider=off".into(),
                    ..default()
                })
                .disable::<WinitPlugin>(),
        )
        .init_state::<warden::states::GameState>()
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
        .init_resource::<CapturedMax>()
        .add_systems(
            Startup,
            retarget_offscreen.after(warden::systems::camera::spawn_camera),
        );
    app
}

fn retarget_offscreen(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    cameras: Query<Entity, With<Camera3d>>,
) {
    use bevy::render::render_resource::{TextureFormat, TextureUsages};
    let mut image = Image::new_target_texture(1280, 720, TextureFormat::Rgba8UnormSrgb, None);
    image.texture_descriptor.usage |= TextureUsages::COPY_SRC;
    let handle = images.add(image);
    for cam in cameras.iter() {
        commands
            .entity(cam)
            .insert((RenderTarget::Image(handle.clone().into()), IsDefaultUiCamera));
    }
    commands.insert_resource(OffscreenTarget(handle));
}

fn record_captured_max(trigger: On<ScreenshotCaptured>, mut seen: ResMut<CapturedMax>) {
    let Some(data) = &trigger.image.data else { return };
    for b in data.iter().step_by(16) {
        if *b > seen.0 {
            seen.0 = *b;
        }
    }
}

fn run_frames(app: &mut App, dur: Duration) {
    let t0 = Instant::now();
    while t0.elapsed() < dur {
        app.update();
    }
}

fn wait_for_capture(app: &mut App, path: &std::path::Path) {
    let t0 = Instant::now();
    while t0.elapsed() < Duration::from_secs(10) {
        app.update();
        if let Ok(meta) = std::fs::metadata(path) {
            if meta.len() > 0 && app.world().resource::<CapturedMax>().0 > 0 {
                return;
            }
        }
    }
    panic!("screenshot never landed: {}", path.display());
}
