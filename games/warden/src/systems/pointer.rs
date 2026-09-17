//! Mouse-first interaction (pointer place, UI2 choice cards, UI3 hover ghost,
//! UI5 end screen).
//!   - The legacy bottom shop bar retired with UI10: the command card now owns
//!     the build list (command.rs); this file keeps the armed-flow placement
//!     click and the shared markers it references.
//!   - Pending three-choose-one options appear as clickable cards (UI2, = 1/2/3).
//!   - Left click on a slot places the armed tower (Intermission only).
//!   - Right click / Esc cancel build mode and the selection (command.rs UI9).
//!   - The GameOver/Win end screen (UI5) and the language button handler (UI7)
//!     live here; the button itself lives in the command bar control zone.
//!
//! Split into small systems: Bevy caps a single system at 16 params.

use bevy::input::mouse::MouseButton;
use bevy::prelude::*;
use bevy::window::Window;

use crate::components::{AttackType, TowerSlot};
use crate::resources::{
    Boosts, BuildMode, Economy, Hand, Lang, MetaSavePath, MetaState, SelectedTower, TowerDefs,
    UiPrefsPath, UPGRADE1_COST, UPGRADE2_COST, WaveChoice, WavePhase, WaveSchedule,
    WaveState,
};
use crate::states::GameState;
use crate::systems::command::{cursor_over_blocking_ui, BlocksWorldInput};
use crate::systems::l10n;
use crate::systems::{
    hud::UiFont, input::{apply_choice, try_start_wave}, meta::buy_upgrade,
};

/// "开始下一波" button (lives in the command-bar control zone since UI9):
/// same effect as the Space key.
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

/// The UI-language toggle button (UI7 = the L key); spawned inside the
/// command-bar control zone since UI9.
#[derive(Component)]
pub struct LangToggleButton;

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
/// Cards are clickable equivalents of the 1/2/3 keys; a language toggle
/// rebuilds them so labels follow (UI7).
pub fn refresh_choice_cards(
    mut commands: Commands,
    font: Res<UiFont>,
    lang: Res<Lang>,
    choice: Res<WaveChoice>,
    mut cache: Local<(bool, Lang)>,
    roots: Query<Entity, With<ChoiceCardsRoot>>,
) {
    let key = (choice.pending, *lang);
    if *cache == key {
        return;
    }
    *cache = key;
    for root in &roots {
        commands.entity(root).despawn();
    }
    if !choice.pending {
        return;
    }
    commands
        .spawn((
            ChoiceCardsRoot,
            BlocksWorldInput,
            bevy::ui::RelativeCursorPosition::default(),
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
                        Text::new(format!(
                            "[{}] {}",
                            i + 1,
                            match *lang {
                                Lang::Zh => opt.label.as_str(),
                                Lang::En => opt.label_en.as_str(),
                            }
                        )),
                        TextFont {
                            font: font.0.clone(),
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
fn build_end_screen(commands: &mut Commands, font: &UiFont, lang: Lang) {
    commands
        .spawn((
            EndScreenRoot,
            BlocksWorldInput,
            bevy::ui::RelativeCursorPosition::default(),
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
                Text::new(l10n::END_BANNER.pick(lang)),
                TextFont {
                    font: font.0.clone(),
                    font_size: FontSize::Px(34.0),
                    ..default()
                },
                TextColor(Color::srgb(1.0, 0.85, 0.4)),
            ));
            root.spawn((
                Text::new(l10n::END_HINT.pick(lang)),
                TextFont {
                    font: font.0.clone(),
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
                    (1usize, l10n::upgrade1_label(lang, UPGRADE1_COST)),
                    (2usize, l10n::upgrade2_label(lang, UPGRADE2_COST)),
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
                                font: font.0.clone(),
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
                        Text::new(l10n::RETRY.pick(lang)),
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
}

/// Show the result screen only on GameOver/Win, hide it otherwise (UI5). The
/// panel is rebuilt when the language toggles so its copy follows (UI7).
pub fn refresh_end_screen(
    mut commands: Commands,
    font: Res<UiFont>,
    lang: Res<Lang>,
    state: Res<State<GameState>>,
    mut built: Local<Option<Lang>>,
    mut roots: Query<(Entity, &mut Node), With<EndScreenRoot>>,
) {
    if *built != Some(*lang) {
        for (e, mut node) in &mut roots {
            commands.entity(e).despawn();
            node.display = Display::None; // rebuilt hidden; state re-applies below
        }
        build_end_screen(&mut commands, &font, *lang);
        *built = Some(*lang);
        return;
    }
    let show = matches!(state.get(), GameState::GameOver | GameState::Win);
    for (_, mut node) in &mut roots {
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

/// Clicking the language button toggles zh/en and persists (UI7 = the L key).
pub fn handle_lang_button(
    mut q: Query<&Interaction, (Changed<Interaction>, With<LangToggleButton>)>,
    mut lang: ResMut<Lang>,
    path: Res<UiPrefsPath>,
) {
    for inter in &mut q {
        if *inter == Interaction::Pressed {
            *lang = lang.next();
            l10n::save_ui_prefs(&lang, &path);
            info!("[ui7] lang toggled to {:?} (button)", *lang);
        }
    }
}

/// Left click in build mode places the armed tower at the nearest slot.
pub fn mouse_place_system(
    buttons: Res<ButtonInput<MouseButton>>,
    server: Option<Res<AssetServer>>,
    windows: Query<&Window>,
    camera: Query<(&Camera, &GlobalTransform)>,
    blocking: Query<&bevy::ui::RelativeCursorPosition, With<BlocksWorldInput>>,
    panel: Res<crate::resources::DebugPanelCapture>,
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
    if cursor_over_blocking_ui(&blocking, &panel) {
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
        crate::systems::tower::spawn_base_tower(
            &mut commands,
            def,
            selected.tower_index,
            stf.translation,
            &mut meshes,
            &mut materials,
            server.as_deref(),
        );
        info!("[mouse] placed {} at slot, gold={}", def.name, economy.gold);
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
