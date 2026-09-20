//! Showroom — a generic neutral-studio asset viewer (team engine crate,
//! ADR-0010). Mechanisms live here; games only register WHAT to show.
//!
//! What this crate owns: the studio environment (gray floor, pad, white key
//! light, neutral ambient/clear color), the entry + resident sweeps (async
//! scene strays are removed the frame they surface), the catalog model
//! (categories -> items, each item decorating an engine-spawned exhibit
//! root), the egui asset list (auto-skipped when no egui is present), the
//! orbit/zoom camera and the Tab/arrows key flow. Everything is gated on
//! the `ShowroomState` resource existing — no game state machine knowledge.
//!
//! What the game owns: building its catalog (reading its own def tables,
//! calling its own appearance seams in the item closures), marking entities
//! that must survive the sweeps (`ShowroomKept`), and freezing its gameplay
//! (its own GameState) when entering.
//!
//! A second consumer ships in this crate: the `asset-viewer` bin scans any
//! directory for .glb/.gltf files and browses them with zero integration.

use bevy::input::mouse::{AccumulatedMouseMotion, MouseWheel};
use bevy::prelude::*;

/// Exhibit stands at the studio origin.
pub const SHOWROOM_PAD_RADIUS: f32 = 1.7;
/// Orbit target height (roughly the exhibit's mid-body).
pub const ORBIT_TARGET: Vec3 = Vec3::new(0.0, 1.0, 0.0);

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

/// Marker for entities that must survive the sweeps (the game inserts it
/// before `enter_studio` — e.g. Warden's hover ghosts cache their entity id).
#[derive(Component)]
pub struct ShowroomKept;

/// Plate font (FontSource: a handle, or the default font).
#[derive(Resource)]
pub struct ShowroomFont(pub FontSource);

/// What a game registers to make an asset browseable. Build with
/// `ShowroomCatalog::build()`.
#[derive(Resource, Default)]
pub struct ShowroomCatalog {
    pub categories: Vec<ShowroomCategory>,
}

/// One browseable category (e.g. "Towers", "Enemies", or a folder name).
pub struct ShowroomCategory {
    pub name: String,
    pub items: Vec<ShowroomItem>,
}

/// One catalog entry: how to name it, what to say about it, and how to put
/// it on the pad. The spawn closure decorates the engine-spawned exhibit
/// ROOT (insert meshes / attach scene children onto it).
pub struct ShowroomItem {
    /// Panel label.
    pub name: String,
    /// Stats block shown under "On the pad" (preformatted by the game).
    pub info: String,
    /// World-space plate text (None or no font => no plate).
    pub plate: Option<String>,
    /// Root anchor height (match the in-game placement anchor).
    pub anchor_y: f32,
    /// Plate height above the root.
    pub plate_y: f32,
    #[allow(clippy::type_complexity)]
    pub spawn: Box<dyn Fn(&mut Commands, Entity, &mut SpawnCtx) + Send + Sync>,
}

/// What the spawn closure may use to assemble the exhibit (mutable:\            
/// mesh/material handles are created through it).
pub struct SpawnCtx<'a> {
    pub meshes: &'a mut Assets<Mesh>,
    pub materials: &'a mut Assets<StandardMaterial>,
    pub server: Option<&'a AssetServer>,
}

impl ShowroomCatalog {
    pub fn build() -> Self {
        Self::default()
    }

    pub fn category(self, name: impl Into<String>) -> CatalogCategoryBuilder {
        CatalogCategoryBuilder {
            catalog: self,
            name: name.into(),
            items: Vec::new(),
        }
    }
}

/// Fluent builder for one category (`catalog.category("Towers").item(...).finish()`).
pub struct CatalogCategoryBuilder {
    catalog: ShowroomCatalog,
    name: String,
    items: Vec<ShowroomItem>,
}

impl CatalogCategoryBuilder {
    pub fn item(mut self, item: ShowroomItem) -> Self {
        self.items.push(item);
        self
    }

    pub fn finish(mut self) -> ShowroomCatalog {
        self.catalog.categories.push(ShowroomCategory {
            name: self.name,
            items: self.items,
        });
        self.catalog
    }
}

/// Enter the studio: sweep 3D visuals away, hide UI root nodes, build the
/// neutral environment, insert the catalog + browser state. The game calls
/// this from its own entry point (scenario/flag) and freezes its gameplay
/// around it; leaving the studio is the consumer's concern (restart).
pub fn enter_studio(world: &mut World, catalog: ShowroomCatalog, font: FontSource) {
    // 1) One-shot sweep of every 3D-visual root (mesh / async scene root /
    //    light) except kept/marked entities. Async stragglers are handled by
    //    the resident sweep system.
    let doomed: Vec<Entity> = {
        let mut q = world.query_filtered::<
            Entity,
            (
                Or<(
                    With<Mesh3d>,
                    With<bevy::world_serialization::WorldAssetRoot>,
                    With<DirectionalLight>,
                )>,
                Without<ShowroomKept>,
                Without<ShowroomFloor>,
                Without<ShowroomPad>,
                Without<ShowroomExhibit>,
                Without<ShowroomLight>,
                Without<Camera3d>,
            ),
        >();
        q.iter(world).collect()
    };
    for e in doomed {
        world.despawn(e);
    }
    // 2) Hide the game UI: every root bevy_ui node (egui panels are not
    //    Nodes and stay).
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
        Transform::from_rotation(Quat::from_euler(EulerRot::ZYX, 0.0, 0.7, -0.9)),
        DirectionalLight {
            color: Color::srgb(1.0, 0.98, 0.95),
            illuminance: 16_000.0,
            shadow_maps_enabled: true,
            ..default()
        },
    ));
    // 4) Catalog + browser state.
    world.insert_resource(catalog);
    world.insert_resource(ShowroomFont(font));
    world.insert_resource(ShowroomState {
        category: 0,
        item: 0,
        spawned: None,
    });
    world.init_resource::<ShowroomOrbit>();
    info!("[sm] studio showroom entered (Tab/arrows or panel; drag orbit, wheel zoom)");
}

/// Registers the viewer systems. Add inside your debug-gated setup; the
/// systems no-op until `enter_studio` inserts the state. The egui list
/// panel is only registered when the host app already has bevy_egui.
pub struct ShowroomPlugin;

impl Plugin for ShowroomPlugin {
    fn build(&self, app: &mut App) {
        // The panel needs a live egui context; skip it entirely otherwise
        // (headless test worlds, the bin without egui, ...).
        if app.is_plugin_added::<bevy_egui::EguiPlugin>() {
            app.add_systems(
                bevy_egui::EguiPrimaryContextPass,
                showroom_panel_ui,
            );
        }
        app.init_resource::<PanelPointer>()
            .add_systems(
                Update,
                (showroom_keys, showroom_present.after(showroom_keys)).chain(),
            )
            .add_systems(Update, showroom_sweep)
            .add_systems(Update, showroom_orbit);
    }
}

/// Browse keys: Tab cycles category, ←/→ cycles the item. Only moves the
/// cursor; `showroom_present` swaps.
pub fn showroom_keys(
    keys: Res<ButtonInput<KeyCode>>,
    state: Option<ResMut<ShowroomState>>,
    catalog: Option<Res<ShowroomCatalog>>,
) {
    let Some(mut state) = state else {
        return;
    };
    let Some(catalog) = catalog else {
        return;
    };
    let len = |c: usize| {
        catalog
            .categories
            .get(c)
            .map(|cat| cat.items.len())
            .unwrap_or(0)
    };
    let n_categories = catalog.categories.len().max(1);
    if keys.just_pressed(KeyCode::Tab) {
        state.category = (state.category + 1) % n_categories;
        state.item = 0;
    }
    if keys.just_pressed(KeyCode::ArrowRight) {
        state.item = (state.item + 1) % len(state.category).max(1);
    }
    if keys.just_pressed(KeyCode::ArrowLeft) {
        state.item = (state.item + len(state.category).max(1) - 1) % len(state.category).max(1);
    }
}

/// The single exhibit swap point: whenever the cursor differs from what is
/// on the pad, despawn the old exhibit, spawn a fresh root and let the
/// catalog item's closure decorate it (also covers the first frame).
#[allow(clippy::too_many_arguments)]
pub fn showroom_present(
    mut commands: Commands,
    state: Option<ResMut<ShowroomState>>,
    exhibits: Query<Entity, With<ShowroomExhibit>>,
    catalog: Option<Res<ShowroomCatalog>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    server: Option<Res<AssetServer>>,
    font: Option<Res<ShowroomFont>>,
) {
    let Some(mut state) = state else {
        return;
    };
    let Some(catalog) = catalog else {
        return;
    };
    if state.spawned == Some((state.category, state.item)) {
        return;
    }
    let Some(item) = catalog
        .categories
        .get(state.category)
        .and_then(|c| c.items.get(state.item))
    else {
        return;
    };
    for e in &exhibits {
        commands.entity(e).despawn();
    }
    let root = commands
        .spawn((
            ShowroomExhibit,
            Visibility::Visible,
            Transform::from_xyz(0.0, item.anchor_y, 0.0),
        ))
        .id();
    let mut ctx = SpawnCtx {
        meshes: &mut meshes,
        materials: &mut materials,
        server: server.as_deref(),
    };
    (item.spawn)(&mut commands, root, &mut ctx);
    if let Some(text) = &item.plate {
        let plate_y = item.plate_y;
        let font = font.map(|f| f.0.clone()).unwrap_or_default();
        commands.entity(root).with_children(|p| {
            p.spawn((
                ShowroomPlate,
                Text2d::new(text.clone()),
                TextFont {
                    font,
                    font_size: FontSize::Px(22.0),
                    ..default()
                },
                TextColor(Color::srgb(0.1, 0.1, 0.12)),
                // Tilt toward the camera so the plate reads upright.
                Transform::from_xyz(0.0, plate_y, 0.0)
                    .with_rotation(Quat::from_rotation_x(-0.67)),
            ));
        });
    }
    info!("[sm] showing {} · {}", item.name, item.plate.as_deref().unwrap_or(""));
    state.spawned = Some((state.category, state.item));
}

/// Resident sweep: while the studio is active, any ROOT entity carrying a
/// 3D-visual component that is not part of the studio is removed the moment
/// it appears — async scene children materialize frames AFTER the entry
/// sweep; scene children orphaned by a mid-load root despawn surface as
/// unmarked roots one frame later and get cleaned then (self-healing).
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
            Without<ShowroomKept>,
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
/// Pointer over any egui panel disables both (the panel sets the capture).
pub fn showroom_orbit(
    buttons: Res<ButtonInput<bevy::input::mouse::MouseButton>>,
    motion: Res<AccumulatedMouseMotion>,
    mut wheel: MessageReader<MouseWheel>,
    mut panel_pointer: Option<ResMut<PanelPointer>>,
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
    let over_panel = panel_pointer.as_mut().map(|p| p.0).unwrap_or(false);
    if !over_panel && buttons.pressed(bevy::input::mouse::MouseButton::Left) {
        orbit.yaw_deg += motion.delta.x * 0.4;
        orbit.pitch_deg = (orbit.pitch_deg + motion.delta.y * 0.3).clamp(8.0, 80.0);
    }
    for ev in wheel.read() {
        orbit.dist = (orbit.dist - ev.y * 0.8).clamp(2.2, 16.0);
    }
    let yaw = orbit.yaw_deg.to_radians();
    let pitch = orbit.pitch_deg.to_radians();
    let offset =
        Vec3::new(pitch.cos() * yaw.sin(), pitch.sin(), pitch.cos() * yaw.cos()) * orbit.dist;
    for mut tf in &mut cameras {
        *tf = Transform::from_translation(ORBIT_TARGET + offset).looking_at(ORBIT_TARGET, Vec3::Y);
    }
}

/// Pointer-over-egui flag written by the panel system, read by the orbit
/// (keeps this crate free of warden's DebugPanelCapture).
#[derive(Resource, Default)]
pub struct PanelPointer(pub bool);

/// The egui asset list (left docked window): click a def to put it on the
/// pad, read its info below. Also flags pointer-over-panel for the orbit.
pub fn showroom_panel_ui(
    mut contexts: bevy_egui::EguiContexts,
    mut panel_pointer: Option<ResMut<PanelPointer>>,
    state: Option<ResMut<ShowroomState>>,
    catalog: Option<Res<ShowroomCatalog>>,
) {
    let Some(mut state) = state else {
        return;
    };
    let Some(catalog) = catalog else {
        return;
    };
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };
    let over = ctx.is_pointer_over_egui();
    if let Some(p) = panel_pointer.as_mut() {
        p.0 = over;
    }
    use bevy_egui::egui;
    egui::Window::new("Showroom")
        .anchor(egui::Align2::LEFT_TOP, egui::vec2(0.0, 0.0))
        .movable(false)
        .resizable(false)
        .collapsible(false)
        .default_width(240.0)
        .show(ctx, |ui| {
            ui.heading("Showroom");
            ui.separator();
            for (c, cat) in catalog.categories.iter().enumerate() {
                ui.label(cat.name.clone());
                for (i, item) in cat.items.iter().enumerate() {
                    let selected = state.category == c && state.item == i;
                    if ui.selectable_label(selected, format!("  {}", item.name)).clicked() {
                        state.select(c, i);
                    }
                }
                ui.separator();
            }
            if let Some(item) = catalog
                .categories
                .get(state.category)
                .and_then(|c| c.items.get(state.item))
            {
                ui.label("On the pad");
                ui.monospace(item.info.clone());
            }
            ui.separator();
            ui.small("Tab/arrows switch · drag orbit · wheel zoom");
        });
}
