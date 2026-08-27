//! GameStateUI: HP bar, wave counter, slash cooldown bar, and the GameOver screen.
//! Capability card 7 — visual only; validated by running the game.

use bevy::prelude::*;

use crate::components::{
    Attack, Hp, Player, UiCooldownFill, UiGameOver, UiHpFill, UiHpText, UiWaveText,
};
use crate::resources::Wave;
use crate::states::GameState;
use crate::systems::combat::SLASH_COOLDOWN;

pub fn spawn_ui(mut commands: Commands) {
    // HP bar (background + fill).
    commands
        .spawn((
            Node {
                width: Val::Px(220.0),
                height: Val::Px(16.0),
                position_type: PositionType::Absolute,
                top: Val::Px(24.0),
                left: Val::Px(12.0),
                ..default()
            },
            BackgroundColor(Color::srgb(0.15, 0.15, 0.15)),
        ))
        .with_children(|p| {
            p.spawn((
                Node {
                    width: Val::Percent(100.0),
                    height: Val::Percent(100.0),
                    ..default()
                },
                BackgroundColor(Color::srgb(0.85, 0.2, 0.2)),
                UiHpFill,
            ));
        });

    // HP text.
    commands.spawn((
        Text::new("HP 100/100"),
        TextFont {
            font_size: FontSize::Px(16.0),
            ..default()
        },
        TextColor(Color::WHITE),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(24.0),
            left: Val::Px(240.0),
            ..default()
        },
        UiHpText,
    ));

    // Wave counter.
    commands.spawn((
        Text::new("Wave 0"),
        TextFont {
            font_size: FontSize::Px(20.0),
            ..default()
        },
        TextColor(Color::WHITE),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(48.0),
            left: Val::Px(12.0),
            ..default()
        },
        UiWaveText,
    ));

    // Slash cooldown bar (background + fill).
    commands
        .spawn((
            Node {
                width: Val::Px(120.0),
                height: Val::Px(10.0),
                position_type: PositionType::Absolute,
                top: Val::Px(80.0),
                left: Val::Px(12.0),
                ..default()
            },
            BackgroundColor(Color::srgb(0.15, 0.15, 0.15)),
        ))
        .with_children(|p| {
            p.spawn((
                Node {
                    width: Val::Percent(100.0),
                    height: Val::Percent(100.0),
                    ..default()
                },
                BackgroundColor(Color::srgb(0.3, 0.6, 0.9)),
                UiCooldownFill,
            ));
        });

    // GameOver screen (hidden until the player dies).
    commands.spawn((
        Text::new("GAME OVER"),
        TextFont {
            font_size: FontSize::Px(48.0),
            ..default()
        },
        TextColor(Color::srgb(0.9, 0.2, 0.2)),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(0.0),
            left: Val::Px(0.0),
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            flex_direction: FlexDirection::Column,
            ..default()
        },
        Visibility::Hidden,
        UiGameOver,
    ));
}

pub fn ui_update(
    player: Query<&Hp, With<Player>>,
    player_attack: Query<&Attack, With<Player>>,
    wave: Res<Wave>,
    state: Res<State<GameState>>,
    mut hp_fill: Query<&mut Node, (With<UiHpFill>, Without<UiCooldownFill>)>,
    mut hp_text: Query<&mut Text, (With<UiHpText>, Without<UiWaveText>, Without<UiGameOver>)>,
    mut wave_text: Query<&mut Text, (With<UiWaveText>, Without<UiHpText>, Without<UiGameOver>)>,
    mut cd_fill: Query<&mut Node, (With<UiCooldownFill>, Without<UiHpFill>)>,
    mut game_over: Query<(&mut Text, &mut Visibility), (With<UiGameOver>, Without<UiHpText>, Without<UiWaveText>)>,
) {
    // HP bar.
    if let Ok(hp) = player.single() {
        let pct = (hp.hp / hp.max).clamp(0.0, 1.0);
        if let Ok(mut node) = hp_fill.single_mut() {
            node.width = Val::Percent(pct * 100.0);
        }
        if let Ok(mut text) = hp_text.single_mut() {
            text.0 = format!("HP {:.0}/{:.0}", hp.hp, hp.max);
        }
    }
    // Slash cooldown bar.
    if let Ok(attack) = player_attack.single() {
        let ready = (1.0 - attack.cooldown / SLASH_COOLDOWN).clamp(0.0, 1.0);
        if let Ok(mut node) = cd_fill.single_mut() {
            node.width = Val::Percent(ready * 100.0);
        }
    }
    // Wave counter.
    if let Ok(mut text) = wave_text.single_mut() {
        text.0 = format!("Wave {}", wave.n);
    }
    // GameOver screen.
    if let Ok((mut text, mut vis)) = game_over.single_mut() {
        let dead = *state.get() == GameState::GameOver;
        *vis = if dead { Visibility::Visible } else { Visibility::Hidden };
        if dead {
            text.0 = format!("GAME OVER — survived to wave {}\n\nPress R to restart", wave.n);
        }
    }
}
