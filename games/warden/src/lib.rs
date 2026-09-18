//! Game library: assembles the app and exposes modules to tests.
//! `main.rs` only calls `build_app()`; business logic lives in plugins/systems.

pub mod components;
pub mod plugins;
pub mod resources;
pub mod sets;
pub mod states;
pub mod systems;

use bevy::prelude::*;

/// Assemble the full app. Shared by `main` (run) and could be reused by
/// integration tests that need the whole stack.
pub fn build_app() -> App {
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
                    primary_window: Some(Window {
                        title: "Warden".into(),
                        resolution: (1280, 720).into(),
                        ..default()
                    }),
                    ..default()
                })
                // icu_provider warns on every CJK re-layout because bevy's
                // bundled ICU4X data ships no CJK segmentation model; without
                // this the log floods (measured ~1200 lines/sec). The text
                // itself renders fine.
                .set(bevy::log::LogPlugin {
                    filter: "wgpu=error,naga=warn,icu_provider=off".into(),
                    ..default()
                }),
        )
        // Frame throttling: `WinitSettings` reactive modes do NOT throttle on
        // macOS (the winit loop self-wakes, measured ~120fps with reactive
        // low-power), so a sleep-based cap in `First` enforces it for real.
        // Focused: 60fps. Unfocused: 10fps (background waves slow-mo on
        // purpose — a single-player TD must not burn a core while idle).
        .insert_resource(FrameCap {
            focused_min_frame: std::time::Duration::from_secs_f64(1.0 / 60.0),
            unfocused_min_frame: std::time::Duration::from_secs_f64(1.0 / 10.0),
            focused: None,
            last_tick: None,
        })
        .add_systems(First, enforce_frame_cap)
        .init_state::<states::GameState>()
        // Domain plugins: one domain = one plugin (team convention —
        // "a domain per Plugin" avoids god-files). Gameplay domains (towers /
        // enemies / waves) are declared now as seams; capability cards fill them.
        .add_plugins((
            plugins::fx::FxPlugin,
            plugins::game::GamePlugin,
            plugins::map::MapPlugin,
            plugins::towers::TowersPlugin,
            plugins::enemies::EnemiesPlugin,
            plugins::waves::WavesPlugin,
            plugins::economy::EconomyPlugin,
            plugins::acquisition::AcquisitionPlugin,
            plugins::meta::MetaPlugin,
            plugins::projectiles::ProjectilesPlugin,
            plugins::skills::SkillsPlugin,
            plugins::ui::UiPlugin,
            plugins::debug::DebugPlugin,
        ));
    app
}

/// Sleep-based frame limiter state (see `enforce_frame_cap`).
#[derive(Resource)]
struct FrameCap {
    focused_min_frame: std::time::Duration,
    unfocused_min_frame: std::time::Duration,
    /// `Window::focused` defaults to `true` and never corrects itself until a
    /// real focus event arrives, so a window that was never clicked (launched
    /// from a terminal, occluded in the background) wrongly reads as focused.
    /// Track the events ourselves and assume NOT focused until proven.
    focused: Option<bool>,
    last_tick: Option<std::time::Instant>,
}

/// Pads each frame with a sleep so the loop respects the cap. Runs first in
/// the schedule; sleeping here delays the whole frame, which is exactly the
/// throttle we want (sim + render + present all slow down together).
fn enforce_frame_cap(
    mut cap: ResMut<FrameCap>,
    mut focus: MessageReader<bevy::window::WindowFocused>,
) {
    for e in focus.read() {
        cap.focused = Some(e.focused);
    }
    let focused = cap.focused.unwrap_or(false);
    let min = if focused {
        cap.focused_min_frame
    } else {
        cap.unfocused_min_frame
    };
    let now = std::time::Instant::now();
    if let Some(last) = cap.last_tick {
        let elapsed = now.duration_since(last);
        if elapsed < min {
            std::thread::sleep(min - elapsed);
        }
    }
    cap.last_tick = Some(std::time::Instant::now());
}
