//! Showroom v2 (SM1 rework, 2026-09-20 user feedback): a NEUTRAL STUDIO
//! viewer, not a battlefield diorama — light-gray floor, soft white light,
//! no game map / HUD / dusk tint; an egui asset list (left) picks the
//! exhibit, the camera orbits and zooms. Still auto-enumerated from the
//! definition tables and still spawning through the gameplay appearance
//! seams (attach_tower_appearance / fused_tower_color / enemy_base_color).
//!
//! "Same look as the game" is amended by the feedback: for JUDGING an asset
//! the environment must be neutral (the dusk palette tinted everything);
//! the in-game context lives on in the SM3 live-fire zone instead.
//!
//! Keys (still available alongside the panel): Tab = category, ←/→ = item.
//! Left-drag = orbit, wheel = zoom, restart to leave. Debug-only like every
//! DB/SM card; entering via the scenario marks the run (save isolation).
//! Logging prefix `[sm]` doubles as the release-scan string.

use bevy::input::mouse::{AccumulatedMouseMotion, MouseWheel};
use bevy::prelude::*;

use crate::resources::{
    DebugPanelCapture, EnemyDefs, FusionDefs, Lang, TowerDefs, TowerDef,
};
use crate::systems::enemy::enemy_base_color;
use crate::systems::hud::UiFont;
use crate::systems::tower::{attach_tower_appearance, fused_tower_color};
use crate::states::GameState;

/// The exhibit stands at the studio origin (the battlefield is gone).
const SHOWROOM_PAD_RADIUS: f32 = 1.7;
/// Orbit target height (roughly the exhibit's mid-body).
const ORBIT_TARGET: Vec3 = Vec3::new(0.0, 1.0, 0.0);

/// The browser cursor + what is currently on the pad. `spawned` differing
/// from (category, item) is the single respawn trigger — keys and the egui
/// panel both only move the cursor, one present system does the swapping.
#[derive(Resource)]
pub struct ShowroomState {
    pub category: usize,
    pub item: usize,
    spawned: Option<(usize, usize)>,
}

impl ShowroomState {
    /// The panel/tests move the cursor exactly the way an egui click does.
    pub fn select(&mut self, category: usize, item: usize) {
        self.category = category;
        self.item = item;
    }
}

/// Camera orbit pose (degrees / world units). Defaults frame the exhibit.
#[derive(Resource)]
pub struct ShowroomOrbit {
    pub yaw_deg: f32,
    pub pitch_deg: f32,
    pub dist: f32,
}

impl Default for ShowroomOrbit {
    fn default() -> Self {
        Self {
            yaw_deg: 45.0,
            pitch_deg: 32.0,
            dist: 6.5,
        }
    }
}

/// Marker on the current model exhibit root (there is at most one).
#[derive(Component)]
pub struct ShowroomExhibit;

/// Marker on the exhibit's name-plate child (Text2d, despawns with it).
#[derive(Component)]
pub struct ShowroomPlate;

/// Marker on the studio floor plane.
#[derive(Component)]
pub struct ShowroomFloor;

/// Marker on the display pad disc.
#[derive(Component)]
pub struct ShowroomPad;

/// Marker on the studio key light (survives the resident sweep).
#[derive(Component)]
pub struct ShowroomLight;

/// Number of browseable categories.
const CATEGORY_COUNT: usize = 2;

fn category_len(
    category: usize,
    towers: &TowerDefs,
    fusions: &FusionDefs,
    enemies: &EnemyDefs,
) -> usize {
    match category {
        0 => towers.list.len() + fusions.list.len(),
        _ => enemies.list.len(),
    }
}

fn pick_lang<'a>(zh: &'a str, en: &'a str, lang: &Lang) -> &'a str {
    match *lang {
        Lang::Zh => zh,
        Lang::En => en,
    }
}

/// Enter the studio (DB2 `Scenario::Showroom` → `apply_scenario`): sweep the
/// battlefield away, build the neutral environment, hide the game UI and
/// move to `GameState::Showroom` (gameplay/UI systems are Playing-gated and
/// freeze). The exhibit itself is spawned by `showroom_present` the same
/// frame. Leaving = restart (dev tool).
pub fn enter_showroom(world: &mut World) {
    // 1) Sweep every 3D visual except the hover ghosts (their entity id is
    //    cached in a Local — hide, never despawn; the UI3 system is
    //    Playing-gated and will not touch them again in Showroom).
    let doomed: Vec<Entity> = {
        let mut q = world
            .query_filtered::<Entity, (With<Mesh3d>, Without<crate::systems::pointer::HoverGhost>)>();
        q.iter(world).collect()
    };
    for e in doomed {
        world.despawn(e);
    }
    let lights: Vec<Entity> = {
        let mut q = world.query_filtered::<Entity, With<DirectionalLight>>();
        q.iter(world).collect()
    };
    for e in lights {
        world.despawn(e);
    }
    {
        let mut q = world
            .query_filtered::<Entity, With<crate::systems::pointer::HoverGhost>>();
        let ghosts: Vec<Entity> = q.iter(world).collect();
        for e in ghosts {
            if let Some(mut vis) = world.get_mut::<Visibility>(e) {
                *vis = Visibility::Hidden;
            }
        }
    }
    // 2) Hide the game UI: every root bevy_ui node (command bar, HUD,
    //    overlays). egui panels are not Nodes and stay.
    {
        let mut q = world.query_filtered::<Entity, (With<Node>, Without<ChildOf>)>();
        let roots: Vec<Entity> = q.iter(world).collect();
        for e in roots {
            if let Some(mut vis) = world.get_mut::<Visibility>(e) {
                *vis = Visibility::Hidden;
            }
        }
    }
    // 3) Neutral studio: pale clear color, white ambient, one white key
    //    light, gray floor + darker pad.
    world.insert_resource(ClearColor(Color::srgb(0.78, 0.78, 0.82)));
    world.insert_resource(GlobalAmbientLight {
        color: Color::srgb(0.95, 0.95, 1.0),
        brightness: 120.0,
        ..default()
    });
    let floor_mesh = world
        .resource_mut::<Assets<Mesh>>()
        .add(Plane3d::new(Vec3::Y, Vec2::new(30.0, 30.0)));
    let floor_mat = world
        .resource_mut::<Assets<StandardMaterial>>()
        .add(StandardMaterial {
            base_color: Color::srgb(0.62, 0.62, 0.66),
            perceptual_roughness: 1.0,
            ..default()
        });
    world.spawn((
        ShowroomFloor,
        Mesh3d(floor_mesh),
        MeshMaterial3d(floor_mat),
        Transform::from_xyz(0.0, 0.0, 0.0),
    ));
    let pad_mesh = world
        .resource_mut::<Assets<Mesh>>()
        .add(Circle::new(SHOWROOM_PAD_RADIUS));
    let pad_mat = world
        .resource_mut::<Assets<StandardMaterial>>()
        .add(StandardMaterial {
            base_color: Color::srgb(0.35, 0.36, 0.4),
            perceptual_roughness: 1.0,
            ..default()
        });
    world.spawn((
        ShowroomPad,
        Mesh3d(pad_mesh),
        MeshMaterial3d(pad_mat),
        Transform::from_xyz(0.0, 0.01, 0.0)
            .with_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)),
    ));
    world.spawn((
        ShowroomLight,
        Transform::from_rotation(Quat::from_euler(
            EulerRot::ZYX,
            0.0,
            0.7,
            -0.9,
        )),
        DirectionalLight {
            color: Color::srgb(1.0, 0.98, 0.95),
            illuminance: 16_000.0,
            shadow_maps_enabled: true,
            ..default()
        },
    ));
    // 4) Browser state + dedicated game state (freezes gameplay & UI).
    world.insert_resource(ShowroomState {
        category: 0,
        item: 0,
        spawned: None,
    });
    world.init_resource::<ShowroomOrbit>();
    world
        .resource_mut::<NextState<GameState>>()
        .set(GameState::Showroom);
    info!("[sm] studio showroom entered (Tab/arrows or panel; drag orbit, wheel zoom; restart to leave)");
}

/// Browse keys (SM1): Tab cycles category, ←/→ cycles the item — the panel
/// is the mouse equivalent. Only moves the cursor; `showroom_present` swaps.
pub fn showroom_keys(
    keys: Res<ButtonInput<KeyCode>>,
    panel: Res<DebugPanelCapture>,
    state: Option<ResMut<ShowroomState>>,
    tower_defs: Res<TowerDefs>,
    fusion_defs: Res<FusionDefs>,
    enemy_defs: Res<EnemyDefs>,
) {
    let Some(mut state) = state else {
        return;
    };
    if panel.keyboard {
        return;
    }
    let len =
        |c: usize| category_len(c, &tower_defs, &fusion_defs, &enemy_defs);
    if keys.just_pressed(KeyCode::Tab) {
        state.category = (state.category + 1) % CATEGORY_COUNT;
        state.item = 0;
    }
    if keys.just_pressed(KeyCode::ArrowRight) {
        state.item = (state.item + 1) % len(state.category);
    }
    if keys.just_pressed(KeyCode::ArrowLeft) {
        state.item = (state.item + len(state.category) - 1) % len(state.category);
    }
}

/// The single exhibit swap point: whenever the cursor differs from what is
/// on the pad, despawn the old exhibit and spawn the new one through the
/// shared appearance seams (also covers the first frame after entering).
#[allow(clippy::too_many_arguments)]
pub fn showroom_present(
    mut commands: Commands,
    state: Option<ResMut<ShowroomState>>,
    exhibits: Query<Entity, With<ShowroomExhibit>>,
    tower_defs: Res<TowerDefs>,
    fusion_defs: Res<FusionDefs>,
    enemy_defs: Res<EnemyDefs>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    server: Option<Res<AssetServer>>,
    lang: Res<Lang>,
    font: Res<UiFont>,
) {
    let Some(mut state) = state else {
        return;
    };
    if state.spawned == Some((state.category, state.item)) {
        return;
    }
    for e in &exhibits {
        commands.entity(e).despawn();
    }
    spawn_exhibit(
        &mut commands,
        &state,
        &tower_defs,
        &fusion_defs,
        &enemy_defs,
        &mut meshes,
        &mut materials,
        server.as_deref(),
        &lang,
        &font,
    );
    state.spawned = Some((state.category, state.item));
}

/// Resident sweep (SM1 v2.1): while the studio is active, any ROOT entity
/// carrying a 3D-visual component (mesh / async scene root / light) that is
/// not part of the studio is removed the moment it appears. Async scene
/// children (ART5 props, ART4 structures) materialize frames AFTER the
/// entry sweep and would otherwise leak into the neutral viewer; scene
/// children orphaned by a mid-load root despawn surface as unmarked roots
/// one frame later and get cleaned then (self-healing). Excludes: bevy_ui
/// nodes (handled by the entry hide), the camera, the hidden hover ghosts.
pub fn showroom_sweep(
    state: Option<Res<ShowroomState>>,
    mut commands: Commands,
    strays: Query<
        Entity,
        (
            Or<(
                With<Mesh3d>,
                With<bevy::world_serialization::WorldAssetRoot>,
                With<DirectionalLight>,
            )>,
            Without<ChildOf>,
            Without<Camera3d>,
            Without<crate::systems::pointer::HoverGhost>,
            Without<ShowroomFloor>,
            Without<ShowroomPad>,
            Without<ShowroomExhibit>,
            Without<ShowroomLight>,
        ),
    >,
) {
    if state.is_none() {
        return;
    }
    for e in &strays {
        commands.entity(e).despawn();
        info!("[sm] swept a stray 3D root {:?}", e);
    }
}

/// Orbit + zoom camera: left-drag orbits around the exhibit, wheel zooms.
/// Pointer over any egui panel disables both (DebugPanelCapture rule).
pub fn showroom_orbit(
    buttons: Res<ButtonInput<bevy::input::mouse::MouseButton>>,
    motion: Res<AccumulatedMouseMotion>,
    panel: Res<DebugPanelCapture>,
    mut wheel: MessageReader<MouseWheel>,
    state: Option<Res<ShowroomState>>,
    orbit: Option<ResMut<ShowroomOrbit>>,
    mut cameras: Query<&mut Transform, With<Camera>>,
) {
    if state.is_none() {
        return;
    }
    let Some(mut orbit) = orbit else {
        return;
    };
    if !panel.pointer && buttons.pressed(bevy::input::mouse::MouseButton::Left) {
        orbit.yaw_deg += motion.delta.x * 0.4;
        orbit.pitch_deg = (orbit.pitch_deg + motion.delta.y * 0.3).clamp(8.0, 80.0);
    }
    for ev in wheel.read() {
        orbit.dist = (orbit.dist - ev.y * 0.8).clamp(2.2, 16.0);
    }
    let yaw = orbit.yaw_deg.to_radians();
    let pitch = orbit.pitch_deg.to_radians();
    let offset = Vec3::new(pitch.cos() * yaw.sin(), pitch.sin(), pitch.cos() * yaw.cos())
        * orbit.dist;
    for mut tf in &mut cameras {
        *tf = Transform::from_translation(ORBIT_TARGET + offset).looking_at(ORBIT_TARGET, Vec3::Y);
    }
}

/// Spawn the current exhibit on the pad: the model through the same
/// appearance seam the gameplay path uses, plus a bilingual name plate
/// (`{name} · {category} {i}/{n}`) riding it as a child.
#[allow(clippy::too_many_arguments)]
fn spawn_exhibit(
    commands: &mut Commands,
    state: &ShowroomState,
    tower_defs: &TowerDefs,
    fusion_defs: &FusionDefs,
    enemy_defs: &EnemyDefs,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    server: Option<&AssetServer>,
    lang: &Lang,
    font: &UiFont,
) {
    // (label, category label, index, total, anchor Y, plate height)
    let (label, cat, i, n, anchor_y, plate_y) = match state.category {
        0 => {
            let t = tower_defs.list.len();
            let (zh, en) = if state.item < t {
                let d = &tower_defs.list[state.item];
                (d.label, d.label_en)
            } else {
                let d = &fusion_defs.list[state.item - t];
                (d.label, d.label_en)
            };
            (
                pick_lang(zh, en, lang),
                pick_lang("塔", "Towers", lang),
                state.item + 1,
                t + fusion_defs.list.len(),
                0.6,
                2.4,
            )
        }
        _ => {
            let def = &enemy_defs.list[state.item];
            (
                pick_lang(def.label, def.label_en, lang),
                pick_lang("怪", "Enemies", lang),
                state.item + 1,
                enemy_defs.list.len(),
                0.4,
                1.7,
            )
        }
    };
    let mut ec = commands.spawn((
        ShowroomExhibit,
        Visibility::Visible,
        Transform::from_xyz(0.0, anchor_y, 0.0),
    ));
    match state.category {
        0 => {
            let t = tower_defs.list.len();
            if state.item < t {
                let def: &TowerDef = &tower_defs.list[state.item];
                attach_tower_appearance(&mut ec, def, state.item, meshes, materials, server);
            } else {
                // Same look as an in-game fused tower: 1.2 box, per-kind color.
                let kind = fusion_defs.list[state.item - t].kind;
                ec.insert((
                    Mesh3d(meshes.add(Cuboid::new(1.2, 1.2, 1.2))),
                    MeshMaterial3d(materials.add(fused_tower_color(kind))),
                ));
            }
        }
        _ => {
            let def = &enemy_defs.list[state.item];
            ec.insert((
                Mesh3d(meshes.add(Cuboid::new(0.7, 0.7, 0.7))),
                MeshMaterial3d(materials.add(enemy_base_color(def))),
            ));
        }
    }
    let text = format!("{label} · {cat} {i}/{n}");
    info!("[sm] showing {text}");
    ec.with_children(|p| {
        p.spawn((
            ShowroomPlate,
            Text2d::new(text),
            TextFont {
                font: font.0.clone(),
                font_size: FontSize::Px(22.0),
                ..default()
            },
            TextColor(Color::srgb(0.1, 0.1, 0.12)),
            // Tilt toward the camera so the plate reads upright (DB3 idiom).
            Transform::from_xyz(0.0, plate_y, 0.0)
                .with_rotation(Quat::from_rotation_x(-0.67)),
        ));
    });
}

/// The egui asset list (left side panel): click a def to put it on the pad,
/// read its numbers below. English text (dev tool, DB discipline). Also ORs
/// itself into `DebugPanelCapture` so pointer-over-panel disables orbit.
pub fn showroom_panel_ui(
    mut contexts: bevy_egui::EguiContexts,
    mut capture: ResMut<DebugPanelCapture>,
    state: Option<ResMut<ShowroomState>>,
    tower_defs: Res<TowerDefs>,
    fusion_defs: Res<FusionDefs>,
    enemy_defs: Res<EnemyDefs>,
) {
    let Some(mut state) = state else {
        return;
    };
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };
    capture.pointer |= ctx.is_pointer_over_egui();
    use bevy_egui::egui;
    // egui 0.36 has no SidePanel/CentralPanel (panels take a root Ui); a
    // docked, immovable Window reads the same and shares the proven
    // debug_panel_ui pattern.
    egui::Window::new("Showroom")
        .anchor(egui::Align2::LEFT_TOP, egui::vec2(0.0, 0.0))
        .movable(false)
        .resizable(false)
        .collapsible(false)
        .default_width(240.0)
        .show(ctx, |ui| {
        ui.heading("Showroom");
        ui.separator();
        let mut list = |ui: &mut egui::Ui, cat: usize, title: &str, names: &[String]| {
            ui.label(title);
            for (i, name) in names.iter().enumerate() {
                let selected = state.category == cat && state.item == i;
                if ui.selectable_label(selected, format!("  {name}")).clicked() {
                    state.select(cat, i);
                }
            }
            ui.separator();
        };
        let tower_names = tower_defs
            .list
            .iter()
            .map(|d| d.name.to_string())
            .chain(fusion_defs.list.iter().map(|d| d.name.to_string()))
            .collect::<Vec<_>>();
        list(ui, 0, "Towers (+ fused)", &tower_names);
        let enemy_names = enemy_defs
            .list
            .iter()
            .map(|d| d.name.to_string())
            .collect::<Vec<_>>();
        list(ui, 1, "Enemies", &enemy_names);
        // Numbers of whatever is on the pad.
        ui.label("On the pad");
        let t = tower_defs.list.len();
        match state.category {
            0 if state.item < t => {
                let d = &tower_defs.list[state.item];
                ui.monospace(format!(
                    "dmg {:.0} · atk {}/s\nrange {:.0} · {:?}\ncost {}",
                    d.damage, d.attack_speed, d.range, d.attack_type, d.cost
                ));
            }
            0 => {
                let d = &fusion_defs.list[state.item - t];
                ui.monospace(format!(
                    "dmg {:.0} · atk {}/s\nrange {:.0} · {:?}\naoe {:.1} · slow {:.2}/{:.1}s",
                    d.damage, d.attack_speed, d.range, d.attack_type, d.aoe_radius,
                    d.slow_factor, d.slow_duration
                ));
            }
            _ => {
                let d = &enemy_defs.list[state.item];
                ui.monospace(format!(
                    "hp {:.0} · speed {:.1}\narmor {} · healer {}\nleak {} · gold {}",
                    d.hp,
                    d.speed,
                    d.physical_armor,
                    d.healer,
                    d.leak,
                    d.kill_gold
                ));
            }
        }
        ui.separator();
        ui.small("Tab/arrows switch · drag orbit · wheel zoom");
        ui.small("restart the game to leave the showroom");
    });
}
