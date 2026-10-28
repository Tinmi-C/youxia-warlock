//! LG3 offscreen panel screenshots (the warden `--ui-shots` lock-proof
//! producer pattern, tests/ui_shots.rs): DefaultPlugins with
//! `primary_window: None` + `WinitPlugin` disabled, the top-down game camera
//! re-targeted onto an offscreen `Image`, `Screenshot::image` readback,
//! manual `finish()/cleanup()` — zero window, zero display server, zero lock
//! dependence, deterministic reruns day or night.
//!
//! Five shots into `docs/img/` (staged round state, all Plan-A panels):
//!   cargo test -p legion --test ui_shots -- --ignored --nocapture
//!   01-panorama-build.png   full HUD over the greybox arena (build phase)
//!   02-shop-card.png        hire shop card solo close-up
//!   03-lives-board.png      four-player lives board solo close-up
//!   04-round-banner.png     round banner solo (build state, countdown)
//!   05-round-banner-settle.png  the settle flash state (frozen for capture)

use bevy::app::AppExit;
use bevy::camera::RenderTarget;
use bevy::ecs::observer::On;
use bevy::prelude::*;
use bevy::render::view::screenshot::{save_to_disk, Screenshot, ScreenshotCaptured};
use bevy::render::RenderPlugin;
use bevy::ui::IsDefaultUiCamera;
use bevy::window::{ExitCondition, WindowPlugin};
use bevy::winit::WinitPlugin;
use legion::components::DeploySlot;
use legion::resources::{Phase, RoundFlow};
use warbase::prelude::{PlayerLives, Wallets};
use legion::systems::panels::{PanelKind, PanelsSolo, SettleFlash, SETTLE_FLASH_FRAMES};
use legion::systems::round::RoundSettled;
use std::time::{Duration, Instant};

/// Offscreen target handle for the later `Screenshot::image` captures.
#[derive(Resource)]
struct OffscreenTarget(Handle<Image>);

/// Brightest captured sample so far (reset before each shot; the non-black
/// guard — a silently-black capture fails the test instead of shipping).
#[derive(Resource, Default)]
struct CapturedMax(u8);

/// The lock-proof LG3 producer: five staged shots, non-black asserted on the
/// GPU readback. GPU + a few seconds: `#[ignore]`d, run with `--ignored`.
#[test]
#[ignore = "GPU offscreen render (evidence capture only) — run with --ignored"]
fn offscreen_panel_shots() {
    let mut app = headless_legion();
    app.finish();
    app.cleanup();

    // Startup settles: map, kits, fonts, pipelines (wall-clock: asset loads
    // and shader compilation are async; frame count alone lies).
    run_frames(&mut app, Duration::from_millis(1500));

    // -- staging -----------------------------------------------------------
    for (i, gold) in [300, 260, 240, 220].into_iter().enumerate() {
        app.world_mut().resource_mut::<Wallets>().0[i].gold = gold;
    }
    app.world_mut().resource_mut::<PlayerLives>().0 = vec![20, 17, 12, 9];
    // Two mercs on P0's line so the panorama carries a field.
    let slots: Vec<Entity> = {
        let world = app.world_mut();
        let mut q = world.query_filtered::<(Entity, &DeploySlot), ()>();
        let mut own: Vec<Entity> = q
            .iter(world)
            .filter(|(_, s)| s.player == 0)
            .map(|(e, _)| e)
            .collect();
        own.sort_by_key(|&e| e.index());
        own.truncate(2);
        own
    };
    for (i, slot) in slots.iter().enumerate() {
        let def = if i == 0 { "merc_tank" } else { "merc_ranged" };
        app.world_mut().write_message(legion::systems::round::HireCommand {
            player: 0,
            def: def.into(),
            slot: *slot,
        });
    }
    run_frames(&mut app, Duration::from_millis(400));

    let target = app.world().resource::<OffscreenTarget>().0.clone();
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("docs/img");
    std::fs::create_dir_all(&dir).unwrap();
    let shot = |app: &mut App, stem: &str| {
        let path = dir.join(format!("{stem}.png"));
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
        println!("[legion ui-shots] {stem} ok (max sample {max})");
    };
    // Reset the round clock to a readable mid-build state (headless updates
    // run far faster than 60fps — without the pin the phase would expire
    // mid-shoot).
    let pin_build = |app: &mut App| {
        *app.world_mut().resource_mut::<RoundFlow>() =
            RoundFlow { round: 3, phase: Phase::Build, frame: 1080 }; // 12s left
    };

    // 01 — full HUD panorama, build phase, staged field.
    pin_build(&mut app);
    run_frames(&mut app, Duration::from_millis(300));
    shot(&mut app, "01-panorama-build");

    // 02 — hire shop card solo close-up.
    app.world_mut().resource_mut::<PanelsSolo>().0 = Some(PanelKind::Shop);
    run_frames(&mut app, Duration::from_millis(200));
    shot(&mut app, "02-shop-card");

    // 03 — lives board solo close-up.
    app.world_mut().resource_mut::<PanelsSolo>().0 = Some(PanelKind::Lives);
    run_frames(&mut app, Duration::from_millis(200));
    shot(&mut app, "03-lives-board");

    // 04 — round banner solo (build state with the countdown reading).
    pin_build(&mut app);
    app.world_mut().resource_mut::<PanelsSolo>().0 = Some(PanelKind::Banner);
    run_frames(&mut app, Duration::from_millis(200));
    shot(&mut app, "04-round-banner");

    // 05 — the settle flash state: light it, then hold it alive through the
    // capture wait (the flash decays one frame per update; headless updates
    // outrun the PNG landing).
    app.world_mut().write_message(RoundSettled { round: 3, income: 50 });
    app.update();
    assert!(
        app.world().get_resource::<SettleFlash>().is_some(),
        "the settle flash must light for the shot"
    );
    let flash = SettleFlash { round: 3, income: 50, frames_left: SETTLE_FLASH_FRAMES };
    let path = dir.join("05-round-banner-settle.png");
    *app.world_mut().resource_mut::<CapturedMax>() = CapturedMax(0);
    app.world_mut()
        .spawn(Screenshot::image(target.clone()))
        .observe(save_to_disk(path.clone()))
        .observe(record_captured_max);
    let t0 = Instant::now();
    while t0.elapsed() < Duration::from_secs(10) {
        // Re-pin the flash BEFORE each update so decay never wins the race.
        *app.world_mut().resource_mut::<SettleFlash>() = flash.clone();
        app.update();
        if let Ok(meta) = std::fs::metadata(&path) {
            if meta.len() > 0 && app.world().resource::<CapturedMax>().0 > 0 {
                break;
            }
        }
    }
    assert!(std::fs::metadata(&path).is_ok_and(|m| m.len() > 0), "settle shot landed");
    assert!(
        app.world().resource::<CapturedMax>().0 > 24,
        "settle shot is black — offscreen render broke"
    );
    println!("[legion ui-shots] 05-round-banner-settle ok");

    let _ = app.world_mut().write_message(AppExit::Success);
}

/// The full legion stack on the headless DefaultPlugins shape (greybox
/// visuals included — the render stores exist).
fn headless_legion() -> App {
    let mut app = App::new();
    app.insert_resource(ClearColor(Color::srgb(0.10, 0.11, 0.10)))
        .insert_resource(GlobalAmbientLight {
            color: Color::WHITE,
            brightness: 900.0,
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
        .add_plugins((
            warbase::WarbasePlugin,
            legion::plugins::game::GamePlugin,
            legion::plugins::visual::VisualPlugin,
        ))
        .init_resource::<CapturedMax>()
        .add_systems(
            Startup,
            retarget_offscreen.after(legion::systems::camera::spawn_camera),
        );
    app
}

/// Board the offscreen texture (1280x720 — the real window's logical size,
/// so the UI lays out exactly like the live game) and move the game camera
/// onto it, claiming the UI (`IsDefaultUiCamera`) so world + HUD render into
/// one texture.
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
/// barely moves in a tight loop — settles must be wall-clock).
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
