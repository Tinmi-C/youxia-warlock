//! Command card (UI9, requirements §16): a persistent three-zone bottom bar
//! (left info / center command card / right control) whose center content
//! follows the `Selection` state machine (五态, §16.2).
//!   - Left click selects a tower or an empty slot; blank click / right click
//!     / Esc cancel everything (§16.3; Esc is the unified cancel).
//!   - The command card greys build/sell/fuse during combat (锁建造铁律),
//!     while the info zone keeps showing attributes (viewing is unrestricted).
//!   - The control zone absorbs start-wave / 2x-placeholder / pause / language.
//! Build and sell/fuse button *actions* arrive with UI10 / UI11+TO7; this
//! card pins the skeleton, the states, the greying and the cancel contract.
//! The old right-top info panel retires here (absorbed by the info zone);
//! the legacy bottom shop bar retires with UI10.

use bevy::input::mouse::MouseButton;
use bevy::prelude::*;
use bevy::window::Window;

use crate::components::{Tower, TowerKind, TowerSlot};
use crate::resources::{
    BuildMode, Economy, FusionDefs, FusionSel, Hand, Lang, Selection, ShopOffers, TowerDefs,
    WavePhase, WaveState,
};
use crate::states::GameState;
use crate::systems::hud::UiFont;
use crate::systems::l10n;

/// Marker on the whole bottom bar root (hidden on GameOver/Win, UI9).
#[derive(Component)]
pub struct CommandBarRoot;

/// Marker on the center command-card zone (rebuilt on state changes).
#[derive(Component)]
pub struct CommandCardRoot;

/// Marker on the left info zone's dynamic text line.
#[derive(Component)]
pub struct InfoZoneText;

/// Marker on the right control zone (rebuilt only on language change).
#[derive(Component)]
pub struct ControlZoneRoot;

/// "2x" speed placeholder button (§16.1: greyed until its own card lands).
#[derive(Component)]
pub struct SpeedPlaceholderButton;

/// Pause button (same behavior as the P key).
#[derive(Component)]
pub struct PauseButton;

/// What one command-card button does. Handlers arrive with UI10 (Build/Buy)
/// and UI11 (Sell/Fuse); UI12 maps grid hotkeys onto the same actions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommandAction {
    /// Build tower type `usize` (armed flow, or directly on a selected slot).
    Build(usize),
    /// Shop offer: unlock tower type `usize` into the hand (AC2 semantics).
    Buy(usize),
    /// Sell the selected tower (TO7).
    Sell,
    /// Enter fusion target picking for the selected tower (UI11).
    Fuse,
    /// Leave fusion target picking.
    FuseCancel,
    /// Reserved grid slot (upgrades/skills later, §16.2) — always disabled.
    Reserved,
}

/// One command-card button: its action plus the enabled flag (combat greying).
#[derive(Component, Clone, Debug)]
pub struct CommandButton {
    pub action: CommandAction,
    pub enabled: bool,
}

/// Marker on the selected tower's ground range ring (selection-driven, UI9).
#[derive(Component)]
pub struct SelectionRing;

/// Spawn the persistent three-zone bottom bar (§16.1). Zone contents are
/// filled/refreshed by the per-frame systems below.
pub fn spawn_command_bar(mut commands: Commands, font: Res<UiFont>) {
    commands
        .spawn((
            CommandBarRoot,
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(0.0),
                right: Val::Px(0.0),
                bottom: Val::Px(0.0),
                height: Val::Px(120.0),
                flex_direction: FlexDirection::Row,
                column_gap: Val::Px(10.0),
                padding: UiRect::all(Val::Px(10.0)),
                ..default()
            },
            BackgroundColor(Color::srgb(0.08, 0.09, 0.12)),
        ))
        .with_children(|bar| {
            // Left: info zone (selected-object details / hints).
            bar.spawn(Node {
                flex_grow: 1.2,
                ..default()
            })
            .with_children(|zone| {
                zone.spawn((
                    InfoZoneText,
                    Text::new(""),
                    TextFont {
                        font: font.0.clone(),
                        font_size: FontSize::Px(13.0),
                        ..default()
                    },
                    TextColor(Color::srgb(0.85, 0.88, 0.92)),
                ));
            });
            // Center: command card (grid of buttons, content per state).
            bar.spawn((
                CommandCardRoot,
                Node {
                    flex_grow: 2.4,
                    flex_direction: FlexDirection::Row,
                    flex_wrap: FlexWrap::Wrap,
                    column_gap: Val::Px(6.0),
                    row_gap: Val::Px(6.0),
                    align_content: AlignContent::Center,
                    ..default()
                },
            ));
            // Right: control zone (filled by refresh_control_zone).
            bar.spawn((
                ControlZoneRoot,
                Node {
                    flex_grow: 1.0,
                    flex_direction: FlexDirection::Row,
                    column_gap: Val::Px(6.0),
                    justify_content: JustifyContent::FlexEnd,
                    align_items: AlignItems::Center,
                    ..default()
                },
            ));
        });
}

/// One command-card button. `enabled=false` renders greyed (combat lock).
fn command_button(
    parent: &mut ChildSpawnerCommands,
    font: &UiFont,
    action: CommandAction,
    label: String,
    enabled: bool,
) {
    parent
        .spawn((
            Button,
            CommandButton { action, enabled },
            Interaction::None,
            Node {
                width: Val::Px(104.0),
                height: Val::Px(42.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(if enabled {
                Color::srgb(0.2, 0.3, 0.4)
            } else {
                Color::srgb(0.16, 0.16, 0.18)
            }),
        ))
        .with_children(|p| {
            p.spawn((
                Text::new(label),
                TextFont {
                    font: font.0.clone(),
                    font_size: FontSize::Px(12.0),
                    ..default()
                },
                TextColor(if enabled {
                    Color::WHITE
                } else {
                    Color::srgb(0.45, 0.45, 0.45)
                }),
            ));
        });
}

/// Fill the right control zone (§16.1): start wave (moved from the old corner
/// bar) + greyed 2x placeholder + pause + language. Rebuilt on lang change.
pub fn refresh_control_zone(
    mut commands: Commands,
    font: Res<UiFont>,
    lang: Res<Lang>,
    mut built: Local<Option<Lang>>,
    bars: Query<Entity, With<CommandBarRoot>>,
    zones: Query<Entity, With<ControlZoneRoot>>,
) {
    if *built == Some(*lang) {
        return;
    }
    *built = Some(*lang);
    let Ok(bar) = bars.single() else {
        return;
    };
    for z in &zones {
        commands.entity(z).despawn();
    }
    commands.entity(bar).with_children(|zone| {
        zone.spawn((
            ControlZoneRoot,
            Node {
                flex_grow: 1.0,
                flex_direction: FlexDirection::Row,
                column_gap: Val::Px(6.0),
                justify_content: JustifyContent::FlexEnd,
                align_items: AlignItems::Center,
                ..default()
            },
        ))
        .with_children(|zone| {
            // 2x fast-forward: greyed placeholder until its own card (§16.1).
            zone.spawn((
                Button,
                SpeedPlaceholderButton,
                Interaction::None,
                Node {
                    width: Val::Px(44.0),
                    height: Val::Px(36.0),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    ..default()
                },
                BackgroundColor(Color::srgb(0.18, 0.18, 0.2)),
            ))
            .with_children(|p| {
                p.spawn((
                    Text::new(l10n::SPEED_2X.pick(*lang)),
                    TextFont {
                        font: font.0.clone(),
                        font_size: FontSize::Px(13.0),
                        ..default()
                    },
                    TextColor(Color::srgb(0.45, 0.45, 0.45)),
                ));
            });
            // Pause (mirrors the P key).
            zone.spawn((
                Button,
                PauseButton,
                Interaction::None,
                Node {
                    width: Val::Px(58.0),
                    height: Val::Px(36.0),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    ..default()
                },
                BackgroundColor(Color::srgb(0.25, 0.25, 0.32)),
            ))
            .with_children(|p| {
                p.spawn((
                    Text::new(l10n::PAUSE.pick(*lang)),
                    TextFont {
                        font: font.0.clone(),
                        font_size: FontSize::Px(12.0),
                        ..default()
                    },
                    TextColor(Color::WHITE),
                ));
            });
            // Language toggle (moved from the old top-right corner).
            zone.spawn((
                Button,
                crate::systems::pointer::LangToggleButton,
                Interaction::None,
                Node {
                    width: Val::Px(56.0),
                    height: Val::Px(36.0),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    ..default()
                },
                BackgroundColor(Color::srgb(0.25, 0.25, 0.35)),
            ))
            .with_children(|p| {
                p.spawn((
                    Text::new(l10n::lang_button_label(*lang)),
                    TextFont {
                        font: font.0.clone(),
                        font_size: FontSize::Px(12.0),
                        ..default()
                    },
                    TextColor(Color::srgb(0.85, 0.9, 1.0)),
                ));
            });
            // Start wave (moved from the old corner control bar, UI2).
            zone.spawn((
                Button,
                crate::systems::pointer::StartWaveButton,
                Interaction::None,
                Node {
                    width: Val::Px(140.0),
                    height: Val::Px(40.0),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    ..default()
                },
                BackgroundColor(Color::srgb(0.15, 0.5, 0.25)),
            ))
            .with_children(|p| {
                p.spawn((
                    Text::new(l10n::START_WAVE.pick(*lang)),
                    TextFont {
                        font: font.0.clone(),
                        font_size: FontSize::Px(15.0),
                        ..default()
                    },
                    TextColor(Color::WHITE),
                ));
            });
        });
        });
    *built = Some(*lang);
}

/// Cache key: rebuild the command card only when one of these changes.
type CardKey = (Selection, bool, Lang, usize, u32, u32);

/// Rebuild the center command card on selection / phase / lang / economy
/// changes (五态, §16.2). During combat every action button greys out but
/// stays visible (信息查看不受限).
#[allow(clippy::too_many_arguments)]
pub fn refresh_command_card(
    mut commands: Commands,
    font: Res<UiFont>,
    lang: Res<Lang>,
    selection: Res<Selection>,
    wave: Res<WaveState>,
    hand: Res<Hand>,
    offers: Res<ShopOffers>,
    economy: Res<Economy>,
    defs: Res<TowerDefs>,
    towers: Query<&Tower>,
    bars: Query<Entity, With<CommandBarRoot>>,
    cards: Query<Entity, With<CommandCardRoot>>,
    mut cache: Local<Option<CardKey>>,
) {
    let combat = wave.phase == WavePhase::Combat;
    // A selected tower that despawned (sold/fused elsewhere) reads as None.
    let selection = match *selection {
        Selection::Tower(e) if towers.get(e).is_err() => Selection::None,
        s => s,
    };
    let key = (
        selection,
        combat,
        *lang,
        hand.owned_towers.len(),
        offers.version,
        economy.gold,
    );
    if *cache == Some(key) {
        return;
    }
    *cache = Some(key);
    let Ok(bar) = bars.single() else {
        return;
    };
    for c in &cards {
        commands.entity(c).despawn();
    }
    commands.entity(bar).with_children(|card| {
        card.spawn((
            CommandCardRoot,
            Node {
                flex_grow: 2.4,
                flex_direction: FlexDirection::Row,
                flex_wrap: FlexWrap::Wrap,
                column_gap: Val::Px(6.0),
                row_gap: Val::Px(6.0),
                align_content: AlignContent::Center,
                ..default()
            },
        ))
        .with_children(|card| match selection {
        Selection::FusionTarget(_) => {
            card.spawn((
                Text::new(l10n::FUSE_TARGET_HINT.pick(*lang)),
                TextFont {
                    font: font.0.clone(),
                    font_size: FontSize::Px(13.0),
                    ..default()
                },
                TextColor(Color::srgb(1.0, 0.9, 0.5)),
            ));
            command_button(
                card,
                &font,
                CommandAction::FuseCancel,
                l10n::FUSE_CANCEL.pick(*lang).to_string(),
                true,
            );
        }
        Selection::Tower(e) => {
            let fused = towers
                .get(e)
                .map(|t| matches!(t.kind, TowerKind::Fused(_)))
                .unwrap_or(false);
            let can_act = !combat;
            command_button(
                card,
                &font,
                CommandAction::Sell,
                l10n::SELL.pick(*lang).to_string(),
                can_act, // wired in UI11+TO7
            );
            if !fused {
                // Fused towers cannot fuse again (§16.6 / UI11).
                command_button(
                    card,
                    &font,
                    CommandAction::Fuse,
                    l10n::FUSE.pick(*lang).to_string(),
                    can_act, // wired in UI11
                );
            }
            for _ in 0..2 {
                command_button(
                    card,
                    &font,
                    CommandAction::Reserved,
                    l10n::RESERVED.pick(*lang).to_string(),
                    false,
                );
            }
        }
        Selection::None | Selection::EmptySlot(_) => {
            // Build list: owned types (Build) + shop offers (Buy, AC2).
            // Actions are wired in UI10; combat greys everything (§16.2).
            let owned: Vec<usize> = hand
                .owned_towers
                .iter()
                .copied()
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter()
                .collect();
            for i in owned {
                let Some(def) = defs.list.get(i) else { continue };
                let afford = economy.gold >= def.cost;
                command_button(
                    card,
                    &font,
                    CommandAction::Build(i),
                    l10n::card_cost(*lang, l10n::tower_label(def, *lang), def.cost),
                    !combat && afford,
                );
            }
            for &i in &offers.offers {
                let Some(def) = defs.list.get(i) else { continue };
                let afford = economy.gold >= def.cost && !hand.owned_towers.contains(&i);
                command_button(
                    card,
                    &font,
                    CommandAction::Buy(i),
                    l10n::offer_cost(*lang, l10n::tower_label(def, *lang), def.cost),
                    !combat && afford,
                );
            }
        }
    });
    });
}

/// Rewrite the info zone from the selection (§16.1 left zone): hints when
/// idle, slot info, or the selected tower's full attributes. Also self-heals
/// a selection whose entity disappeared.
pub fn refresh_info_zone(
    mut q: Query<&mut Text, With<InfoZoneText>>,
    mut selection: ResMut<Selection>,
    towers: Query<&Tower>,
    defs: Res<TowerDefs>,
    fusion_defs: Res<FusionDefs>,
    lang: Res<Lang>,
) {
    if let Selection::Tower(e) = *selection {
        if towers.get(e).is_err() {
            *selection = Selection::None;
        }
    }
    let text = match *selection {
        Selection::None => l10n::INFO_NONE.pick(*lang).to_string(),
        Selection::EmptySlot(_) => l10n::INFO_SLOT.pick(*lang).to_string(),
        Selection::FusionTarget(_) => l10n::FUSE_TARGET_HINT.pick(*lang).to_string(),
        Selection::Tower(e) => match towers.get(e) {
            Ok(tower) => {
            let kind_label = match tower.kind {
                TowerKind::Base(i) => defs
                    .list
                    .get(i)
                    .map(|d| l10n::tower_label(d, *lang))
                    .unwrap_or(l10n::BASE_TOWER_FALLBACK.pick(*lang)),
                TowerKind::Fused(k) => fusion_defs
                    .list
                    .iter()
                    .find(|r| r.kind == k)
                    .map(|r| l10n::fusion_label(r, *lang))
                    .unwrap_or(l10n::FUSED_TOWER_FALLBACK.pick(*lang)),
            };
            let mut block = format!(
                "{} · {}\n{}",
                kind_label,
                l10n::attack_label(tower.attack_type, *lang),
                l10n::tower_stats(*lang, tower.damage, tower.attack_speed, tower.range),
            );
            if let TowerKind::Fused(k) = tower.kind {
                block.push('\n');
                block.push_str(&l10n::fusion_trait(k, *lang));
            }
            block
        }
        Err(_) => l10n::INFO_NONE.pick(*lang).to_string(),
        },
    };
    for mut t in &mut q {
        if t.0 != text {
            t.0 = text.clone();
        }
    }
}

/// Left click selects (§16.3): tower first (threshold), then the nearest
/// empty slot, blank ground cancels. No-op while a build is armed (that
/// click is a placement click). Runs in Playing only, both phases — viewing
/// attributes during combat is unrestricted (§16.2).
#[allow(clippy::type_complexity)]
pub fn select_object(
    buttons: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window>,
    camera: Query<(&Camera, &GlobalTransform)>,
    build: Res<BuildMode>,
    state: Res<State<GameState>>,
    mut selection: ResMut<Selection>,
    towers: Query<(Entity, &Transform), With<Tower>>,
    slots: Query<(Entity, &TowerSlot, &Transform)>,
) {
    if build.armed || !buttons.just_pressed(MouseButton::Left) || *state.get() != GameState::Playing
    {
        return;
    }
    let Some((cam, cam_tf)) = camera.single().ok() else {
        return;
    };
    let Some(cursor) = windows.single().ok().and_then(|w| w.cursor_position()) else {
        return;
    };
    // 1) A tower under the cursor?
    let mut best_tower: Option<(Entity, f32)> = None;
    for (eid, ttf) in towers.iter() {
        if let Ok(sp) = cam.world_to_viewport(cam_tf, ttf.translation) {
            let d = sp.distance(cursor);
            if d <= 45.0 && best_tower.map_or(true, |(_, bd)| d < bd) {
                best_tower = Some((eid, d));
            }
        }
    }
    *selection = if let Some((e, _)) = best_tower {
        Selection::Tower(e)
    } else {
        // 2) The nearest slot (screen-space)?
        let mut best_slot: Option<(Entity, f32, bool)> = None;
        for (eid, slot, stf) in slots.iter() {
            if let Ok(sp) = cam.world_to_viewport(cam_tf, stf.translation) {
                let d = sp.distance(cursor);
                if d <= 45.0 && best_slot.map_or(true, |(_, bd, _)| d < bd) {
                    best_slot = Some((eid, d, slot.occupied));
                }
            }
        }
        match best_slot {
            Some((e, _, false)) => Selection::EmptySlot(e),
            Some((e, _, true)) => {
                // Occupied slot: select the tower standing on it (nearest).
                let Ok((_, _, stf)) = slots.get(e) else {
                    return;
                };
                let mut tower_here: Option<(Entity, f32)> = None;
                for (eid, ttf) in towers.iter() {
                    let d = ttf.translation.distance(stf.translation);
                    if tower_here.map_or(true, |(_, bd)| d < bd) {
                        tower_here = Some((eid, d));
                    }
                }
                tower_here.map(|(e, _)| Selection::Tower(e)).unwrap_or(Selection::None)
            }
            None => Selection::None, // 3) blank ground cancels (§16.3)
        }
    };
}

/// Unified cancel (§16.3): right click or Esc clears the armed build, the
/// fusion picking and the selection itself. Not state-gated (harmless).
pub fn cancel_modes(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut build: ResMut<BuildMode>,
    mut selection: ResMut<Selection>,
    mut fusion_sel: ResMut<FusionSel>,
) {
    if mouse.just_pressed(MouseButton::Right) || keys.just_pressed(KeyCode::Escape) {
        build.armed = false;
        *selection = Selection::None;
        fusion_sel.a = None; // legacy F-key flow until UI11 retires it
        info!("[ui9] cancelled modes/selection");
    }
}

/// Ground range ring for the selected tower (moved from the retired UI2
/// right-top panel; the ring is selection-driven now).
pub fn refresh_selection_ring(
    mut commands: Commands,
    selection: Res<Selection>,
    towers: Query<(&Tower, &Transform)>,
    rings: Query<Entity, With<SelectionRing>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut last: Local<Selection>,
) {
    let target = match *selection {
        Selection::Tower(e) => towers.get(e).ok().map(|(t, tf)| (t.range, tf.translation)),
        _ => None,
    };
    if *last == *selection && target.is_some() {
        return;
    }
    *last = *selection;
    for r in &rings {
        commands.entity(r).despawn();
    }
    if let Some((range, pos)) = target {
        commands.spawn((
            SelectionRing,
            Mesh3d(meshes.add(Circle::new(range))),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: Color::srgba(0.3, 0.8, 1.0, 0.22),
                unlit: true,
                alpha_mode: AlphaMode::Blend,
                ..default()
            })),
            Transform::from_xyz(pos.x, 0.05, pos.z)
                .with_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)),
        ));
    }
}

/// Clicking the pause button mirrors the P key (shares the state logic).
pub fn handle_pause_button(
    q: Query<&Interaction, (Changed<Interaction>, With<PauseButton>)>,
    state: Res<State<GameState>>,
    mut next: ResMut<NextState<GameState>>,
) {
    for inter in &q {
        if *inter == Interaction::Pressed {
            crate::plugins::game::apply_pause(&state, &mut next);
        }
    }
}

/// Show the start-wave button only during the build window (Intermission, no
/// pending choice, Playing) — logic moved from the retired UI2 control bar.
pub fn refresh_start_wave_button(
    wave: Res<WaveState>,
    choice: Res<crate::resources::WaveChoice>,
    state: Res<State<GameState>>,
    mut buttons: Query<&mut Node, With<crate::systems::pointer::StartWaveButton>>,
) {
    let show = wave.phase == WavePhase::Intermission
        && !choice.pending
        && *state.get() == GameState::Playing;
    for mut node in &mut buttons {
        node.display = if show { Display::Flex } else { Display::None };
    }
}

/// Hide the whole bottom bar on terminal states; the end screen is the only
/// surface there (§16.1 / UI9 acceptance).
pub fn refresh_bar_visibility(
    state: Res<State<GameState>>,
    mut roots: Query<&mut Node, With<CommandBarRoot>>,
) {
    let show = !matches!(state.get(), GameState::GameOver | GameState::Win);
    for mut node in &mut roots {
        node.display = if show { Display::Flex } else { Display::None };
    }
}
