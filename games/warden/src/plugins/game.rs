//! GamePlugin: camera + placement cursor + player input. It also owns the
//! cross-domain `GameSet` ordering (the temporal contract) so every domain
//! plugin can mount systems into a named stage without hand-editing a chain.

use bevy::prelude::*;

use crate::resources::{BuildMode, DebugPanelCapture};
use crate::sets::GameSet;
use crate::states::GameState;
use crate::systems;

pub struct GamePlugin;

impl Plugin for GamePlugin {
    fn build(&self, app: &mut App) {
        // Temporal contract: input -> placement -> spawn -> simulate -> cleanup
        // -> observe. Systems mount by name in each domain plugin.
        app.configure_sets(
            Update,
            (
                GameSet::Input,
                GameSet::Placement,
                GameSet::Spawn,
                GameSet::Simulate,
                GameSet::Cleanup,
                GameSet::Observe,
            )
                .chain(),
        )
        .init_resource::<BuildMode>()
        // Debug-panel input capture (DB1): registered unconditionally (the
        // guard systems read it in release too); only debug builds write it.
        .init_resource::<DebugPanelCapture>()
        // D1 晴昼要塞 ambience (docs/art-direction.md, 2026-09-20 re-tone):
        // morning blue-white sky bounce and a pale blue clear color behind
        // the cliff skirt (replaces the rejected dusk purple/brown).
        // (bevy 0.19: the global ambient moved off `AmbientLight`, which is
        // now a component.)
        .insert_resource(GlobalAmbientLight {
            color: Color::srgb(0.60, 0.65, 0.75),
            brightness: 90.0,
            ..default()
        })
        .insert_resource(ClearColor(Color::srgb(0.55, 0.65, 0.80)))
        .add_systems(Startup, systems::camera::spawn_camera)
        .add_systems(
            Update,
            (
                systems::input::choose_choice.in_set(GameSet::Input),
                systems::input::start_next_wave.in_set(GameSet::Input),
                systems::command::handle_command_buttons
                    .in_set(GameSet::Input)
                    // SK10: its TowerSkill dispatch queues CastSkill — keep
                    // it ahead of the framework's validator (same frame).
                    .before(systems::skills::handle_cast_requests),
                systems::command::command_grid_hotkeys
                    .in_set(GameSet::Input)
                    .before(systems::command::handle_command_buttons),
                systems::pointer::handle_start_wave_button.in_set(GameSet::Input),
                systems::pointer::handle_choice_card_buttons.in_set(GameSet::Input),
                systems::command::select_object
                    .in_set(GameSet::Input)
                    .before(systems::command::apply_pending_fusion),
                systems::pointer::mouse_place_system.in_set(GameSet::Input),
                systems::command::apply_pending_fusion.in_set(GameSet::Input),
            )
                .run_if(in_state(GameState::Playing)),
        )
        // Pause toggle must run in every state (P resumes from GameOver/Win too).
        .add_systems(Update, toggle_pause);
    }
}

/// Shared pause-state transition for the P key and the pause button (UI9).
pub fn apply_pause(state: &State<GameState>, next: &mut NextState<GameState>) {
    let new_state = match state.get() {
        GameState::Playing => GameState::Paused,
        GameState::Paused => GameState::Playing,
        GameState::GameOver => GameState::Playing,
        GameState::Win => GameState::Playing,
        // The studio viewer has nothing to pause (SM1); P is a no-op there.
        GameState::Showroom => GameState::Showroom,
    };
    next.set(new_state);
    info!("[game] state -> {new_state:?}");
}

/// P toggles Playing/Paused (GameOver/Win reset to Playing).
fn toggle_pause(
    keys: Res<ButtonInput<KeyCode>>,
    panel: Res<DebugPanelCapture>,
    state: Res<State<GameState>>,
    mut next: ResMut<NextState<GameState>>,
) {
    // Typing into a debug-panel field must not pause the game (DB1).
    if panel.keyboard {
        return;
    }
    if keys.just_pressed(KeyCode::KeyP) {
        apply_pause(&state, &mut next);
    }
}
