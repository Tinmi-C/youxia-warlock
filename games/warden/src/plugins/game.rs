//! GamePlugin: camera + placement cursor + player input. It also owns the
//! cross-domain `GameSet` ordering (the temporal contract) so every domain
//! plugin can mount systems into a named stage without hand-editing a chain.

use bevy::prelude::*;

use crate::resources::{BuildMode, FusionSel};
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
        .init_resource::<FusionSel>()
        .add_systems(
            Startup,
            (
                systems::camera::spawn_camera,
                systems::cursor::spawn_placement_cursor,
            ),
        )
        .add_systems(
            Update,
            (
                systems::cursor::move_cursor.in_set(GameSet::Input),
                systems::input::select_tower.in_set(GameSet::Input),
                systems::input::choose_choice.in_set(GameSet::Input),
                systems::input::start_next_wave.in_set(GameSet::Input),
                systems::pointer::handle_shop_buttons.in_set(GameSet::Input),
                systems::pointer::mouse_cancel.in_set(GameSet::Input),
                systems::pointer::mouse_place_system.in_set(GameSet::Input),
                systems::pointer::mouse_fuse_system.in_set(GameSet::Input),
            )
                .run_if(in_state(GameState::Playing)),
        )
        .add_systems(
            Update,
            systems::input::place_tower
                .in_set(GameSet::Placement)
                .run_if(in_state(GameState::Playing)),
        )
        // Pause toggle must run in every state (P resumes from GameOver/Win too).
        .add_systems(Update, toggle_pause);
    }
}

/// P toggles Playing/Paused (GameOver/Win reset to Playing).
fn toggle_pause(
    keys: Res<ButtonInput<KeyCode>>,
    state: Res<State<GameState>>,
    mut next: ResMut<NextState<GameState>>,
) {
    if !keys.just_pressed(KeyCode::KeyP) {
        return;
    }
    let new_state = match state.get() {
        GameState::Playing => GameState::Paused,
        GameState::Paused => GameState::Playing,
        GameState::GameOver => GameState::Playing,
        GameState::Win => GameState::Playing,
    };
    next.set(new_state);
    info!("[game] state -> {new_state:?}");
}
