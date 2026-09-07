//! Mouse-first interaction (UI1 + pointer place / point-select fusion, UI2 panel).
//!   - A bottom shop bar (bevy_ui) shows the owned tower types (build mode) and
//!     the current shop offers (buy to unlock, AC2); it rebuilds automatically
//!     whenever the hand, the offer, or gold changes; unaffordable cards grey out.
//!   - A control bar (bottom right) hosts the "开始下一波" start-wave button (UI2).
//!   - Pending three-choose-one options appear as clickable cards (UI2, = keys 1/2/3).
//!   - Left click on a slot places the armed tower (Intermission only).
//!   - Left click on a tower selects it: info panel + range ring appear (UI2);
//!     clicking a second tower fuses them if they match a recipe.
//!   - Right click cancels build mode / clears fusion selection.
//! All player-facing copy is Chinese (UI2); code identifiers and logs stay English.
//! Keyboard hotkeys (1-4 / E / F / Space) remain as fallback.
//!
//! Split into small systems: Bevy caps a single system at 16 params.

use std::collections::BTreeSet;

use bevy::input::mouse::MouseButton;
use bevy::prelude::*;
use bevy::window::Window;

use crate::components::{AttackType, FusionKind, ShopButton, Tower, TowerKind, TowerSlot};
use crate::resources::{
    Boosts, BuildMode, Economy, FusionDefs, FusionSel, Hand, MetaSavePath, MetaState, SelectedTower,
    ShopOffers, TowerDefs, UPGRADE1_COST, UPGRADE2_COST, WaveChoice, WavePhase, WaveSchedule,
    WaveState,
};
use crate::states::GameState;
use crate::systems::{hud::ui_font, input::{apply_choice, try_start_wave}, meta::buy_upgrade};

/// Marker on the shop bar root so it can be despawned on rebuild (Bevy 0.19
/// despawn removes children recursively).
#[derive(Component)]
pub struct ShopBarRoot;

/// A bevy_ui shop offer button: buys (unlocks) that base tower type (AC2).
#[derive(Component)]
pub struct OfferButton {
    pub tower_index: usize,
}

/// Marker on the start-wave control bar root (UI2).
#[derive(Component)]
pub struct ControlBarRoot;

/// "开始下一波" button (UI2): same effect as the Space key.
#[derive(Component)]
pub struct StartWaveButton;

/// Marker on the three-choose-one card panel root (UI2).
#[derive(Component)]
pub struct ChoiceCardsRoot;

/// One clickable three-choose-one option card (UI2).
#[derive(Component)]
pub struct ChoiceCardButton {
    pub index: usize,
}

/// Marker on the selected-tower info panel (UI2).
#[derive(Component)]
pub struct TowerInfoPanel;

/// Marker on the selected tower's ground range ring (UI2).
#[derive(Component)]
pub struct TowerRangeRing;

/// Marker on the build-mode ghost preview at the hovered slot (UI3).
#[derive(Component)]
pub struct HoverGhost;

/// Marker on the GameOver/Win result screen root (UI5).
#[derive(Component)]
pub struct EndScreenRoot;

/// "再来一局" button on the result screen (UI5 = the P/retry key).
#[derive(Component)]
pub struct RetryButton;

/// A meta-upgrade purchase button on the result screen (UI5 = the 1/2 keys).
#[derive(Component)]
pub struct MetaUpgradeButton {
    pub which: usize, // 1 or 2
}

/// Spawn the bottom shop bar: owned types row (UI1) + offers row (AC2).
/// Chinese labels (UI2); a card greys out when gold cannot afford its cost.
fn build_shop_bar(commands: &mut Commands, defs: &TowerDefs, offers: &ShopOffers, hand: &Hand, economy: &Economy) {
    // Dedupe for display: AC3 drops may push a type that is already owned.
    let owned: Vec<usize> = hand
        .owned_towers
        .iter()
        .copied()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    commands
        .spawn((
            ShopBarRoot,
            Node {
                position_type: PositionType::Absolute,
                bottom: Val::Px(12.0),
                left: Val::Px(12.0),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(6.0),
                ..default()
            },
        ))
        .with_children(|root| {
            // Row 1: owned types — click arms build mode (UI1).
            root.spawn(Node {
                flex_direction: FlexDirection::Row,
                column_gap: Val::Px(8.0),
                ..default()
            })
            .with_children(|row| {
                for i in owned {
                    let Some(def) = defs.list.get(i) else {
                        continue;
                    };
                    let affordable = economy.gold >= def.cost;
                    row.spawn((
                        Button,
                        ShopButton { tower_index: i },
                        Interaction::None,
                        Node {
                            width: Val::Px(96.0),
                            height: Val::Px(48.0),
                            justify_content: JustifyContent::Center,
                            align_items: AlignItems::Center,
                            ..default()
                        },
                        BackgroundColor(Color::srgb(0.2, 0.3, 0.4)),
                    ))
                    .with_children(|p| {
                        p.spawn((
                            Text::new(format!("{} {}金", def.label, def.cost)),
                            TextFont {
                                font: ui_font(),
                                font_size: FontSize::Px(13.0),
                                ..default()
                            },
                            TextColor(card_color(affordable, Color::WHITE)),
                        ));
                    });
                }
            });
            // Row 2: shop offers — click buys (unlocks) the type (AC2).
            root.spawn(Node {
                flex_direction: FlexDirection::Row,
                column_gap: Val::Px(8.0),
                ..default()
            })
            .with_children(|row| {
                for &i in &offers.offers {
                    let Some(def) = defs.list.get(i) else {
                        continue;
                    };
                    let affordable = economy.gold >= def.cost;
                    row.spawn((
                        Button,
                        OfferButton { tower_index: i },
                        Interaction::None,
                        Node {
                            width: Val::Px(96.0),
                            height: Val::Px(40.0),
                            justify_content: JustifyContent::Center,
                            align_items: AlignItems::Center,
                            ..default()
                        },
                        BackgroundColor(Color::srgb(0.45, 0.35, 0.15)),
                    ))
                    .with_children(|p| {
                        p.spawn((
                            Text::new(format!("[商店] {} {}金", def.label, def.cost)),
                            TextFont {
                                font: ui_font(),
                                font_size: FontSize::Px(12.0),
                                ..default()
                            },
                            TextColor(card_color(affordable, Color::srgb(1.0, 0.9, 0.6))),
                        ));
                    });
                }
            });
        });
}

/// Normal color when affordable, dim grey otherwise (UI2 affordability cue).
fn card_color(affordable: bool, normal: Color) -> Color {
    if affordable {
        normal
    } else {
        Color::srgb(0.45, 0.45, 0.45)
    }
}

/// Rebuild the shop bar whenever the hand, the offer, or gold changes (AC1/2/3
/// feed it; gold changes refresh the affordability grey-out). Cached key avoids
/// per-frame work.
pub fn refresh_shop_bar(
    mut commands: Commands,
    defs: Res<TowerDefs>,
    offers: Res<ShopOffers>,
    hand: Res<Hand>,
    economy: Res<Economy>,
    mut cache: Local<(u32, usize, u32)>,
    roots: Query<Entity, With<ShopBarRoot>>,
) {
    let key = (offers.version, hand.owned_towers.len(), economy.gold);
    if *cache == key {
        return;
    }
    *cache = key;
    for root in &roots {
        commands.entity(root).despawn();
    }
    build_shop_bar(&mut commands, &defs, &offers, &hand, &economy);
}

/// A pressed shop button arms build mode with that tower type.
pub fn handle_shop_buttons(
    mut q: Query<(&Interaction, &ShopButton), Changed<Interaction>>,
    mut selected: ResMut<SelectedTower>,
    mut build: ResMut<BuildMode>,
) {
    for (inter, btn) in &mut q {
        if *inter == Interaction::Pressed {
            selected.tower_index = btn.tower_index;
            build.armed = true;
            info!("[mouse] build mode armed for tower {}", btn.tower_index);
        }
    }
}

/// AC2: clicking an offer buys (unlocks) that base tower type. The purchase
/// pays the tower cost once; each later placement of the unlocked type still
/// pays per build (TO2 rule unchanged).
pub fn handle_offer_buttons(
    mut q: Query<(&Interaction, &OfferButton), Changed<Interaction>>,
    defs: Res<TowerDefs>,
    mut economy: ResMut<Economy>,
    mut hand: ResMut<Hand>,
) {
    for (inter, btn) in &mut q {
        if *inter != Interaction::Pressed {
            continue;
        }
        if hand.owned_towers.contains(&btn.tower_index) {
            info!("[mouse] offer {}: already owned", btn.tower_index);
            continue;
        }
        let Some(def) = defs.list.get(btn.tower_index) else {
            continue;
        };
        if economy.gold < def.cost {
            info!(
                "[mouse] offer {}: not enough gold ({} < {})",
                def.name, economy.gold, def.cost
            );
            continue;
        }
        economy.gold -= def.cost;
        hand.owned_towers.push(btn.tower_index);
        info!(
            "[mouse] bought {} for {} (hand {:?})",
            def.name, def.cost, hand.owned_towers
        );
    }
}

/// Spawn the bottom-right control bar with the "开始下一波" button (UI2).
fn build_control_bar(commands: &mut Commands) {
    commands
        .spawn((
            ControlBarRoot,
            Node {
                position_type: PositionType::Absolute,
                bottom: Val::Px(12.0),
                right: Val::Px(12.0),
                ..default()
            },
        ))
        .with_children(|root| {
            root.spawn((
                Button,
                StartWaveButton,
                Interaction::None,
                Node {
                    width: Val::Px(150.0),
                    height: Val::Px(48.0),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    ..default()
                },
                BackgroundColor(Color::srgb(0.15, 0.5, 0.25)),
            ))
            .with_children(|p| {
                p.spawn((
                    Text::new("▶ 开始下一波"),
                    TextFont {
                        font: ui_font(),
                        font_size: FontSize::Px(15.0),
                        ..default()
                    },
                    TextColor(Color::WHITE),
                ));
            });
        });
}

/// Spawn the control bar once, then toggle the start button's visibility:
/// shown during the build window only (Intermission, no pending choice).
pub fn refresh_control_bar(
    mut commands: Commands,
    wave: Res<WaveState>,
    choice: Res<WaveChoice>,
    state: Res<State<GameState>>,
    mut spawned: Local<bool>,
    mut buttons: Query<&mut Node, With<StartWaveButton>>,
) {
    if !*spawned {
        *spawned = true;
        build_control_bar(&mut commands);
        return;
    }
    let show = wave.phase == WavePhase::Intermission
        && !choice.pending
        && *state.get() == GameState::Playing;
    for mut node in &mut buttons {
        node.display = if show { Display::Flex } else { Display::None };
    }
}

/// Clicking "开始下一波" starts the next wave — identical to the Space key (UI2).
pub fn handle_start_wave_button(
    mut q: Query<(&Interaction, &StartWaveButton), Changed<Interaction>>,
    mut wave: ResMut<WaveState>,
    schedule: Res<WaveSchedule>,
    choice: Res<WaveChoice>,
    state: Res<State<GameState>>,
) {
    for (inter, _) in &mut q {
        if *inter == Interaction::Pressed {
            try_start_wave(&mut wave, &schedule, &choice, &state);
        }
    }
}

/// Show/hide the three-choose-one option cards with `WaveChoice::pending` (UI2).
/// Cards are clickable equivalents of the 1/2/3 keys.
pub fn refresh_choice_cards(
    mut commands: Commands,
    choice: Res<WaveChoice>,
    mut cache: Local<bool>,
    roots: Query<Entity, With<ChoiceCardsRoot>>,
) {
    if *cache == choice.pending {
        return;
    }
    *cache = choice.pending;
    for root in &roots {
        commands.entity(root).despawn();
    }
    if !choice.pending {
        return;
    }
    commands
        .spawn((
            ChoiceCardsRoot,
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(110.0),
                left: Val::Px(0.0),
                right: Val::Px(0.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                column_gap: Val::Px(12.0),
                ..default()
            },
        ))
        .with_children(|root| {
            for (i, opt) in choice.options.iter().enumerate() {
                root.spawn((
                    Button,
                    ChoiceCardButton { index: i },
                    Interaction::None,
                    Node {
                        width: Val::Px(230.0),
                        height: Val::Px(64.0),
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        ..default()
                    },
                    BackgroundColor(Color::srgb(0.25, 0.22, 0.45)),
                ))
                .with_children(|p| {
                    p.spawn((
                        Text::new(format!("[{}] {}", i + 1, opt.label)),
                        TextFont {
                            font: ui_font(),
                            font_size: FontSize::Px(14.0),
                            ..default()
                        },
                        TextColor(Color::WHITE),
                    ));
                });
            }
        });
    info!("[ui2] choice cards shown: {}", choice.options.len());
}

/// Clicking a choice card applies that option — identical to pressing its digit.
pub fn handle_choice_card_buttons(
    mut q: Query<(&Interaction, &ChoiceCardButton), Changed<Interaction>>,
    mut choice: ResMut<WaveChoice>,
    mut boosts: ResMut<Boosts>,
    mut hand: ResMut<Hand>,
) {
    for (inter, btn) in &mut q {
        if *inter == Interaction::Pressed {
            apply_choice(&mut choice, &mut boosts, &mut hand, btn.index);
        }
    }
}

/// Rebuild the tower info panel + ground range ring whenever the fusion
/// selection changes (UI2): select a tower to inspect it, deselect to close.
pub fn refresh_tower_info(
    mut commands: Commands,
    fusion_sel: Res<FusionSel>,
    towers: Query<(Entity, &Tower, &Transform)>,
    defs: Res<TowerDefs>,
    fusion_defs: Res<FusionDefs>,
    panels: Query<Entity, With<TowerInfoPanel>>,
    rings: Query<Entity, With<TowerRangeRing>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    if !fusion_sel.is_changed() {
        return;
    }
    for p in &panels {
        commands.entity(p).despawn();
    }
    for r in &rings {
        commands.entity(r).despawn();
    }
    let Some(sel) = fusion_sel.a else {
        return;
    };
    let Ok((_, tower, tf)) = towers.get(sel) else {
        return;
    };
    let kind_label = match tower.kind {
        TowerKind::Base(i) => defs
            .list
            .get(i)
            .map(|d| d.label)
            .unwrap_or("基础塔"),
        TowerKind::Fused(k) => fusion_defs
            .list
            .iter()
            .find(|r| r.kind == k)
            .map(|r| r.label)
            .unwrap_or("融合塔"),
    };
    let damage_label = match tower.attack_type {
        AttackType::Physical => "物理",
        AttackType::Magic => "魔法",
        AttackType::Mixed => "混合·无视护甲",
    };
    let mut lines = format!(
        "{} · {}\n伤害 {:.0} · 攻速 {:.1}/秒 · 射程 {:.1}",
        kind_label, damage_label, tower.damage, tower.attack_speed, tower.range
    );
    lines.push_str(&match tower.kind {
        TowerKind::Fused(FusionKind::Archmage) => "\n特技：AOE 爆炸（半径 3.0）".to_string(),
        TowerKind::Fused(FusionKind::Bastion) => "\n特技：AOE + 减速30%（1秒）".to_string(),
        TowerKind::Fused(FusionKind::Marksman) => "\n特技：优先攻击血量最高的怪".to_string(),
        TowerKind::Fused(FusionKind::Hybrid) => "\n特技：混合伤害穿透物理护甲".to_string(),
        TowerKind::Base(_) => String::new(),
    });
    lines.push_str("\n点另一座塔=尝试融合 · 右键=取消");
    commands
        .spawn((
            TowerInfoPanel,
            Node {
                position_type: PositionType::Absolute,
                right: Val::Px(12.0),
                top: Val::Px(60.0),
                width: Val::Px(240.0),
                padding: UiRect::all(Val::Px(8.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.65)),
        ))
        .with_children(|p| {
            p.spawn((
                Text::new(lines),
                TextFont {
                    font: ui_font(),
                    font_size: FontSize::Px(13.0),
                    ..default()
                },
                TextColor(Color::WHITE),
            ));
        });
    // Ground ring showing the tower's range (flat, semi-transparent).
    commands.spawn((
        TowerRangeRing,
        Mesh3d(meshes.add(Circle::new(tower.range))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgba(0.3, 0.8, 1.0, 0.22),
            unlit: true,
            alpha_mode: AlphaMode::Blend,
            ..default()
        })),
        Transform::from_xyz(tf.translation.x, 0.05, tf.translation.z)
            .with_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)),
    ));
    info!("[ui2] tower info panel shown for {}", kind_label);
}

/// Build-mode ghost preview (UI3): while a build is armed, show a translucent
/// copy of the armed tower on the slot under the cursor, so the player sees
/// exactly where and what will be placed. Hidden otherwise (or on an occupied
/// slot). A single entity is reused; it is respawned only when the armed tower
/// type changes (so the colour stays in sync).
pub fn update_hover_ghost(
    mut commands: Commands,
    windows: Query<&Window>,
    camera: Query<(&Camera, &GlobalTransform)>,
    build: Res<BuildMode>,
    selected: Res<SelectedTower>,
    defs: Res<TowerDefs>,
    wave: Res<WaveState>,
    state: Res<State<GameState>>,
    slots: Query<(Entity, &mut TowerSlot, &Transform)>,
    mut cache: Local<(usize, Option<Entity>)>, // (last tower type, ghost entity)
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let armed = build.armed
        && wave.phase == WavePhase::Intermission
        && *state.get() == GameState::Playing;

    // Pick the slot under the cursor when armed and the slot is free.
    let mut target: Option<Vec3> = None;
    if armed {
        let cam = camera.single().ok();
        let cursor = windows.single().ok().and_then(|w| w.cursor_position());
        if let (Some((cam, cam_tf)), Some(cursor)) = (cam, cursor) {
            if let Some((slot_e, pos)) = nearest_slot_screen(&slots, cursor, cam, cam_tf) {
                if let Ok((_, slot, _)) = slots.get(slot_e) {
                    if !slot.occupied {
                        target = Some(pos);
                    }
                }
            }
        }
    }

    // When not armed / no free slot: hide the ghost, if any.
    if target.is_none() {
        if let Some(e) = cache.1 {
            commands.entity(e).insert(Visibility::Hidden);
        }
        return;
    }
    let pos = target.unwrap();

    // Respawn when the armed tower type changed (colour must follow it).
    if cache.0 != selected.tower_index || cache.1.is_none() {
        if let Some(e) = cache.1 {
            commands.entity(e).despawn();
        }
        let def = defs.list.get(selected.tower_index);
        let (r, g, b) = match def.map(|d| d.attack_type) {
            Some(AttackType::Physical) => (0.7, 0.7, 0.8),
            Some(AttackType::Magic) => (0.5, 0.4, 0.9),
            Some(AttackType::Mixed) => (0.6, 0.9, 0.6),
            None => (0.8, 0.8, 0.8),
        };
        let mesh = meshes.add(Cuboid::new(1.0, 1.0, 1.0));
        let mat = materials.add(StandardMaterial {
            base_color: Color::srgba(r, g, b, 0.35),
            unlit: true,
            alpha_mode: AlphaMode::Blend,
            ..default()
        });
        let e = commands
            .spawn((
                HoverGhost,
                Mesh3d(mesh),
                MeshMaterial3d(mat),
                Visibility::Visible,
                Transform::from_xyz(pos.x, 0.6, pos.z),
            ))
            .id();
        *cache = (selected.tower_index, Some(e));
    } else if let Some(e) = cache.1 {
        commands
            .entity(e)
            .insert(Visibility::Visible)
            .insert(Transform::from_xyz(pos.x, 0.6, pos.z));
    }
}

/// Build the GameOver/Win result screen: a centered banner + the retry button
/// and the two meta-upgrade purchase buttons (UI5). Starts hidden; the
/// `refresh_end_screen` system toggles its visibility with the game state.
fn build_end_screen(commands: &mut Commands) {
    commands
        .spawn((
            EndScreenRoot,
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(0.0),
                right: Val::Px(0.0),
                top: Val::Px(36.0),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                row_gap: Val::Px(10.0),
                display: Display::None,
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.55)),
        ))
        .with_children(|root| {
            root.spawn((
                Text::new("🏁 本局结束"),
                TextFont {
                    font: ui_font(),
                    font_size: FontSize::Px(34.0),
                    ..default()
                },
                TextColor(Color::srgb(1.0, 0.85, 0.4)),
            ));
            root.spawn((
                Text::new("按 1/2 购买 Meta 升级，或点「再来一局」"),
                TextFont {
                    font: ui_font(),
                    font_size: FontSize::Px(14.0),
                    ..default()
                },
                TextColor(Color::WHITE),
            ));
            root.spawn(Node {
                flex_direction: FlexDirection::Row,
                column_gap: Val::Px(10.0),
                ..default()
            })
            .with_children(|row| {
                for (which, label) in [
                    (1usize, format!("升级1 开局必含弓箭手塔（{}币）", UPGRADE1_COST)),
                    (2usize, format!("升级2 开局+20金（{}币）", UPGRADE2_COST)),
                ] {
                    row.spawn((
                        Button,
                        MetaUpgradeButton { which },
                        Interaction::None,
                        Node {
                            width: Val::Px(210.0),
                            height: Val::Px(44.0),
                            justify_content: JustifyContent::Center,
                            align_items: AlignItems::Center,
                            ..default()
                        },
                        BackgroundColor(Color::srgb(0.2, 0.35, 0.5)),
                    ))
                    .with_children(|p| {
                        p.spawn((
                            Text::new(label),
                            TextFont {
                                font: ui_font(),
                                font_size: FontSize::Px(13.0),
                                ..default()
                            },
                            TextColor(Color::WHITE),
                        ));
                    });
                }
                row.spawn((
                    Button,
                    RetryButton,
                    Interaction::None,
                    Node {
                        width: Val::Px(130.0),
                        height: Val::Px(44.0),
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        ..default()
                    },
                    BackgroundColor(Color::srgb(0.15, 0.5, 0.25)),
                ))
                .with_children(|p| {
                    p.spawn((
                        Text::new("再来一局"),
                        TextFont {
                            font: ui_font(),
                            font_size: FontSize::Px(15.0),
                            ..default()
                        },
                        TextColor(Color::WHITE),
                    ));
                });
            });
        });
}

/// Show the result screen only on GameOver/Win, hide it otherwise (UI5). The
/// panel is built once and its visibility toggled with the game state.
pub fn refresh_end_screen(
    mut commands: Commands,
    state: Res<State<GameState>>,
    mut spawned: Local<bool>,
    mut roots: Query<&mut Node, With<EndScreenRoot>>,
) {
    if !*spawned {
        *spawned = true;
        build_end_screen(&mut commands);
        return;
    }
    let show = matches!(state.get(), GameState::GameOver | GameState::Win);
    for mut node in &mut roots {
        node.display = if show { Display::Flex } else { Display::None };
    }
}

/// Clicking "再来一局" restarts from a terminal state (UI5 = the P/retry key).
pub fn handle_retry_button(
    mut q: Query<(&Interaction, &RetryButton), Changed<Interaction>>,
    state: Res<State<GameState>>,
    mut next: ResMut<NextState<GameState>>,
) {
    for (inter, _) in &mut q {
        if *inter != Interaction::Pressed {
            continue;
        }
        if matches!(state.get(), GameState::GameOver | GameState::Win) {
            next.set(GameState::Playing);
            info!("[ui5] retry -> Playing");
        }
    }
}

/// Clicking a meta-upgrade button buys that upgrade (UI5 = the 1/2 keys).
pub fn handle_meta_upgrade_buttons(
    mut q: Query<(&Interaction, &MetaUpgradeButton), Changed<Interaction>>,
    state: Res<State<GameState>>,
    mut meta: ResMut<MetaState>,
    path: Res<MetaSavePath>,
) {
    if !matches!(state.get(), GameState::GameOver | GameState::Win) {
        return;
    }
    for (inter, btn) in &mut q {
        if *inter != Interaction::Pressed {
            continue;
        }
        buy_upgrade(&mut meta, &path, btn.which);
    }
}

/// Right click cancels build mode and clears the fusion selection.
pub fn mouse_cancel(
    buttons: Res<ButtonInput<MouseButton>>,
    mut build: ResMut<BuildMode>,
    mut fusion_sel: ResMut<FusionSel>,
) {
    if buttons.just_pressed(MouseButton::Right) {
        build.armed = false;
        fusion_sel.a = None;
        info!("[mouse] cleared build/fusion selection");
    }
}

/// Left click in build mode places the armed tower at the nearest slot.
pub fn mouse_place_system(
    buttons: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window>,
    camera: Query<(&Camera, &GlobalTransform)>,
    selected: Res<SelectedTower>,
    defs: Res<TowerDefs>,
    wave: Res<WaveState>,
    hand: Res<Hand>,
    build: Res<BuildMode>,
    mut economy: ResMut<Economy>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut slots: Query<(Entity, &mut TowerSlot, &Transform)>,
) {
    if !build.armed || !buttons.just_pressed(MouseButton::Left) || wave.phase != WavePhase::Intermission
    {
        return;
    }
    let Some((cam_comp, cam_tf)) = camera.single().ok() else {
        return;
    };
    let Some(cursor) = windows.single().ok().and_then(|w| w.cursor_position()) else {
        return;
    };
    let Some((slot_e, _)) = nearest_slot_screen(&slots, cursor, cam_comp, cam_tf) else {
        return;
    };
    let Some(def) = defs.list.get(selected.tower_index) else {
        return;
    };
    if !hand.owned_towers.contains(&selected.tower_index) || economy.gold < def.cost {
        return;
    }
    if let Ok((_, mut slot, stf)) = slots.get_mut(slot_e) {
        if slot.occupied {
            return;
        }
        slot.occupied = true;
        economy.gold -= def.cost;
        commands.spawn((
            Tower {
                tower_index: selected.tower_index,
                damage: def.damage,
                attack_speed: def.attack_speed,
                range: def.range,
                attack_type: def.attack_type,
                cooldown: 0.0,
                target: None,
                kind: TowerKind::Base(selected.tower_index),
                aoe_radius: 0.0,
                slow_factor: 0.0,
                slow_duration: 0.0,
            },
            Mesh3d(meshes.add(Cuboid::new(1.0, 1.0, 1.0))),
            MeshMaterial3d(materials.add(match def.attack_type {
                AttackType::Physical => Color::srgb(0.7, 0.7, 0.8),
                AttackType::Magic => Color::srgb(0.5, 0.4, 0.9),
                AttackType::Mixed => Color::srgb(0.6, 0.9, 0.6),
            })),
            Transform::from_xyz(stf.translation.x, 0.6, stf.translation.z),
        ));
        info!("[mouse] placed {} at slot, gold={}", def.name, economy.gold);
    }
}

/// Left click outside build mode selects a tower, or fuses the two selected.
pub fn mouse_fuse_system(
    buttons: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window>,
    camera: Query<(&Camera, &GlobalTransform)>,
    wave: Res<WaveState>,
    build: Res<BuildMode>,
    mut fusion_sel: ResMut<FusionSel>,
    fusion_defs: Res<FusionDefs>,
    defs: Res<TowerDefs>,
    mut economy: ResMut<Economy>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut slots: Query<(Entity, &mut TowerSlot, &Transform)>,
    towers: Query<(Entity, &Tower, &Transform)>,
) {
    if build.armed || !buttons.just_pressed(MouseButton::Left) || wave.phase != WavePhase::Intermission
    {
        return;
    }
    let Some((cam_comp, cam_tf)) = camera.single().ok() else {
        return;
    };
    let Some(cursor) = windows.single().ok().and_then(|w| w.cursor_position()) else {
        return;
    };
    let Some(tower_e) = tower_at_screen(&towers, cursor, cam_comp, cam_tf) else {
        return;
    };
    match fusion_sel.a {
        None => {
            fusion_sel.a = Some(tower_e);
            info!("[mouse] selected tower for fusion");
        }
        Some(prev) if prev == tower_e => {
            fusion_sel.a = None;
            info!("[mouse] deselected tower");
        }
        Some(prev) => {
            if let Some(name) = try_fuse_two(
                prev,
                tower_e,
                &*fusion_defs,
                &*defs,
                &towers,
                &mut *economy,
                &mut commands,
                &mut *meshes,
                &mut *materials,
                &mut slots,
            ) {
                fusion_sel.a = None;
                info!("[mouse] fused -> {name}");
            } else {
                fusion_sel.a = Some(tower_e);
                info!("[mouse] pair not a recipe, reselected");
            }
        }
    }
}

/// Nearest slot to the screen-space cursor position.
fn nearest_slot_screen(
    slots: &Query<(Entity, &mut TowerSlot, &Transform)>,
    cursor: Vec2,
    cam: &Camera,
    cam_tf: &GlobalTransform,
) -> Option<(Entity, Vec3)> {
    let mut best: Option<(Entity, f32, Vec3)> = None;
    for (eid, _slot, stf) in slots.iter() {
        let Ok(sp) = cam.world_to_viewport(cam_tf, stf.translation) else {
            continue;
        };
        let d = sp.distance(cursor);
        if best.map_or(true, |(_, bd, _)| d < bd) {
            best = Some((eid, d, stf.translation));
        }
    }
    best.map(|(e, _, p)| (e, p))
}

/// The tower whose screen position is nearest the cursor (within a threshold).
fn tower_at_screen(
    towers: &Query<(Entity, &Tower, &Transform)>,
    cursor: Vec2,
    cam: &Camera,
    cam_tf: &GlobalTransform,
) -> Option<Entity> {
    let threshold = 45.0;
    let mut best: Option<(Entity, f32)> = None;
    for (eid, _tower, ttf) in towers.iter() {
        let Ok(sp) = cam.world_to_viewport(cam_tf, ttf.translation) else {
            continue;
        };
        let d = sp.distance(cursor);
        if d <= threshold && best.map_or(true, |(_, bd)| d < bd) {
            best = Some((eid, d));
        }
    }
    best.map(|(e, _)| e)
}

/// Fuse two specific towers if they match a recipe. Deducts the 20% fee, frees
/// both ingredient slots and occupies one for the result. Returns the recipe name.
fn try_fuse_two(
    a: Entity,
    b: Entity,
    fusion_defs: &FusionDefs,
    defs: &TowerDefs,
    towers: &Query<(Entity, &Tower, &Transform)>,
    economy: &mut Economy,
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    slots: &mut Query<(Entity, &mut TowerSlot, &Transform)>,
) -> Option<&'static str> {
    let (kind_a, pos_a) = base_kind_pos(towers, a)?;
    let (kind_b, pos_b) = base_kind_pos(towers, b)?;
    let recipe = fusion_defs.list.iter().find(|r| {
        let mut need = r.ingredients.to_vec();
        for k in [kind_a, kind_b] {
            if let Some(i) = need.iter().position(|&x| x == k) {
                need.remove(i);
            }
        }
        need.is_empty()
    })?;
    let cost_ing = defs.list[recipe.ingredients[0]].cost + defs.list[recipe.ingredients[1]].cost;
    let fee = (cost_ing as f32 * recipe.fee_ratio).round() as u32;
    if economy.gold < fee {
        return None;
    }
    economy.gold -= fee;

    for eid in [a, b] {
        commands.entity(eid).despawn();
    }
    // Free both ingredient slots, then occupy one for the result.
    if let Some((sa, _)) = nearest_slot_world(slots, pos_a) {
        set_occupied(slots, sa, false);
    }
    if let Some((sb, _)) = nearest_slot_world(slots, pos_b) {
        set_occupied(slots, sb, false);
    }
    let (result_slot, result_pos) = nearest_slot_world(slots, pos_a)?;
    set_occupied(slots, result_slot, true);

    commands.spawn((
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
        },
        Mesh3d(meshes.add(Cuboid::new(1.2, 1.2, 1.2))),
        MeshMaterial3d(materials.add(match recipe.kind {
            FusionKind::Marksman => Color::srgb(0.2, 0.8, 1.0),
            FusionKind::Archmage => Color::srgb(0.7, 0.2, 1.0),
            FusionKind::Hybrid => Color::srgb(0.2, 0.9, 0.4),
            FusionKind::Bastion => Color::srgb(1.0, 0.6, 0.1),
        })),
        Transform::from_xyz(result_pos.x, 0.6, result_pos.z),
    ));
    Some(recipe.name)
}

/// (base_kind, world_pos) for a Base tower, or None for a fused tower.
fn base_kind_pos(
    towers: &Query<(Entity, &Tower, &Transform)>,
    eid: Entity,
) -> Option<(usize, Vec3)> {
    towers.get(eid).ok().and_then(|(_, t, tf)| match t.kind {
        TowerKind::Base(i) => Some((i, tf.translation)),
        _ => None,
    })
}

fn nearest_slot_world(
    slots: &mut Query<(Entity, &mut TowerSlot, &Transform)>,
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

fn set_occupied(
    slots: &mut Query<(Entity, &mut TowerSlot, &Transform)>,
    ent: Entity,
    occupied: bool,
) {
    if let Ok((_, mut slot, _)) = slots.get_mut(ent) {
        slot.occupied = occupied;
    }
}
