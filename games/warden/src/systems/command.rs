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
    BuildMode, Economy, FusionDefs, Hand, Lang, Selection, ShopOffers, SELL_REFUND_RATIO,
    TowerDefs, WavePhase, WaveState,
};
use crate::states::GameState;
use crate::systems::hud::UiFont;
use crate::systems::l10n;
use crate::systems::skills::SkillButton;

/// Marker on the whole bottom bar root (hidden on GameOver/Win, UI9).
#[derive(Component)]
pub struct CommandBarRoot;

/// Marker on UI roots that must swallow world clicks (§16.3): while the
/// cursor is over one (bottom bar / choice cards / end screen), world
/// picking and armed placement stay idle — the click that presses a command
/// button belongs to the UI and must not also run `select_object` (whose
/// blank-ground cancel used to kill the fusion entry armed by that very
/// click, UI11). Hidden roots (Display::None) collapse in layout, so they
/// never report `cursor_over`.
#[derive(Component)]
pub struct BlocksWorldInput;

/// True while the cursor hovers any `BlocksWorldInput` root, or while the
/// F1 debug panel (debug builds) is under the pointer (DB1): world picking
/// and armed placement stay idle then. The bevy_ui flags are maintained by
/// `ui_focus_system` (DefaultPlugins build); headless tests set the
/// component by hand; the panel flag is written by the egui panel itself.
pub fn cursor_over_blocking_ui(
    roots: &Query<&bevy::ui::RelativeCursorPosition, With<BlocksWorldInput>>,
    panel: &crate::resources::DebugPanelCapture,
) -> bool {
    panel.pointer || roots.iter().any(|r| r.cursor_over())
}

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

/// One command-card button: its action, the enabled flag (combat greying)
/// and its grid slot (§16.7: row-major Q W E R / A S D F).
#[derive(Component, Clone, Debug)]
pub struct CommandButton {
    pub action: CommandAction,
    pub enabled: bool,
    pub grid: u8,
}

/// The grid hotkey letters in row-major order (§16.7).
pub const GRID_KEYS: [KeyCode; 8] = [
    KeyCode::KeyQ,
    KeyCode::KeyW,
    KeyCode::KeyE,
    KeyCode::KeyR,
    KeyCode::KeyA,
    KeyCode::KeyS,
    KeyCode::KeyD,
    KeyCode::KeyF,
];

/// Badge letter for a grid slot (printed on the button corner, §16.7).
pub fn grid_badge(grid: u8) -> char {
    "QWERASDF".as_bytes()[grid as usize % 8] as char
}

/// Marker on the selected tower's ground range ring (selection-driven, UI9).
#[derive(Component)]
pub struct SelectionRing;

/// Marker on the build-preview range ring shown while hovering a Build
/// button with an empty slot selected (UI10, §16.4).
#[derive(Component)]
pub struct PreviewRing;

/// Marker on the ground discs highlighting pairable fusion targets while the
/// selection is `FusionTarget` (UI11, §16.6).
#[derive(Component)]
pub struct PairHighlight;

/// Spawn the persistent three-zone bottom bar (§16.1). Zone contents are
/// filled/refreshed by the per-frame systems below.
pub fn spawn_command_bar(mut commands: Commands, font: Res<UiFont>) {
    commands
        .spawn((
            CommandBarRoot,
            BlocksWorldInput,
            bevy::ui::RelativeCursorPosition::default(),
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
    grid: u8,
) {
    parent
        .spawn((
            Button,
            CommandButton { action, enabled, grid },
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
            // Hotkey corner badge (§16.7).
            p.spawn((
                Text::new(grid_badge(grid).to_string()),
                TextFont {
                    font: font.0.clone(),
                    font_size: FontSize::Px(9.0),
                    ..default()
                },
                TextColor(Color::srgb(0.6, 0.8, 1.0)),
                Node {
                    position_type: PositionType::Absolute,
                    top: Val::Px(2.0),
                    right: Val::Px(4.0),
                    ..default()
                },
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
    fusion_defs: Res<FusionDefs>,
    towers: Query<(Entity, &Tower)>,
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
                0,
            );
        }
        Selection::Tower(e) => {
            let fused = towers
                .get(e)
                .map(|(_, t)| matches!(t.kind, TowerKind::Fused(_)))
                .unwrap_or(false);
            let can_act = !combat;
            command_button(
                card,
                &font,
                CommandAction::Sell,
                l10n::SELL.pick(*lang).to_string(),
                can_act,
                0,
            );
            if !fused {
                // Fused towers cannot fuse again (§16.6 / UI11). The Fuse
                // command greys out while no *other* tower pairs with it.
                let pairable = match towers.get(e).ok().map(|(_, t)| t.kind) {
                    Some(TowerKind::Base(ka)) => towers.iter().any(|(eid2, t)| {
                        eid2 != e
                            && matches!(t.kind, TowerKind::Base(kb) if recipe_matches(&fusion_defs, ka, kb).is_some())
                    }),
                    _ => false,
                };
                command_button(
                    card,
                    &font,
                    CommandAction::Fuse,
                    l10n::FUSE.pick(*lang).to_string(),
                    can_act && pairable,
                    1,
                );
            }
            for grid in 2..4u8 {
                command_button(
                    card,
                    &font,
                    CommandAction::Reserved,
                    l10n::RESERVED.pick(*lang).to_string(),
                    false,
                    grid,
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
            let mut grid: u8 = 0;
            for i in owned {
                let Some(def) = defs.list.get(i) else { continue };
                let afford = economy.gold >= def.cost;
                command_button(
                    card,
                    &font,
                    CommandAction::Build(i),
                    l10n::card_cost(*lang, l10n::tower_label(def, *lang), def.cost),
                    !combat && afford,
                    grid,
                );
                grid = grid.saturating_add(1);
            }
            for &i in &offers.offers {
                let Some(def) = defs.list.get(i) else { continue };
                let afford = economy.gold >= def.cost && !hand.owned_towers.contains(&i);
                if grid >= GRID_KEYS.len() as u8 {
                    break; // the 2x4 grid is full
                }
                command_button(
                    card,
                    &font,
                    CommandAction::Buy(i),
                    l10n::offer_cost(*lang, l10n::tower_label(def, *lang), def.cost),
                    !combat && afford,
                    grid,
                );
                grid = grid.saturating_add(1);
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
    towers: Query<(Entity, &Tower)>,
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
        Selection::FusionTarget(a) => {
            // §16.6: show the recipe (+fee) the picking would produce.
            let ka = towers
                .get(a)
                .ok()
                .and_then(|(_, t)| match t.kind {
                    TowerKind::Base(i) => Some(i),
                    _ => None,
                })
                .unwrap_or(usize::MAX);
            let pair = towers.iter().find_map(|(eid, t)| match t.kind {
                TowerKind::Base(kb) if eid != a => {
                    recipe_matches(&fusion_defs, ka, kb).map(|r| (r, kb))
                }
                _ => None,
            });
            match pair {
                Some((recipe, _)) => {
                    let cost_ing =
                        defs.list[recipe.ingredients[0]].cost + defs.list[recipe.ingredients[1]].cost;
                    let fee = (cost_ing as f32 * recipe.fee_ratio).round() as u32;
                    format!("{}\n{}", l10n::FUSE_TARGET_HINT.pick(*lang), l10n::fusion_preview(*lang, l10n::fusion_label(recipe, *lang), fee))
                }
                None => l10n::FUSE_NONE.pick(*lang).to_string(),
            }
        }
        Selection::Tower(e) => match towers.get(e) {
            Ok((_, tower)) => {
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

/// Intent resource: the tower clicked while fusion picking is active (UI11).
/// `select_object` posts it; `apply_pending_fusion` (chained right after)
/// executes the fusion or falls back to plain re-selection.
#[derive(Resource)]
pub struct PendingFusionConfirm(pub Entity);

/// Left click selects (§16.3): tower first (threshold), then the nearest
/// empty slot, blank ground cancels. During fusion target picking a tower
/// click posts `PendingFusionConfirm`; the chained `apply_pending_fusion`
/// confirms the fusion or falls back to plain selection (no accidental
/// fusing, §16.6). While a point-skill target pick is active (SK1) the skill
/// owns the world click — this system stays idle for it.
#[allow(clippy::type_complexity)]
pub fn select_object(
    mut commands: Commands,
    buttons: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window>,
    camera: Query<(&Camera, &GlobalTransform)>,
    blocking: Query<&bevy::ui::RelativeCursorPosition, With<BlocksWorldInput>>,
    panel: Res<crate::resources::DebugPanelCapture>,
    build: Res<BuildMode>,
    targeting: Res<crate::resources::SkillTargeting>,
    state: Res<State<GameState>>,
    mut selection: ResMut<Selection>,
    towers: Query<(Entity, &Tower, &Transform)>,
    slots: Query<(Entity, &TowerSlot, &Transform)>,
) {
    if build.armed
        || targeting.0.is_some()
        || !buttons.just_pressed(MouseButton::Left)
        || *state.get() != GameState::Playing
    {
        return;
    }
    if cursor_over_blocking_ui(&blocking, &panel) {
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
    for (eid, _, ttf) in towers.iter() {
        if let Ok(sp) = cam.world_to_viewport(cam_tf, ttf.translation) {
            let d = sp.distance(cursor);
            if d <= 45.0 && best_tower.map_or(true, |(_, bd)| d < bd) {
                best_tower = Some((eid, d));
            }
        }
    }
    if let Some((e, _)) = best_tower {
        if matches!(*selection, Selection::FusionTarget(_)) {
            // Defer to the chained applier (keeps this system lean).
            commands.insert_resource(PendingFusionConfirm(e));
        } else {
            *selection = Selection::Tower(e);
        }
        return;
    }
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
    *selection = match best_slot {
        Some((e, _, false)) => Selection::EmptySlot(e),
        Some((e, _, true)) => {
            // Occupied slot: select the tower standing on it (nearest).
            let Ok((_, _, stf)) = slots.get(e) else {
                return;
            };
            let mut tower_here: Option<(Entity, f32)> = None;
            for (eid, _, ttf) in towers.iter() {
                let d = ttf.translation.distance(stf.translation);
                if tower_here.map_or(true, |(_, bd)| d < bd) {
                    tower_here = Some((eid, d));
                }
            }
            tower_here.map(|(e, _)| Selection::Tower(e)).unwrap_or(Selection::None)
        }
        None => Selection::None, // 3) blank ground cancels (§16.3)
    };
}

/// Executes a posted fusion confirm (UI11 step 3, §16.6): fuses the picked
/// source with the clicked target when they pair; otherwise simply selects
/// the clicked tower. Chained right after `select_object`.
#[allow(clippy::too_many_arguments)]
pub fn apply_pending_fusion(
    mut commands: Commands,
    pending: Option<Res<PendingFusionConfirm>>,
    mut selection: ResMut<Selection>,
    fusion_defs: Res<FusionDefs>,
    defs: Res<TowerDefs>,
    towers: Query<(Entity, &Tower, &Transform)>,
    mut economy: ResMut<Economy>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut slots: Query<(Entity, &mut TowerSlot, &Transform)>,
) {
    let Some(pending) = pending else {
        return;
    };
    let b = pending.0;
    commands.remove_resource::<PendingFusionConfirm>();
    if confirm_fusion_target(
        &mut selection,
        b,
        &fusion_defs,
        &defs,
        &towers,
        &mut economy,
        &mut commands,
        &mut meshes,
        &mut materials,
        &mut slots,
    )
    .is_none()
    {
        // Not a recipe (or unaffordable): fall back to plain selection.
        if towers.get(b).is_ok() {
            *selection = Selection::Tower(b);
        }
    }
}

/// Unified cancel (§16.3): right click or Esc clears the armed build, the
/// fusion picking, the point-skill target picking (SK1 — a cancelled cast
/// never pays its cooldown) and the selection itself. Not state-gated
/// (harmless).
pub fn cancel_modes(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut build: ResMut<BuildMode>,
    mut selection: ResMut<Selection>,
    mut skill_targeting: ResMut<crate::resources::SkillTargeting>,
) {
    if mouse.just_pressed(MouseButton::Right) || keys.just_pressed(KeyCode::Escape) {
        build.armed = false;
        *selection = Selection::None;
        if skill_targeting.0.take().is_some() {
            info!("[sk] target picking cancelled (no cooldown)");
        }
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

// --- UI10: build command migration + dual entry (§16.4) ----------------------

/// Command-card button actions (UI10/UI11): the two build entries (§16.4),
/// AC2 shop purchase, TO7 sell and the fusion step-in (§16.6). Greyed buttons
/// (enabled=false) never act.
#[allow(clippy::too_many_arguments)]
pub fn handle_command_buttons(
    mut commands: Commands,
    server: Option<Res<AssetServer>>,
    mut q: Query<(&Interaction, &CommandButton), Changed<Interaction>>,
    mut selection: ResMut<Selection>,
    mut selected: ResMut<crate::resources::SelectedTower>,
    mut build: ResMut<BuildMode>,
    defs: Res<TowerDefs>,
    mut hand: ResMut<Hand>,
    mut economy: ResMut<Economy>,
    wave: Res<WaveState>,
    state: Res<State<GameState>>,
    towers: Query<(&Tower, &Transform)>,
    mut slots: Query<(Entity, &mut TowerSlot, &Transform)>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let intermission = wave.phase == WavePhase::Intermission && *state.get() == GameState::Playing;
    for (inter, btn) in &mut q {
        if *inter != Interaction::Pressed || !btn.enabled {
            continue;
        }
        match btn.action {
            CommandAction::Build(i) => {
                if !intermission {
                    continue;
                }
                let Some(def) = defs.list.get(i) else { continue };
                if !hand.owned_towers.contains(&i) || economy.gold < def.cost {
                    continue;
                }
                match *selection {
                    // Second entry (§16.4): an empty slot is selected, so the
                    // button builds directly on that slot.
                    Selection::EmptySlot(slot_e) => {
                        if let Ok((_, mut slot, stf)) = slots.get_mut(slot_e) {
                            if slot.occupied {
                                continue;
                            }
                            slot.occupied = true;
                            economy.gold -= def.cost;
                            let e = crate::systems::tower::spawn_base_tower(
                                &mut commands,
                                def,
                                i,
                                stf.translation,
                                &mut meshes,
                                &mut materials,
                                server.as_deref(),
                            );
                            *selection = Selection::Tower(e);
                            info!("[ui10] built {} on selected slot, gold={}", def.name, economy.gold);
                        }
                    }
                    // First entry (§16.4): arm build mode, then click a slot
                    // (UI1 mouse flow preserved).
                    _ => {
                        selected.tower_index = i;
                        build.armed = true;
                        info!("[ui10] build armed for tower {i}");
                    }
                }
            }
            CommandAction::Buy(i) => {
                // AC2 semantics unchanged: pay once, unlock into the hand.
                if !intermission || hand.owned_towers.contains(&i) {
                    continue;
                }
                let Some(def) = defs.list.get(i) else { continue };
                if economy.gold < def.cost {
                    continue;
                }
                economy.gold -= def.cost;
                hand.owned_towers.push(i);
                info!("[ui10] bought {} for {} (hand {:?})", def.name, def.cost, hand.owned_towers);
            }
            CommandAction::Sell => {
                // TO7 (§16.5): intermission only; refund = invested × 70%.
                if !intermission {
                    continue;
                }
                let Selection::Tower(e) = *selection else { continue };
                let Ok((tower, tf)) = towers.get(e) else { continue };
                let refund = (tower.invested as f32 * SELL_REFUND_RATIO).floor() as u32;
                economy.gold += refund;
                if let Some((slot_e, _)) = nearest_slot_world(&slots, tf.translation) {
                    if let Ok((_, mut slot, _)) = slots.get_mut(slot_e) {
                        slot.occupied = false;
                    }
                }
                commands.entity(e).despawn();
                *selection = Selection::None;
                info!("[to7] sold tower (invested {} -> refund {}), gold={}", tower.invested, refund, economy.gold);
            }
            CommandAction::Fuse => {
                // UI11 step 2 (§16.6): enter fusion target picking.
                if !intermission {
                    continue;
                }
                if let Selection::Tower(e) = *selection {
                    *selection = Selection::FusionTarget(e);
                    info!("[ui11] fusion target picking for {e:?}");
                }
            }
            CommandAction::FuseCancel => {
                *selection = Selection::None;
            }
            CommandAction::Reserved => {}
        }
    }
}

/// Nearest slot (world-space) to a position — used to free/occupy the slot a
/// tower stands on.
fn nearest_slot_world(
    slots: &Query<(Entity, &mut TowerSlot, &Transform)>,
    pos: Vec3,
) -> Option<(Entity, Vec3)> {
    let mut best: Option<(Entity, f32, Vec3)> = None;
    for (eid, _slot, stf) in slots.iter() {
        let d = stf.translation.distance(pos);
        if best.map_or(true, |(_, bd, _)| d < bd) {
            best = Some((eid, d, stf.translation));
        }
    }
    best.map(|(e, _, p)| (e, p))
}

/// Does a recipe match the (order-free) pair of base kinds {ka, kb}?
fn recipe_matches(
    fusion_defs: &FusionDefs,
    ka: usize,
    kb: usize,
) -> Option<&crate::resources::FusionDef> {
    fusion_defs.list.iter().find(|r| {
        let mut need = r.ingredients.to_vec();
        for k in [ka, kb] {
            if let Some(i) = need.iter().position(|&x| x == k) {
                need.remove(i);
            }
        }
        need.is_empty()
    })
}

/// UI11 step 3 (§16.6): confirm the fusion on target tower `b` while `a` is
/// in `FusionTarget` picking. Deducts the 20% fee, despawns both, frees both
/// slots and lands the result on B's slot (A's slot frees — net one slot
/// gained, unchanged). Returns the result entity, or None when the pair is
/// not a recipe / unaffordable (caller then simply re-selects `b`).
#[allow(clippy::too_many_arguments)]
pub fn confirm_fusion_target(
    selection: &mut Selection,
    b: Entity,
    fusion_defs: &FusionDefs,
    defs: &TowerDefs,
    towers: &Query<(Entity, &Tower, &Transform)>,
    economy: &mut Economy,
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    slots: &mut Query<(Entity, &mut TowerSlot, &Transform)>,
) -> Option<Entity> {
    let a = match *selection {
        Selection::FusionTarget(a) => a,
        _ => return None,
    };
    let (ka, pos_a) = towers.get(a).ok().and_then(|(_, t, tf)| match t.kind {
        TowerKind::Base(i) => Some((i, tf.translation)),
        _ => None,
    })?;
    let (kb, pos_b) = towers.get(b).ok().and_then(|(_, t, tf)| match t.kind {
        TowerKind::Base(i) => Some((i, tf.translation)),
        _ => None,
    })?;
    let recipe = recipe_matches(fusion_defs, ka, kb)?;
    let cost_ing = defs.list[recipe.ingredients[0]].cost + defs.list[recipe.ingredients[1]].cost;
    let fee = (cost_ing as f32 * recipe.fee_ratio).round() as u32;
    if economy.gold < fee {
        info!("[ui11] fusion fee {fee} unaffordable",);
        return None;
    }
    economy.gold -= fee;

    commands.entity(a).despawn();
    commands.entity(b).despawn();
    // Free A's slot, then land the result on B's slot (§16.6 落位规则).
    if let Some((sa, _)) = nearest_slot_world(slots, pos_a) {
        if let Ok((_, mut slot, _)) = slots.get_mut(sa) {
            slot.occupied = false;
        }
    }
    let result_slot = nearest_slot_world(slots, pos_b)?;
    if let Ok((_, mut slot, _)) = slots.get_mut(result_slot.0) {
        slot.occupied = true;
    }
    let result = commands
        .spawn((
            Tower {
                tower_index: recipe.ingredients[0],
                damage: recipe.damage,
                attack_speed: recipe.attack_speed,
                range: recipe.range,
                attack_type: recipe.attack_type,
                cooldown: 0.0,
                target: None,
                kind: TowerKind::Fused(recipe.kind),
                aoe_radius: recipe.aoe_radius,
                slow_factor: recipe.slow_factor,
                slow_duration: recipe.slow_duration,
                invested: cost_ing + fee,
            },
            Mesh3d(meshes.add(Cuboid::new(1.2, 1.2, 1.2))),
            MeshMaterial3d(materials.add(match recipe.kind {
                crate::components::FusionKind::Marksman => Color::srgb(0.2, 0.8, 1.0),
                crate::components::FusionKind::Archmage => Color::srgb(0.7, 0.2, 1.0),
                crate::components::FusionKind::Hybrid => Color::srgb(0.2, 0.9, 0.4),
                crate::components::FusionKind::Bastion => Color::srgb(1.0, 0.6, 0.1),
            })),
            Transform::from_xyz(pos_b.x, 0.6, pos_b.z),
        ))
        .id();
    *selection = Selection::Tower(result);
    info!("[ui11] fused {} (fee {fee}), result on B slot", recipe.name);
    Some(result)
}

/// Highlight every tower that pairs with the picked fusion source (§16.6):
/// flat discs under the candidates, rebuilt only when the candidate set
/// changes (towers may appear/vanish while picking).
pub fn refresh_fusion_candidates(
    mut commands: Commands,
    selection: Res<Selection>,
    fusion_defs: Res<FusionDefs>,
    towers: Query<(Entity, &Tower, &Transform)>,
    highlights: Query<Entity, With<PairHighlight>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut cache: Local<Vec<Entity>>, // highlighted tower entities
) {
    // Candidate towers that pair with the picked source (sorted for compare).
    let candidates: Vec<Entity> = match *selection {
        Selection::FusionTarget(a) => {
            let ka = towers
                .get(a)
                .ok()
                .and_then(|(_, t, _)| match t.kind {
                    TowerKind::Base(i) => Some(i),
                    _ => None,
                })
                .unwrap_or(usize::MAX);
            let mut v: Vec<Entity> = towers
                .iter()
                .filter(|&(eid, t, _)| {
                    eid != a
                        && matches!(t.kind, TowerKind::Base(_))
                        && recipe_matches(
                            &fusion_defs,
                            ka,
                            match t.kind {
                                TowerKind::Base(i) => i,
                                _ => usize::MAX,
                            },
                        )
                        .is_some()
                })
                .map(|(eid, _, _)| eid)
                .collect();
            v.sort_unstable_by_key(|e| e.to_bits());
            v
        }
        _ => Vec::new(),
    };
    if candidates == *cache {
        return;
    }
    *cache = candidates.clone();
    for h in &highlights {
        commands.entity(h).despawn();
    }
    for eid in candidates {
        let Ok((_, _, tf)) = towers.get(eid) else { continue };
        commands.spawn((
            PairHighlight,
            Mesh3d(meshes.add(Circle::new(0.8))),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: Color::srgba(1.0, 0.85, 0.2, 0.55),
                unlit: true,
                alpha_mode: AlphaMode::Blend,
                ..default()
            })),
            Transform::from_xyz(tf.translation.x, 0.08, tf.translation.z)
                .with_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)),
        ));
    }
}

/// Hover preview (§16.4 second entry): while an empty slot is selected,
/// hovering a Build button shows the ghost tower + that type's range ring
/// on the slot. The preview follows the hovered type live.
#[allow(clippy::too_many_arguments)]
pub fn hover_build_preview(
    mut commands: Commands,
    buttons: Query<(&Interaction, &CommandButton)>,
    selection: Res<Selection>,
    defs: Res<TowerDefs>,
    wave: Res<WaveState>,
    state: Res<State<GameState>>,
    slots: Query<(&TowerSlot, &Transform)>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut cache: Local<Option<(usize, Entity, Entity)>>, // (type, ghost, ring)
) {
    let active = wave.phase == WavePhase::Intermission && *state.get() == GameState::Playing;
    let hovered_type = buttons
        .iter()
        .find(|(inter, btn)| {
            **inter == Interaction::Hovered && matches!(btn.action, CommandAction::Build(_))
        })
        .and_then(|(_, btn)| match btn.action {
            CommandAction::Build(i) => Some(i),
            _ => None,
        });
    let target = match (*selection, hovered_type) {
        (Selection::EmptySlot(e), Some(i)) if active => slots
            .get(e)
            .ok()
            .filter(|(slot, _)| !slot.occupied)
            .map(|(_, t)| (i, t.translation)),
        _ => None,
    };
    match target {
        None => {
            if let Some((_, ghost, ring)) = *cache {
                commands.entity(ghost).insert(Visibility::Hidden);
                commands.entity(ring).insert(Visibility::Hidden);
            }
        }
        Some((i, pos)) => {
            if cache.map_or(true, |(t, _, _)| t != i) {
                // (Re)spawn the ghost + ring for the newly hovered type.
                if let Some((_, ghost, ring)) = *cache {
                    commands.entity(ghost).despawn();
                    commands.entity(ring).despawn();
                }
                let Some(def) = defs.list.get(i) else { return };
                let (r, g, b) = match def.attack_type {
                    crate::components::AttackType::Physical => (0.7, 0.7, 0.8),
                    crate::components::AttackType::Magic => (0.5, 0.4, 0.9),
                    crate::components::AttackType::Mixed => (0.6, 0.9, 0.6),
                };
                let ghost = commands
                    .spawn((
                        crate::systems::pointer::HoverGhost,
                        Mesh3d(meshes.add(Cuboid::new(1.0, 1.0, 1.0))),
                        MeshMaterial3d(materials.add(StandardMaterial {
                            base_color: Color::srgba(r, g, b, 0.35),
                            unlit: true,
                            alpha_mode: AlphaMode::Blend,
                            ..default()
                        })),
                        Visibility::Visible,
                        Transform::from_xyz(pos.x, 0.6, pos.z),
                    ))
                    .id();
                let ring = commands
                    .spawn((
                        PreviewRing,
                        Mesh3d(meshes.add(Circle::new(def.range))),
                        MeshMaterial3d(materials.add(StandardMaterial {
                            base_color: Color::srgba(0.4, 0.9, 0.5, 0.25),
                            unlit: true,
                            alpha_mode: AlphaMode::Blend,
                            ..default()
                        })),
                        Visibility::Visible,
                        Transform::from_xyz(pos.x, 0.05, pos.z)
                            .with_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)),
                    ))
                    .id();
                *cache = Some((i, ghost, ring));
            } else if let Some((_, ghost, ring)) = *cache {
                commands.entity(ghost).insert(Visibility::Visible);
                commands.entity(ring).insert(Visibility::Visible);
            }
        }
    }
}

// --- UI12: grid hotkeys + tooltip (§16.7/§16.8) -------------------------------

/// Grid hotkeys (§16.7): QWER/ASDF press the button at that grid slot by
/// injecting `Interaction::Pressed` — the regular click handler then runs the
/// exact same path, so hotkeys and clicks can never diverge. Chained before
/// `handle_command_buttons`.
pub fn command_grid_hotkeys(
    keys: Res<ButtonInput<KeyCode>>,
    panel: Res<crate::resources::DebugPanelCapture>,
    buttons: Query<(Entity, &CommandButton)>,
    mut commands: Commands,
) {
    // Typing into a debug-panel text field must not fire world hotkeys (DB1).
    if panel.keyboard {
        return;
    }
    for (i, key) in GRID_KEYS.iter().enumerate() {
        if !keys.just_pressed(*key) {
            continue;
        }
        // The button at this grid slot (unique within the card).
        if let Some((e, _)) = buttons
            .iter()
            .find(|(_, b)| b.grid as usize == i && b.enabled)
        {
            commands.entity(e).insert(Interaction::Pressed);
        }
    }
}

/// Marker on the tooltip panel text (UI12).
#[derive(Component)]
pub struct TooltipText;

/// Which button a tooltip is currently shown for (command card or skill).
#[derive(Clone, Copy, PartialEq)]
pub enum TooltipSource {
    Command(CommandAction),
    Skill(crate::resources::SkillId),
}

/// Tooltip (§16.8): hovering any command-card button shows the full info —
/// name / hotkey / price (or refund/fee) / attributes / one-line role. Skill
/// buttons in the control zone share the panel (SK1, §17.3): name / hotkey /
/// CD / effect numbers. The panel sits above the bottom bar; content rebuilds
/// on hover change.
#[allow(clippy::too_many_arguments)]
pub fn refresh_tooltip(
    mut commands: Commands,
    font: Res<UiFont>,
    lang: Res<Lang>,
    buttons: Query<(&Interaction, &CommandButton)>,
    skill_buttons: Query<(&Interaction, &SkillButton)>,
    selection: Res<Selection>,
    towers: Query<&Tower>,
    defs: Res<TowerDefs>,
    panels: Query<Entity, With<TooltipText>>,
    mut cache: Local<Option<(TooltipSource, Lang)>>,
) {
    let hovered = buttons
        .iter()
        .find(|(inter, _)| **inter == Interaction::Hovered)
        .map(|(_, b)| TooltipSource::Command(b.action))
        .or_else(|| {
            skill_buttons
                .iter()
                .find(|(inter, _)| **inter == Interaction::Hovered)
                .map(|(_, b)| TooltipSource::Skill(b.skill))
        });
    if *cache == hovered.map(|s| (s, *lang)) && hovered.is_some() {
        return;
    }
    *cache = hovered.map(|s| (s, *lang));
    for p in &panels {
        commands.entity(p).despawn();
    }
    let Some(source) = hovered else {
        return;
    };
    let skill_line = |source: TooltipSource| -> Option<String> {
        match source {
            TooltipSource::Skill(id) => {
                let def = crate::systems::skills::skill_def(id);
                Some(format!(
                    "{}: {}\n{}",
                    l10n::TT_HOTKEY.pick(*lang),
                    def.badge,
                    l10n::skill_tooltip(*lang, id, crate::resources::SKILL_COOLDOWNS[id.index()])
                ))
            }
            _ => None,
        }
    };
    if let Some(body) = skill_line(source) {
        let title = match source {
            TooltipSource::Skill(id) => l10n::skill_name(id, *lang).to_string(),
            _ => unreachable!("skill_line only builds for skills"),
        };
        let mut text = title;
        text.push_str("\n");
        text.push_str(&body);
        spawn_tooltip(&mut commands, &font, text);
        return;
    }
    let TooltipSource::Command(action) = source else {
        return;
    };
    let (title, body) = match action {
        CommandAction::Build(i) | CommandAction::Buy(i) => {
            let Some(def) = defs.list.get(i) else {
                return;
            };
            (
                l10n::tower_label(def, *lang).to_string(),
                format!(
                    "{}: {} · {}: {}{}\n{} · {}\n{}",
                    l10n::TT_HOTKEY.pick(*lang),
                    grid_badge(buttons.iter().find(|(_, b)| b.action == action).map(|(_, b)| b.grid).unwrap_or(0)),
                    l10n::TT_COST.pick(*lang),
                    def.cost,
                    if matches!(action, CommandAction::Buy(_)) { "" } else { "" },
                    l10n::tower_stats(*lang, def.damage, def.attack_speed, def.range),
                    l10n::attack_label(def.attack_type, *lang),
                    l10n::tower_role(def, *lang),
                ),
            )
        }
        CommandAction::Sell => {
            let refund = match *selection {
                Selection::Tower(e) => towers
                    .get(e)
                    .map(|t| (t.invested as f32 * crate::resources::SELL_REFUND_RATIO).floor() as u32)
                    .unwrap_or(0),
                _ => 0,
            };
            (
                l10n::SELL.pick(*lang).to_string(),
                format!("{}: {}{}", l10n::TT_SELL_FOR.pick(*lang), refund, if *lang == Lang::Zh { "金" } else { "g" }),
            )
        }
        CommandAction::Fuse => (
            l10n::FUSE.pick(*lang).to_string(),
            l10n::TT_FUSE_DESC.pick(*lang).to_string(),
        ),
        CommandAction::FuseCancel => (l10n::FUSE_CANCEL.pick(*lang).to_string(), String::new()),
        CommandAction::Reserved => (
            l10n::RESERVED.pick(*lang).to_string(),
            l10n::TT_RESERVED.pick(*lang).to_string(),
        ),
    };
    let mut text = title;
    if !body.is_empty() {
        text.push_str("\n");
        text.push_str(&body);
    }
    spawn_tooltip(&mut commands, &font, text);
}

/// Spawn the shared tooltip panel (one text node above the bottom bar).
fn spawn_tooltip(commands: &mut Commands, font: &UiFont, text: String) {
    commands.spawn((
        TooltipText,
        Text::new(text),
        TextFont {
            font: font.0.clone(),
            font_size: FontSize::Px(12.0),
            ..default()
        },
        TextColor(Color::WHITE),
        Node {
            position_type: PositionType::Absolute,
            bottom: Val::Px(128.0),
            left: Val::Px(240.0),
            width: Val::Px(360.0),
            padding: UiRect::all(Val::Px(8.0)),
            ..default()
        },
        BackgroundColor(Color::srgba(0.05, 0.06, 0.1, 0.92)),
    ));
}
