//! GamePlugin: every system of the game domain. One domain = one plugin.

use bevy::prelude::*;

use crate::{states::GameState, systems};

pub struct GamePlugin;

impl Plugin for GamePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Startup,
            (
                systems::camera::spawn_camera,
                systems::camera::spawn_environment,
                systems::player::spawn_player,
                spawn_hint_ui,
            ),
        )
        .add_systems(
            Update,
            (
                systems::player::move_player,
                systems::combat::player_attack,
            )
                .run_if(in_state(GameState::Playing)),
        )
        // Pause toggle must run in every state (P resumes from Paused too).
        .add_systems(Update, toggle_pause);
    }
}

/// P toggles Playing/Paused (GameOver resets to Playing).
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
    };
    next.set(new_state);
    info!("[game] state -> {new_state:?}");
}

/// Static UI hint (bevy_ui text pipeline). Dynamic text is a future capability card.
fn spawn_hint_ui(mut commands: Commands) {
    commands.spawn((
        Text::new("WASD move | Space slash | P pause | F12 screenshot"),
        TextFont {
            font_size: FontSize::Px(20.0),
            ..default()
        },
        TextColor(Color::WHITE),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(12.0),
            left: Val::Px(12.0),
            ..default()
        },
    ));
}
