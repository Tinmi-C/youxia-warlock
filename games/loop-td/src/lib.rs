//! Game library: assembles the app and exposes modules to tests.
//! `main.rs` only calls `build_app()`; business logic lives in plugins/systems.

pub mod components;
pub mod plugins;
pub mod resources;
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
            DefaultPlugins.set(WindowPlugin {
                primary_window: Some(Window {
                    title: "loop-td".into(),
                    resolution: (1280, 720).into(),
                    ..default()
                }),
                ..default()
            }),
        )
        .init_state::<states::GameState>()
        .add_plugins((
            // Battle runtime base (ADR-0012): set contract + mechanisms.
            // Game logic plugs in around WarbaseSet via before/after.
            warbase::WarbasePlugin,
            plugins::game::GamePlugin,
            plugins::debug::DebugPlugin,
        ));
    app
}
