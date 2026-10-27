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
//! SEL5 (WC3 alignment, ADR-0016): the screen->world->hit bottom layer now
//! rides the warbase picking seam — `ActiveWorldPicker` (with warden's
//! `GroundPlanePicker`, see below) converts cursor pixels to ground-plane
//! world XY, `pick_bridge` emits the base `PickAttempt` message, and
//! `resolve_world_picks` produces the base `PickResolved` shape for the
//! semantic consumers (`select_object` / `mouse_place_system`).
//!
//! UPCAST BOUNDARY (scope pin, do not over-adopt): warden upcasts the pick
//! MATH (ray->ground-plane + world-space disc hits) and the MESSAGE SHAPES
//! (`PickAttempt`/`PickResolved`) only. It does NOT adopt the base
//! `SelectionState`/`SelectCommand` selection-set model — a TD keeps its own
//! single-target `resources::Selection` state machine (tower / empty slot /
//! fusion target), which stays the semantic layer here. Slot resolution also
//! stays warden-side: the base `resolve_picks` hits `Selectable` bodies and
//! FreeGrid cells, while warden's slots are FixedSlots authored points.
//!
//! Why not the base `OrthoTopDownPicker`: it only inverts orthographic
//! cameras; warden frames the arena with a perspective 3/4 view (camera.rs),
//! so warden swaps the resource with its own perspective implementation —
//! exactly the seam contract ("games with unusual cameras swap the resource",
//! warbase input module).

use bevy::prelude::*;
use bevy::window::Window;

use crate::components::{AttackType, Tower, TowerSlot};
use crate::resources::{
    Boosts, BuildMode, Economy, Hand, Lang, MetaSavePath, MetaState, SelectedTower, TowerDefs,
    UiPrefsPath, UPGRADE1_COST, UPGRADE2_COST, WaveChoice, WavePhase, WaveSchedule,
    WaveState,
};
use crate::states::GameState;
use crate::systems::command::BlocksWorldInput;
use crate::systems::l10n;
use crate::systems::{
    hud::UiFont, input::{apply_choice, try_start_wave}, meta::buy_upgrade,
};
use warbase::input::{ActiveWorldPicker, WorldPicker};
use warbase::prelude::{PickAttempt, PickButton, PickResolved};

// ---------------------------------------------------------------------------
// SEL5: screen->world seam (warden's ActiveWorldPicker implementation)
// ---------------------------------------------------------------------------

/// Warden's `WorldPicker`: perspective camera ray intersected with the y=0
/// ground plane, gameplay 2D coordinates = world (x, z). Installed as the
/// `ActiveWorldPicker` resource by GamePlugin (swapping the base's ortho
/// default — see the module header).
pub struct GroundPlanePicker;

impl WorldPicker for GroundPlanePicker {
    fn screen_to_world(&self, world: &World, screen: Vec2) -> Option<Vec2> {
        // &World seam: iterate entities instead of a QueryState (same trick
        // as the base OrthoTopDownPicker); warden frames exactly one camera.
        let mut found: Option<(&Camera, &GlobalTransform)> = None;
        let mut count = 0usize;
        for entity in world.iter_entities() {
            if let (Some(cam), Some(tf)) = (entity.get::<Camera>(), entity.get::<GlobalTransform>())
            {
                count += 1;
                found = Some((cam, tf));
            }
        }
        if count != 1 {
            return None; // camera.single() semantics (no or ambiguous camera)
        }
        let (cam, cam_tf) = found?;
        ray_ground_point(cam, cam_tf, screen)
    }
}

/// Shared core of every warden screen->world conversion: viewport pixel ->
/// camera ray -> intersection with the y=0 ground plane, as (x, z). Same
/// math the pre-SEL5 skill aiming used (`skills::cursor_ground_point` now
/// delegates here); the ground-plane picker above is this plus camera lookup.
pub fn ray_ground_point(
    cam: &Camera,
    cam_tf: &GlobalTransform,
    screen: Vec2,
) -> Option<Vec2> {
    let ray = cam.viewport_to_world(cam_tf, screen).ok()?;
    if ray.direction.y.abs() < 1e-6 {
        return None; // view direction parallel to the ground plane
    }
    let t = -ray.origin.y / ray.direction.y;
    if t < 0.0 {
        return None; // the ground is behind the camera
    }
    let point = ray.origin + *ray.direction * t;
    Some(Vec2::new(point.x, point.z))
}

/// The cursor's ground-plane position, refreshed every frame by `pick_bridge`
/// (Playing only). Non-click consumers (the UI3 hover ghost) read this
/// instead of re-deriving the ray themselves — one screen->world owner.
#[derive(Resource, Debug, Default)]
pub struct CursorGround(pub Option<Vec2>);

/// World-space click slop. The pre-SEL5 hit tests compared SCREEN distances
/// with a 45 px threshold; through warden's pinned framing (camera at
/// (0, 22, 27.5), 45 deg fov, 1280x720) 45 px is ~1.8 world units laterally
/// and ~2.3 along the depth axis at map center, so 2.0 is the disc mid-value.
/// Slots sit >= 4.0 apart (`map::SLOT_POSITIONS`), so the disc never changes
/// WHICH slot/tower wins a pick — it only trims far-field clicks. The
/// old-vs-new equivalence is pinned by tests/pick_upcast.rs.
pub const TOWER_PICK_RADIUS: f32 = 2.0;
/// See `TOWER_PICK_RADIUS`.
pub const SLOT_PICK_RADIUS: f32 = 2.0;

/// Raw-input bridge (SEL5): the single owner of screen->world input —
/// refreshes `CursorGround` every frame and emits one base `PickAttempt`
/// per left press that lands on the world (not over blocking UI). The
/// semantic consumers downstream never touch raw mouse/window state again.
/// Exclusive like the base `pointer_bridge` (the picker seam needs &World).
pub fn pick_bridge(world: &mut World) {
    // Cursor from THE window (pre-SEL5 consumers used windows.single()).
    let cursor = {
        let mut windows = world.query::<&Window>();
        match windows.single(world) {
            Ok(w) => w.cursor_position(),
            Err(_) => None,
        }
    };
    let ground = cursor.and_then(|c| {
        world
            .resource::<ActiveWorldPicker>()
            .0
            .screen_to_world(world, c)
    });
    world.resource_mut::<CursorGround>().0 = ground;

    let Some(buttons) = world.get_resource::<ButtonInput<bevy::input::mouse::MouseButton>>().cloned()
    else {
        return;
    };
    if !buttons.just_pressed(bevy::input::mouse::MouseButton::Left) {
        return;
    }
    // UI panels swallow world picks (pre-SEL5 each consumer checked this;
    // the bridge now checks it once for all of them — same outcome).
    let over_ui = {
        let panel = world
            .get_resource::<crate::resources::DebugPanelCapture>()
            .map(|p| p.pointer)
            .unwrap_or(false);
        let mut roots = world.query_filtered::<&bevy::ui::RelativeCursorPosition, With<BlocksWorldInput>>();
        panel || roots.iter(world).any(|r| r.cursor_over())
    };
    if over_ui {
        return;
    }
    let Some(point) = ground else {
        return;
    };
    let (ctrl, shift) = world
        .get_resource::<ButtonInput<KeyCode>>()
        .map(|keys| {
            (
                keys.any_pressed([KeyCode::ControlLeft, KeyCode::ControlRight]),
                keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]),
            )
        })
        .unwrap_or((false, false));
    world.write_message(PickAttempt {
        world: point,
        button: PickButton::Left,
        click_count: 1,
        ctrl,
        shift,
    });
}

/// SEL5 resolver: base `PickAttempt` -> base-shaped `PickResolved` with
/// warden's FixedSlots semantics. `entities` = towers whose (x, z) disc
/// catches the pick (nearest first, ties by Entity index — D6); `cell` =
/// FixedSlots cell `(slot_index, 0)` of the nearest slot within
/// `SLOT_PICK_RADIUS`. Occupancy stays a consumer concern (the grid is the
/// authority, WX6-6c); the pre-threshold "nearest slot anywhere" used by
/// armed placement is intentionally NOT encoded here — `mouse_place_system`
/// keeps its own no-threshold nearest-slot from the pick point.
pub fn resolve_world_picks(
    mut attempts: MessageReader<PickAttempt>,
    mut resolved: MessageWriter<PickResolved>,
    towers: Query<(Entity, &Transform), With<Tower>>,
    slots: Query<(Entity, &TowerSlot, &Transform)>,
) {
    for attempt in attempts.read() {
        let mut tower_hits: Vec<(f32, Entity)> = towers
            .iter()
            .filter_map(|(eid, tf)| {
                let d = Vec2::new(tf.translation.x, tf.translation.z).distance(attempt.world);
                (d <= TOWER_PICK_RADIUS).then_some((d, eid))
            })
            .collect();
        tower_hits.sort_by(|a, b| {
            a.0.total_cmp(&b.0).then_with(|| a.1.index().cmp(&b.1.index()))
        });
        let mut slot_hits: Vec<(f32, Entity, i32)> = slots
            .iter()
            .filter_map(|(eid, slot, tf)| {
                let d = Vec2::new(tf.translation.x, tf.translation.z).distance(attempt.world);
                (d <= SLOT_PICK_RADIUS).then_some((d, eid, slot.index as i32))
            })
            .collect();
        slot_hits.sort_by(|a, b| {
            a.0.total_cmp(&b.0).then_with(|| a.1.index().cmp(&b.1.index()))
        });
        resolved.write(PickResolved {
            world: attempt.world,
            button: attempt.button,
            entities: tower_hits.into_iter().map(|(_, e)| e).collect(),
            cell: slot_hits.first().map(|(_, _, idx)| (*idx, 0)),
        });
    }
}

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
    wave: Res<WaveState>,
    schedule: Res<WaveSchedule>,
    choice: Res<WaveChoice>,
    state: Res<State<GameState>>,
    mut start: MessageWriter<warbase::prelude::StartWave>,
) {
    for (inter, _) in &mut q {
        if *inter == Interaction::Pressed {
            try_start_wave(&wave, &schedule, &choice, &state, &mut start);
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
    let stale: Vec<Entity> = roots.iter().collect();
    if !choice.pending {
        // WARDEN-SKIN: despawn stays deferred; no themed spawn follows.
        for root in stale {
            commands.entity(root).despawn();
        }
        return;
    }
    let font = font.0.clone();
    let lang = *lang;
    let options: Vec<(String, String)> = choice
        .options
        .iter()
        .map(|opt| (opt.label.clone(), opt.label_en.clone()))
        .collect();
    commands.queue(move |world: &mut World| {
        let ui = world.resource::<warbase::ui::UiTheme>().clone();
        for root in stale {
            let _ = world.entity_mut(root).despawn();
        }
        world
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
                for (i, (label_zh, label_en)) in options.iter().enumerate() {
                    root.spawn((
                        Button,
                        ChoiceCardButton { index: i },
                        Interaction::None,
                        Node {
                            width: Val::Px(crate::systems::theme::CARD_W),
                            height: Val::Px(crate::systems::theme::CARD_H),
                            flex_direction: FlexDirection::Column,
                            justify_content: JustifyContent::Center,
                            align_items: AlignItems::Center,
                            row_gap: Val::Px(4.0),
                            border: UiRect::all(Val::Px(ui.spacing.slot_border_w)),
                            border_radius: BorderRadius::all(Val::Px(ui.radius.slot)),
                            ..default()
                        },
                        BackgroundColor(ui.palette.panel_raised.color()),
                        BorderColor::all(ui.palette.accent.color()),
                    ))
                    .with_children(|p| {
                        p.spawn((
                            Text::new(format!("[{}", i + 1)),
                            TextFont {
                                font: font.clone(),
                                font_size: FontSize::Px(ui.typography.font_caption),
                                ..default()
                            },
                            TextColor(ui.palette.accent.color()),
                        ));
                        p.spawn((
                            Text::new(match lang {
                                Lang::Zh => label_zh.as_str(),
                                Lang::En => label_en.as_str(),
                            }),
                            TextFont {
                                font: font.clone(),
                                font_size: FontSize::Px(ui.typography.font_title),
                                ..default()
                            },
                            TextColor(ui.palette.text_primary.color()),
                        ));
                    });
                }
            });
        info!("[ui2] choice cards shown: {}", options.len());
    });
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
///
/// SEL5: the cursor's ground point comes from `CursorGround` (the pick
/// bridge owns screen->world); the nearest slot is measured in world space
/// with NO threshold — the pre-SEL5 ghost likewise highlighted the nearest
/// slot wherever the cursor was (screen-space nearest, also unthresholded).
pub fn update_hover_ghost(
    mut commands: Commands,
    cursor: Res<CursorGround>,
    build: Res<BuildMode>,
    selected: Res<SelectedTower>,
    defs: Res<TowerDefs>,
    wave: Res<WaveState>,
    state: Res<State<GameState>>,
    grid: Res<warbase::prelude::BuildGrid>,
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
        if let Some(point) = cursor.0 {
            if let Some((slot_e, pos)) = nearest_slot_ground(&slots, point) {
                if let Ok((_, slot, _)) = slots.get(slot_e) {
                    // WX6-6c: same-source occupancy verdict from the grid.
                    if grid.check(crate::systems::map::slot_cell(slot.index)).is_ok() {
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
/// WARDEN-SKIN: themed inside the deferred spawn (signature unchanged; the
/// 34px banner stays a literal — no display-scale token in UiTheme yet, see
/// systems/theme.rs TODO).
fn build_end_screen(commands: &mut Commands, font: &UiFont, lang: Lang) {
    let font = font.0.clone();
    commands.queue(move |world: &mut World| {
        let ui = world.resource::<warbase::ui::UiTheme>().clone();
        world
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
                BackgroundColor(ui.state_colors.cooldown.color()),
            ))
            .with_children(|root| {
                root.spawn((
                    Text::new(l10n::END_BANNER.pick(lang)),
                    TextFont {
                        font: font.clone(),
                        font_size: FontSize::Px(34.0),
                        ..default()
                    },
                    TextColor(ui.state_colors.gold.color()),
                ));
                root.spawn((
                    Text::new(l10n::END_HINT.pick(lang)),
                    TextFont {
                        font: font.clone(),
                        font_size: FontSize::Px(ui.typography.font_label),
                        ..default()
                    },
                    TextColor(ui.palette.text_secondary.color()),
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
                                border: UiRect::all(Val::Px(ui.spacing.slot_border_w)),
                                ..default()
                            },
                            BackgroundColor(ui.palette.panel_raised.color()),
                            BorderColor::all(ui.palette.accent.color()),
                        ))
                        .with_children(|p| {
                            p.spawn((
                                Text::new(label),
                                TextFont {
                                    font: font.clone(),
                                    font_size: FontSize::Px(ui.typography.font_label),
                                    ..default()
                                },
                                TextColor(ui.palette.text_primary.color()),
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
                            border: UiRect::all(Val::Px(ui.spacing.slot_border_w)),
                            ..default()
                        },
                        BackgroundColor(ui.state_colors.gold.color()),
                        BorderColor::all(ui.palette.accent.color()),
                    ))
                    .with_children(|p| {
                        p.spawn((
                            Text::new(l10n::RETRY.pick(lang)),
                            TextFont {
                                font: font.clone(),
                                font_size: FontSize::Px(ui.typography.font_title),
                                ..default()
                            },
                            TextColor(ui.palette.panel_bg.color()),
                        ));
                    });
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
/// Bundled placement-gate params (mouse_place_system sat at the 16-tuple
/// wall once the grid joined; same pattern as command.rs's CardParams).
#[derive(bevy::ecs::system::SystemParam)]
pub struct PlaceGate<'w> {
    pub selected: Res<'w, SelectedTower>,
    pub defs: Res<'w, TowerDefs>,
    pub registry: Res<'w, warbase::prelude::UnitRegistry>,
    pub wave: Res<'w, WaveState>,
    pub hand: Res<'w, Hand>,
    pub build: Res<'w, BuildMode>,
    pub economy: ResMut<'w, Economy>,
    pub grid: ResMut<'w, warbase::prelude::BuildGrid>,
}

/// SEL5: consumes base `PickResolved` picks (the pick bridge already gated
/// the click on the raw side: left press, Playing, not over blocking UI).
/// Keeps the pre-SEL5 placement semantics verbatim: armed + Intermission
/// only, and the NEAREST slot to the pick point with NO distance threshold
/// (an armed click anywhere on the world commits at the nearest slot — the
/// resolver's `SLOT_PICK_RADIUS` cell is a selection affordance, not a
/// placement gate).
pub fn mouse_place_system(
    mut picks: MessageReader<PickResolved>,
    server: Option<Res<AssetServer>>,
    mut place: PlaceGate<'_>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    slots: Query<(Entity, &mut TowerSlot, &Transform)>,
) {
    if !place.build.armed || place.wave.phase != WavePhase::Intermission {
        return;
    }
    for pick in picks.read() {
        if pick.button != PickButton::Left {
            continue;
        }
        let Some((slot_e, _)) = nearest_slot_ground(&slots, pick.world) else {
            continue;
        };
        let Some(def) = place.defs.list.get(place.selected.tower_index) else {
            continue;
        };
        let cost = crate::systems::tower::tower_cost(&place.registry, def);
        if !place.hand.owned_towers.contains(&place.selected.tower_index)
            || place.economy.gold < cost
        {
            continue;
        }
        if let Ok((_, slot, stf)) = slots.get(slot_e) {
            let cell = crate::systems::map::slot_cell(slot.index);
            if place.grid.check(cell).is_err() {
                continue; // occupied (or invalid) — the grid is the authority
            }
            place.economy.gold -= cost;
            let tower = crate::systems::tower::spawn_base_tower(
                &mut commands,
                def,
                place.selected.tower_index,
                stf.translation,
                &mut meshes,
                &mut materials,
                server.as_deref(),
            );
            place.grid.place(cell, tower);
            info!("[mouse] placed {} at slot, gold={}", def.name, place.economy.gold);
        }
    }
}

/// Nearest slot to a ground-plane pick point (world space, no threshold) —
/// the placement/ghost counterpart of the pre-SEL5 screen-space
/// `nearest_slot_screen` (which also applied no threshold).
fn nearest_slot_ground(
    slots: &Query<(Entity, &mut TowerSlot, &Transform)>,
    point: Vec2,
) -> Option<(Entity, Vec3)> {
    let mut best: Option<(Entity, f32, Vec3)> = None;
    for (eid, _slot, stf) in slots.iter() {
        let d = Vec2::new(stf.translation.x, stf.translation.z).distance(point);
        if best.map_or(true, |(_, bd, _)| d < bd) {
            best = Some((eid, d, stf.translation));
        }
    }
    best.map(|(e, _, p)| (e, p))
}
