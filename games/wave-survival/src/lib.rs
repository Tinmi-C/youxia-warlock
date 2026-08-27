//! Game library: assembles the app and exposes modules to tests.
//! `main.rs` only calls `build_app()`; business logic lives in plugins/systems.

pub mod components;
pub mod plugins;
pub mod resources;
pub mod states;
pub mod systems;

use bevy::prelude::*;
use bevy_rapier3d::prelude::{NoUserData, RapierPhysicsPlugin};

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
                    title: "wave-survival".into(),
                    resolution: (1280, 720).into(),
                    ..default()
                }),
                ..default()
            }),
        )
        .add_plugins(RapierPhysicsPlugin::<NoUserData>::default())
        .init_state::<states::GameState>()
        // bevy_egui drives the F1 tuning panel (card 11); the panel plugin must
        // come after GamePlugin so `Balance` already exists as a resource.
        .add_plugins(bevy_egui::EguiPlugin::default())
        .add_plugins(
            (
                plugins::game::GamePlugin,
                plugins::vfx::VfxPlugin,
                plugins::tuning::TuningPlugin,
                plugins::debug::DebugPlugin,
            )
        );
    app
}
