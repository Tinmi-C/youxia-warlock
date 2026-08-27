//! GamePlugin: every system of the game domain. One domain = one plugin.

use bevy::prelude::*;

use crate::components::{Attack, Hp, Monster, NovaAttack, Player};
use crate::{resources::Balance, resources::Wave, states::GameState, systems};

pub struct GamePlugin;

impl Plugin for GamePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Wave>()
            .init_resource::<Balance>()
            .add_message::<systems::nova::NovaFired>()
            .add_systems(
                Startup,
                (
                    systems::camera::spawn_camera,
                    systems::camera::spawn_environment,
                    systems::player::spawn_player,
                    systems::ui::spawn_ui,
                    spawn_hint_ui,
                ),
            )
            .add_systems(
                Update,
                (
                    systems::player::move_player,
                    systems::enemy::enemy_chase,
                    systems::combat::player_attack,
                    systems::nova::nova_slash,
                    systems::contact::contact_damage,
                    systems::contact::death_despawn,
                    systems::pickup::pickup_drop,
                    systems::wave::wave_system,
                    // card 12: after every Transform writer above
                    systems::player::update_walk_cycle,
                )
                    .chain()
                    .run_if(in_state(GameState::Playing)),
            )
            // HUD updates in every state (so the GameOver screen can show).
            .add_systems(Update, systems::ui::ui_update)
            // Pause toggle runs in Playing/Paused; restart runs only in GameOver.
            .add_systems(Update, toggle_pause)
            .add_systems(Update, restart.run_if(in_state(GameState::GameOver)))
            // card 12: outside Playing nothing moves the player — hold the walk
            // flag down so the model never moonwalks through pause/GameOver.
            .add_systems(
                Update,
                systems::player::clear_walk_on_pause.run_if(not(in_state(GameState::Playing))),
            );
    }
}

/// P toggles Playing/Paused. It does NOT revive a dead player (R restarts).
fn toggle_pause(
    keys: Res<ButtonInput<KeyCode>>,
    state: Res<State<GameState>>,
    mut next: ResMut<NextState<GameState>>,
) {
    if !keys.just_pressed(KeyCode::KeyP) {
        return;
    }
    match state.get() {
        GameState::Playing => next.set(GameState::Paused),
        GameState::Paused => next.set(GameState::Playing),
        GameState::GameOver => {} // dead: R restarts, P does nothing
    }
}

/// R restarts from GameOver: clear monsters, reset the player, reset the wave.
fn restart(
    keys: Res<ButtonInput<KeyCode>>,
    mut commands: Commands,
    monsters: Query<Entity, With<Monster>>,
    mut player: Query<(&mut Hp, &mut Transform, &mut Attack, &mut NovaAttack), With<Player>>,
    mut wave: ResMut<Wave>,
    mut next: ResMut<NextState<GameState>>,
) {
    if !keys.just_pressed(KeyCode::KeyR) {
        return;
    }
    for e in &monsters {
        commands.entity(e).despawn();
    }
    if let Ok((mut hp, mut tf, mut attack, mut nova)) = player.single_mut() {
        hp.hp = hp.max;
        hp.invuln = 0.0;
        tf.translation = Vec3::new(0.0, 0.5, 0.0);
        attack.cooldown = 0.0;
        nova.cooldown = 0.0;
    }
    *wave = Wave::default();
    next.set(GameState::Playing);
    info!("[game] restart — back to wave 0");
}

/// Static UI hint (bevy_ui text pipeline). Dynamic text is a future capability card.
fn spawn_hint_ui(mut commands: Commands) {
    commands.spawn((
        Text::new("WASD move | Space slash | P pause | R restart | F12 screenshot"),
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
