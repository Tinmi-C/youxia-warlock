//! GameStateUI (cards 7 + 16): HP bar, HP text, slash & Nova cooldown bars,
//! wave-alive pips, pause overlay, and the GameOver screen.
//! Card 7 built the base HUD (visual only); card 16 formalizes it: fills the
//! GDD line-30 gaps (波次格子 / Nova 冷却条 / 暂停), moves the debug hint line
//! out of the HUD corner, and收敛 layout/palette into the constant group below
//! (single tuning point). Data sources are read-only: Hp / Attack / NovaAttack /
//! Wave / alive-Monster count / GameState. The F1 egui tuning panel (card 11)
//! is a separate pipeline and stays untouched.

use bevy::prelude::*;

use crate::components::{
    Attack, EquippedWeapon, Hp, Monster, NovaAttack, Player, UiCooldownFill, UiGameOver, UiHpFill,
    UiHpText, UiNovaFill, UiPauseOverlay, UiWavePips, UiWaveText, WeaponKind,
};
use crate::resources::{Balance, Wave};
use crate::states::GameState;
use crate::systems::nova::NOVA_COOLDOWN;

// --- card 16: layout & palette constants (single tuning point) ---
const MARGIN: f32 = 12.0;
const HP_BAR: (f32, f32) = (220.0, 16.0);
const CD_BAR: (f32, f32) = (120.0, 10.0);
const PIP: (f32, f32) = (10.0, 10.0);
const SLASH_BAR_TOP: f32 = 80.0;
const NOVA_BAR_TOP: f32 = 96.0;
const COLOR_PANEL: Color = Color::srgb(0.15, 0.15, 0.15);
const COLOR_HP: Color = Color::srgb(0.85, 0.2, 0.2);
const COLOR_CD_SLASH: Color = Color::srgb(0.3, 0.6, 0.9);
const COLOR_CD_NOVA: Color = Color::srgb(0.48, 0.36, 1.0);
const COLOR_PIP: Color = Color::srgb(0.85, 0.2, 0.2);
/// Translucent black used by the pause overlay and the GameOver backdrop.
const COLOR_OVERLAY: Color = Color::srgba(0.0, 0.0, 0.0, 0.6);
const COLOR_TEXT_DIM: Color = Color::srgba(1.0, 1.0, 1.0, 0.6);

pub fn spawn_ui(mut commands: Commands) {
    // HP bar (background + fill).
    commands
        .spawn((
            Node {
                width: Val::Px(HP_BAR.0),
                height: Val::Px(HP_BAR.1),
                position_type: PositionType::Absolute,
                top: Val::Px(24.0),
                left: Val::Px(MARGIN),
                ..default()
            },
            BackgroundColor(COLOR_PANEL),
        ))
        .with_children(|p| {
            p.spawn((
                Node {
                    width: Val::Percent(100.0),
                    height: Val::Percent(100.0),
                    ..default()
                },
                BackgroundColor(COLOR_HP),
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
            left: Val::Px(MARGIN + HP_BAR.0 + 8.0),
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
            left: Val::Px(MARGIN),
            ..default()
        },
        UiWaveText,
    ));

    // Slash cooldown bar (background + fill).
    commands
        .spawn((
            Node {
                width: Val::Px(CD_BAR.0),
                height: Val::Px(CD_BAR.1),
                position_type: PositionType::Absolute,
                top: Val::Px(SLASH_BAR_TOP),
                left: Val::Px(MARGIN),
                ..default()
            },
            BackgroundColor(COLOR_PANEL),
        ))
        .with_children(|p| {
            p.spawn((
                Node {
                    width: Val::Percent(100.0),
                    height: Val::Percent(100.0),
                    ..default()
                },
                BackgroundColor(COLOR_CD_SLASH),
                UiCooldownFill,
            ));
        });

    // card 16: Nova cooldown bar — mirrors the slash bar, violet to read as
    // "Nova family" (card 9 shockwave is golden; bar hue stays distinguishable).
    commands
        .spawn((
            Node {
                width: Val::Px(CD_BAR.0),
                height: Val::Px(CD_BAR.1),
                position_type: PositionType::Absolute,
                top: Val::Px(NOVA_BAR_TOP),
                left: Val::Px(MARGIN),
                ..default()
            },
            BackgroundColor(COLOR_PANEL),
        ))
        .with_children(|p| {
            p.spawn((
                Node {
                    width: Val::Percent(100.0),
                    height: Val::Percent(100.0),
                    ..default()
                },
                BackgroundColor(COLOR_CD_NOVA),
                UiNovaFill,
            ));
        });

    // card 16: wave-alive pips — one red square per living monster, centered.
    // The row spans full width and centers its children so the count can grow
    // symmetrically; actual pip entities are synced per-frame in `ui_update`.
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(24.0),
            left: Val::Px(0.0),
            width: Val::Percent(100.0),
            height: Val::Px(PIP.1),
            justify_content: JustifyContent::Center,
            ..default()
        },
        UiWavePips,
    ));

    // card 16: pause overlay (hidden until P).
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            ..default()
        },
        BackgroundColor(COLOR_OVERLAY),
        Text::new("PAUSED — P to resume"),
        TextFont {
            font_size: FontSize::Px(32.0),
            ..default()
        },
        TextColor(Color::WHITE),
        Visibility::Hidden,
        UiPauseOverlay,
    ));

    // GameOver screen (hidden until the player dies); card 16 adds a backdrop.
    commands.spawn((
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
        BackgroundColor(COLOR_OVERLAY),
        Text::new("GAME OVER"),
        TextFont {
            font_size: FontSize::Px(48.0),
            ..default()
        },
        TextColor(Color::srgb(0.9, 0.2, 0.2)),
        Visibility::Hidden,
        UiGameOver,
    ));

    // card 16: the debug-era control hints move to a dim bottom line — they are
    // still useful for new players but must not sit on top of the HUD corner.
    commands.spawn((
        Text::new("WASD move | Shift run | Q nova | Space slash | P pause | R restart | F12 shot"),
        TextFont {
            font_size: FontSize::Px(12.0),
            ..default()
        },
        TextColor(COLOR_TEXT_DIM),
        Node {
            position_type: PositionType::Absolute,
            bottom: Val::Px(MARGIN),
            left: Val::Px(0.0),
            width: Val::Percent(100.0),
            justify_content: JustifyContent::Center,
            ..default()
        },
    ));
}

#[allow(clippy::too_many_arguments)]
pub fn ui_update(
    mut commands: Commands,
    player: Query<&Hp, With<Player>>,
    player_attack: Query<(&Attack, Option<&EquippedWeapon>), With<Player>>,
    player_nova: Query<&NovaAttack, With<Player>>,
    balance: Res<Balance>,
    wave: Res<Wave>,
    state: Res<State<GameState>>,
    monsters: Query<(), With<Monster>>,
    mut hp_fill: Query<&mut Node, (With<UiHpFill>, Without<UiCooldownFill>, Without<UiNovaFill>)>,
    mut hp_text: Query<
        &mut Text,
        (
            With<UiHpText>,
            Without<UiWaveText>,
            Without<UiGameOver>,
            Without<UiPauseOverlay>,
        ),
    >,
    mut wave_text: Query<
        &mut Text,
        (
            With<UiWaveText>,
            Without<UiHpText>,
            Without<UiGameOver>,
            Without<UiPauseOverlay>,
        ),
    >,
    mut cd_fill: Query<&mut Node, (With<UiCooldownFill>, Without<UiHpFill>, Without<UiNovaFill>)>,
    mut nova_fill: Query<&mut Node, (With<UiNovaFill>, Without<UiHpFill>, Without<UiCooldownFill>)>,
    mut game_over: Query<
        (&mut Text, &mut Visibility),
        (
            With<UiGameOver>,
            Without<UiHpText>,
            Without<UiWaveText>,
            Without<UiPauseOverlay>,
        ),
    >,
    mut pause_overlay: Query<
        &mut Visibility,
        (
            With<UiPauseOverlay>,
            Without<UiGameOver>,
            Without<UiHpText>,
            Without<UiWaveText>,
        ),
    >,
    mut pips: Query<
        (Entity, &mut Node, Option<&Children>),
        (
            With<UiWavePips>,
            Without<UiHpFill>,
            Without<UiCooldownFill>,
            Without<UiNovaFill>,
        ),
    >,
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
    // Slash cooldown bar (card 29: mirrors the equipped weapon's table
    // cooldown × live balance scale, like combat and the AttackRoot window).
    if let Ok((attack, equipped)) = player_attack.single() {
        let max_cd = equipped
            .map(|e| e.0.cooldown())
            .unwrap_or(WeaponKind::IronSword.cooldown())
            * balance.slash_cooldown_scale;
        let ready = (1.0 - attack.cooldown / max_cd).clamp(0.0, 1.0);
        if let Ok(mut node) = cd_fill.single_mut() {
            node.width = Val::Percent(ready * 100.0);
        }
    }
    // card 16: Nova cooldown bar — same formula, its own cooldown resource.
    if let Ok(nova) = player_nova.single() {
        let ready = (1.0 - nova.cooldown / NOVA_COOLDOWN).clamp(0.0, 1.0);
        if let Ok(mut node) = nova_fill.single_mut() {
            node.width = Val::Percent(ready * 100.0);
        }
    }
    // Wave counter.
    if let Ok(mut text) = wave_text.single_mut() {
        text.0 = format!("Wave {}", wave.n);
    }
    // card 16: wave-alive pips — rebuild only when the count changes (kills are
    // rare events; this never churns on idle frames). Rebuild = despawn all +
    // respawn `alive` pips, so the count can only ever be exact. Note: Children
    // is removed by bevy once the last child despawns, hence Option<&Children>.
    if let Ok((container, mut row, children)) = pips.single_mut() {
        let alive = monsters.iter().count();
        let current = children.map(|c| c.len()).unwrap_or(0);
        if current != alive {
            if let Some(existing) = children {
                for child in existing.iter() {
                    commands.entity(child).despawn();
                }
            }
            commands.entity(container).with_children(|p| {
                for _ in 0..alive {
                    p.spawn((
                        Node {
                            width: Val::Px(PIP.0),
                            height: Val::Px(PIP.1),
                            margin: UiRect::horizontal(Val::Px(2.0)),
                            ..default()
                        },
                        BackgroundColor(COLOR_PIP),
                    ));
                }
            });
        }
        // row height follows the pip so the centered strip never clips
        row.height = Val::Px(PIP.1);
    }
    // GameOver screen.
    if let Ok((mut text, mut vis)) = game_over.single_mut() {
        let dead = *state.get() == GameState::GameOver;
        *vis = if dead {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
        if dead {
            text.0 = format!(
                "GAME OVER — survived to wave {}\n\nPress R to restart",
                wave.n
            );
        }
    }
    // card 16: pause overlay.
    if let Ok(mut vis) = pause_overlay.single_mut() {
        *vis = if *state.get() == GameState::Paused {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
    }
}
