//! STAB offscreen skin screenshots (`--ui-shots`' lock-proof producer).
//!
//! The windowed `--ui-shots` CLI captures the primary window's drawable —
//! which macOS never composites while the session is locked, yielding black
//! PNGs (empirically established 2026-10-28; loop-td's `--ui-mock v2`
//! reproduces it too). This test instead boards the SAME frame script on
//! the bevy `headless_renderer` pattern (MMP c59bda9): DefaultPlugins with
//! `primary_window: None` + `WinitPlugin` disabled, the game camera
//! re-targeted onto an offscreen `Image`, `Screenshot::image` readback,
//! manual `finish()/cleanup()` — zero window, zero display server, zero
//! lock dependence, deterministic reruns day or night.
//!
//! Both skins run sequentially (one env-driven app each):
//!   cargo test -p warden --test ui_shots -- --ignored --nocapture
//! producing `docs/img/skin/{default,plan-a}/0{1..6}-*.png` (the same
//! SHOTS table the CLI driver uses).

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
use warden::components::{Enemy, Tower};
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
use warden::resources::{DebugRun, Economy, Selection};
use warden::systems::command::{CommandAction, CommandButton};
use warden::systems::debug::{free_slots_in_order, place_towers, ShotSkin};
use warden::systems::ui_shots::{out_dir, solo_bottom_bar, SoloTarget, SHOTS};
use warbase::prelude::WaveDirector;

/// Offscreen target handle for the later `Screenshot::image` captures.
#[derive(Resource)]
struct OffscreenTarget(Handle<Image>);

/// Brightest captured sample so far (reset before each shot; the non-black
/// guard — a silently-black capture fails the test instead of shipping).
#[derive(Resource, Default)]
struct CapturedMax(u8);

/// The lock-proof STAB producer: both skins, six shots each, non-black
/// asserted on the GPU readback. GPU + a few seconds: `#[ignore]`d, run
/// explicitly with `--ignored`.
#[test]
#[ignore = "GPU offscreen render (evidence capture only) — run with --ignored"]
fn offscreen_skin_shots_default_and_plan_a() {
    for skin in [ShotSkin::Default, ShotSkin::PlanA] {
        println!("[ui-shots-offscreen] skin {} starting", skin.env_name());
        run_skin(skin);
    }
}

fn run_skin(skin: ShotSkin) {
    // insert_ui_skin resolves the theme from WARDEN_SKIN at Startup — one
    // source of truth with the CLI flag (set before the app builds).
    std::env::set_var("WARDEN_SKIN", skin.env_name());

    let mut app = headless_warden();
    app.finish();
    app.cleanup();

    // Startup settles: map, layout, fonts, pipelines (wall-clock: asset
    // loads and shader compilation are async; frame count alone lies).
    run_frames(&mut app, Duration::from_millis(1500));

    // -- staging (the CLI driver's frame script, same gateways) ----------
    app.world_mut().resource_mut::<DebugRun>().marked = true;
    app.world_mut().resource_mut::<Economy>().gold = 120;
    let slots: Vec<(Entity, Vec3)> =
        free_slots_in_order(app.world_mut()).into_iter().take(2).collect();
    assert!(slots.len() >= 2, "the map must offer two free build slots");
    place_towers(app.world_mut(), 0, &slots);
    let hero = {
        let mut q = app.world_mut().query_filtered::<Entity, With<Tower>>();
        let towers: Vec<Entity> = q.iter(app.world()).collect();
        assert!(!towers.is_empty(), "staging placed no tower");
        towers[0]
    };
    let target = app.world().resource::<OffscreenTarget>().0.clone();
    let dir = out_dir(skin);
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
        println!("[ui-shots-offscreen] {stem} ok (max sample {max})");
    };

    // 01 — full HUD, build card, intermission.
    run_frames(&mut app, Duration::from_millis(400));
    shot(&mut app, SHOTS[0].1);

    // 02 — selected tower: sell/fuse/upgrade + skill chips.
    *app.world_mut().resource_mut::<Selection>() = Selection::Tower(hero);
    run_frames(&mut app, Duration::from_millis(300));
    shot(&mut app, SHOTS[1].1);

    // 03 — command card solo close-up.
    solo_bottom_bar(app.world_mut(), Some(SoloTarget::Card));
    run_frames(&mut app, Duration::from_millis(200));
    shot(&mut app, SHOTS[2].1);

    // 04 — info zone solo close-up (tower attributes).
    solo_bottom_bar(app.world_mut(), Some(SoloTarget::InfoZone));
    run_frames(&mut app, Duration::from_millis(200));
    shot(&mut app, SHOTS[3].1);

    // 05 — combat lock: restore zones, real wave 1 through the director,
    // capture mid-fight (build card greyed while the wave runs).
    solo_bottom_bar(app.world_mut(), None);
    *app.world_mut().resource_mut::<Selection>() = Selection::None;
    app.world_mut().resource_mut::<WaveDirector>().force_begin(0);
    run_frames(&mut app, Duration::from_millis(2600));
    shot(&mut app, SHOTS[4].1);

    // 06 — tooltip hover: settle the wave, then a real cursor hover over a
    // build button (an injected PrimaryWindow entity — headless has no OS
    // cursor; bevy_ui's focus system reads this window's cursor position).
    let doomed: Vec<Entity> = {
        let mut q = app.world_mut().query_filtered::<Entity, With<Enemy>>();
        q.iter(app.world()).collect()
    };
    for e in doomed {
        let _ = app.world_mut().entity_mut(e).despawn();
    }
    app.world_mut().resource_mut::<WaveDirector>().force_settle();
    *app.world_mut().resource_mut::<Selection>() = Selection::None;
    run_frames(&mut app, Duration::from_millis(800));
    pin_cursor_over_build_button(&mut app);
    run_frames(&mut app, Duration::from_millis(700));
    shot(&mut app, SHOTS[5].1);

    // The app never runs a runner; drop it (GPU context freed) and move on
    // to the next skin.
    let _ = app.world_mut().write_message(AppExit::Success);
}

/// The full warden stack minus DebugPlugin (bevy_egui demands a window) and
/// the frame cap, on the headless DefaultPlugins shape.
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
                // Shaders ready before the first captured frame (short-lived
                // capture, no room for async pipeline warmup).
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

/// Board the offscreen texture (1280x720 — the real window's logical size,
/// so the UI lays out exactly like the live game) and move the game camera
/// onto it, claiming the UI (`IsDefaultUiCamera`) so world + HUD render
/// into one texture.
fn retarget_offscreen(mut commands: Commands, mut images: ResMut<Assets<Image>>, cameras: Query<Entity, With<Camera3d>>) {
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

/// Non-black guard: sample the GPU readback as it passes through the
/// `ScreenshotCaptured` observer payload (no PNG decode needed).
fn record_captured_max(trigger: On<ScreenshotCaptured>, mut seen: ResMut<CapturedMax>) {
    let Some(data) = &trigger.image.data else { return };
    // Sample every 16th byte (RGBA — any channel) to keep it cheap.
    for b in data.iter().step_by(16) {
        if *b > seen.0 {
            seen.0 = *b;
        }
    }
}

/// Advance updates until `dur` wall time has passed (headless virtual time
/// barely moves in a tight loop — settles must be wall-clock, MMP's
/// discipline).
fn run_frames(app: &mut App, dur: Duration) {
    let t0 = Instant::now();
    while t0.elapsed() < dur {
        app.update();
    }
}

/// Tick until the PNG lands on disk AND the readback observer has fired
/// (the file alone is not proof the pixels left the GPU).
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

/// Inject a PrimaryWindow whose cursor sits on the first build button's
/// center — real hover through bevy_ui's focus system (the MMP
/// `stage_window` trick; no OS mouse exists headless).
fn pin_cursor_over_build_button(app: &mut App) {
    let old: Vec<Entity> = {
        let mut q = app
            .world_mut()
            .query_filtered::<Entity, With<bevy::window::PrimaryWindow>>();
        q.iter(app.world()).collect()
    };
    for e in old {
        let _ = app.world_mut().entity_mut(e).despawn();
    }
    let center = {
        use bevy::ui::{ComputedNode, UiGlobalTransform};
        let world = app.world_mut();
        let mut q = world.query_filtered::<
            (&ComputedNode, &UiGlobalTransform, &CommandButton),
            With<Button>,
        >();
        q.iter(world)
            .find(|(_, _, b)| matches!(b.action, CommandAction::Build(_)))
            .map(|(node, tf, _)| tf.transform_point2(node.size / 2.0))
            .expect("a Build button must exist on the command card")
    };
    let window = app
        .world_mut()
        .spawn((bevy::window::PrimaryWindow, Window::default()))
        .id();
    // Image target scale is 1.0 and the window's default scale is 1.0, so
    // UI physical space == window logical space here (1280x720 both).
    app.world_mut()
        .entity_mut(window)
        .get_mut::<Window>()
        .unwrap()
        .set_cursor_position(Some(center));
}
