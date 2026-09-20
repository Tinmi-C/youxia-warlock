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

/// The browser cursor + what is currently on the pad. The stage key
/// (query, gallery, category, item) differing from what is staged is the
/// single respawn trigger — keys and the egui panel both only move the
/// cursor, one present system does the swapping. `gallery` lays the WHOLE
/// current category (or, with an active search, the hit set) out in a
/// grid; clicking an exhibit (or a list entry) returns to focus mode.
#[derive(Resource)]
pub struct ShowroomState {
    pub category: usize,
    pub item: usize,
    pub gallery: bool,
    /// Active composition part (N5-②): Some(i) shows only that top-level
    /// child of the focused exhibit; None shows everything.
    pub selected_part: Option<usize>,
    /// Compare mode (SM8): two catalog entries staged side by side.
    pub compare: Option<((usize, usize), (usize, usize))>,
    /// Panel flag: the next list click becomes compare's B entry.
    pub compare_pick_b: bool,
    stage: Option<Stage>,
}

/// What is currently staged (respawn whenever the wanted stage differs).
#[derive(PartialEq)]
struct Stage {
    query: String,
    gallery: bool,
    category: usize,
    item: usize,
    compare: Option<((usize, usize), (usize, usize))>,
}

impl ShowroomState {
    /// The panel/tests move the cursor exactly the way an egui click does
    /// (any direct selection leaves compare mode and part isolation).
    pub fn select(&mut self, category: usize, item: usize) {
        self.category = category;
        self.item = item;
        self.compare = None;
        self.selected_part = None;
    }

    fn wanted_stage(&self, query: &str) -> Stage {
        Stage {
            query: query.to_string(),
            gallery: self.gallery,
            category: self.category,
            item: self.item,
            compare: self.compare,
        }
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

/// Animation state on the exhibit root (SM2). `index` is the wanted clip,
/// `wired` the clip actually playing (None => wiring still pending/retry;
/// Some(usize::MAX) => the asset has no clips). The clip list comes from
/// the whole-file `Gltf` asset the item carried; the player being driven is
/// the one the gltf loader instantates INSIDE the scene subtree (an external
/// player plays into the void — bones stay bound to the loader's player).
#[derive(Component)]
pub struct ShowroomAnim {
    pub gltf: Handle<bevy::gltf::Gltf>,
    pub index: usize,
    wired: Option<usize>,
}

/// Identity marker on gallery exhibits (which catalog entry this root is)
/// — the click-to-focus pick reads it.
#[derive(Component)]
pub struct ShowroomGalleryItem {
    pub category: usize,
    pub item: usize,
}

/// Marker on a composition exhibit root: how many parts (top-level
/// children, in spawn order) it has (N5-②).
#[derive(Component)]
pub struct ShowroomPartsTag(pub usize);

/// Marker the parts system inserts on each part child (part index).
#[derive(Component)]
pub struct ShowroomPart(pub usize);

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
    /// Asset card: structured label/value rows (extensible — N4).
    pub info_rows: Vec<(String, String)>,
    /// Search/filter metadata (name is always searchable; tags add type,
    /// localized names, composition parts, provenance ids, ...).
    pub tags: Vec<String>,
    /// Provenance rows (filled by the game from its asset manifest; P2).
    pub origin: Vec<Origin>,
    /// World-space plate text (None or no font => no plate).
    pub plate: Option<String>,
    /// Root anchor height (match the in-game placement anchor).
    pub anchor_y: f32,
    /// Plate height above the root.
    pub plate_y: f32,
    /// Whole-file gltf handle when the item is an animated model — enables
    /// the SM2 clip browser (auto idle loop + Space to cycle + panel list).
    pub gltf: Option<Handle<bevy::gltf::Gltf>>,
    /// Composition parts (N5-②): labels for the exhibit's top-level
    /// children in spawn order; two or more enable the part-breakdown view.
    pub parts: Vec<String>,
    #[allow(clippy::type_complexity)]
    pub spawn: Box<dyn Fn(&mut Commands, Entity, &mut SpawnCtx) + Send + Sync>,
}

/// Where a catalog entry (or one part of a composition) comes from.
#[derive(Clone)]
pub enum Origin {
    /// A vault asset (ADR-0009): id + pinned version + human name.
    Vault {
        name: String,
        id: String,
        version: String,
        /// Part role inside a composition (e.g. "tower body" / "ballista").
        role: Option<String>,
    },
    /// Made/adjusted game-side; not (yet) backflowed to the vault.
    GameLocal { note: Option<String> },
}

impl Origin {
    /// One card row for this origin.
    pub fn row(&self) -> String {
        match self {
            Origin::Vault { name, id, version, role } => {
                let r = role
                    .as_ref()
                    .map(|s| format!(" · {s}"))
                    .unwrap_or_default();
                format!("{name} v{version}{r} · {id}")
            }
            Origin::GameLocal { note } => note
                .as_ref()
                .map(|s| format!("游戏本地 · {s}"))
                .unwrap_or_else(|| "游戏本地".into()),
        }
    }
}

/// Search state: empty text = no filtering. Matching is a case-insensitive
/// substring over name + tags; hits flatten across categories.
#[derive(Resource, Default)]
pub struct SearchQuery {
    pub text: String,
}

/// Playback time control (SM5): writes the virtual clock, so animations,
/// the turntable and everything else slow/freeze coherently. `step` lets
/// exactly N frames run while paused.
#[derive(Resource)]
pub struct ShowroomTime {
    pub paused: bool,
    pub speed: f32,
    pub step: u32,
}

impl Default for ShowroomTime {
    fn default() -> Self {
        Self { paused: false, speed: 1.0, step: 0 }
    }
}

/// Turntable auto-rotate toggle (SM5).
#[derive(Resource, Default)]
pub struct ShowroomTurntable {
    pub on: bool,
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
    // 4) Catalog + browser state (+ search/time/turntable defaults).
    world.insert_resource(catalog);
    world.insert_resource(ShowroomFont(font));
    world.insert_resource(ShowroomState {
        category: 0,
        item: 0,
        gallery: false,
        selected_part: None,
        compare: None,
        compare_pick_b: false,
        stage: None,
    });
    world.init_resource::<ShowroomOrbit>();
    world.init_resource::<SearchQuery>();
    world.init_resource::<ShowroomTime>();
    world.init_resource::<ShowroomTurntable>();
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
            .add_systems(Update, showroom_animate)
            .add_systems(Update, showroom_pick)
            .add_systems(Update, showroom_parts)
            .add_systems(Update, showroom_time_control)
            .add_systems(Update, showroom_orbit);
    }
}

/// Browse keys: Tab cycles category, ←/→ cycles the item, G toggles the
/// gallery grid, T the turntable, R resets the camera. Typing into the
/// panel's search box suspends them (panel sets the keyboard flag). Only
/// moves the cursor; `showroom_present` swaps.
pub fn showroom_keys(
    keys: Res<ButtonInput<KeyCode>>,
    mut state: Option<ResMut<ShowroomState>>,
    mut orbit: Option<ResMut<ShowroomOrbit>>,
    mut turntable: Option<ResMut<ShowroomTurntable>>,
    panel_keyboard: Option<Res<PanelKeyboard>>,
    catalog: Option<Res<ShowroomCatalog>>,
) {
    let Some(mut state) = state else {
        return;
    };
    let Some(catalog) = catalog else {
        return;
    };
    if panel_keyboard.map(|p| p.0).unwrap_or(false) {
        return;
    }
    if keys.just_pressed(KeyCode::KeyR) {
        if let Some(o) = orbit.as_mut() {
            **o = ShowroomOrbit::default();
        }
    }
    if keys.just_pressed(KeyCode::KeyT) {
        if let Some(t) = turntable.as_mut() {
            t.on = !t.on;
        }
    }
    let len = |c: usize| {
        catalog
            .categories
            .get(c)
            .map(|cat| cat.items.len())
            .unwrap_or(0)
    };
    let n_categories = catalog.categories.len().max(1);
    if keys.just_pressed(KeyCode::Tab)
        || keys.just_pressed(KeyCode::KeyG)
        || keys.just_pressed(KeyCode::ArrowLeft)
        || keys.just_pressed(KeyCode::ArrowRight)
    {
        state.compare = None; // browsing leaves compare mode
    }
    if keys.just_pressed(KeyCode::Tab) {
        state.category = (state.category + 1) % n_categories;
        state.item = 0;
    }
    if keys.just_pressed(KeyCode::KeyG) {
        state.gallery = !state.gallery;
        info!("[sm] {} mode", if state.gallery { "gallery" } else { "focus" });
    }
    if keys.just_pressed(KeyCode::ArrowRight) {
        state.item = (state.item + 1) % len(state.category).max(1);
    }
    if keys.just_pressed(KeyCode::ArrowLeft) {
        state.item = (state.item + len(state.category).max(1) - 1) % len(state.category).max(1);
    }
}

/// Case-insensitive substring match over name + tags, flattened across
/// categories (SM6).
fn matching(catalog: &ShowroomCatalog, q: &str) -> Vec<(usize, usize)> {
    catalog
        .categories
        .iter()
        .enumerate()
        .flat_map(|(c, cat)| {
            cat.items
                .iter()
                .enumerate()
                .filter_map(move |(i, it)| {
                    let name_hit = it.name.to_lowercase().contains(q);
                    let tag_hit = it.tags.iter().any(|t| t.to_lowercase().contains(q));
                    (name_hit || tag_hit).then_some((c, i))
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

/// The single stage-swap point: whenever the wanted stage (query/mode/
/// category/item) differs from what is on the floor, despawn everything
/// and rebuild. Focus mode puts ONE exhibit on the pad (search keeps the
/// current item only while it matches, else jumps to the first hit);
/// gallery mode lays the current category — or, with an active search, the
/// flattened hit set — out in a grid.
#[allow(clippy::too_many_arguments)]
pub fn showroom_present(
    mut commands: Commands,
    mut state: Option<ResMut<ShowroomState>>,
    mut orbit: Option<ResMut<ShowroomOrbit>>,
    exhibits: Query<Entity, With<ShowroomExhibit>>,
    catalog: Option<Res<ShowroomCatalog>>,
    query: Option<Res<SearchQuery>>,
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
    let query = query
        .map(|q| q.text.trim().to_lowercase())
        .unwrap_or_default();
    let hits = if query.is_empty() {
        None
    } else {
        Some(matching(&catalog, &query))
    };
    // Search-guided focus: the current item stays only while it matches.
    if let Some(hits) = &hits {
        if !state.gallery && !hits.contains(&(state.category, state.item)) {
            if let Some(&(c, i)) = hits.first() {
                state.select(c, i);
            }
        }
    }
    let wanted = state.wanted_stage(&query);
    if state.stage.as_ref() == Some(&wanted) {
        return;
    }
    state.selected_part = None;
    for e in &exhibits {
        commands.entity(e).despawn();
    }
    let mut ctx = SpawnCtx {
        meshes: &mut meshes,
        materials: &mut materials,
        server: server.as_deref(),
    };
    let plate_font = font.map(|f| f.0.clone()).unwrap_or_default();

    let mut stage: Vec<(usize, usize, &ShowroomItem, Vec3)> = Vec::new();
    if let Some((a, b)) = state.compare {
        // SM8: exactly two entries side by side, plates tagged A/B.
        for (n, (c, i)) in [a, b].iter().enumerate() {
            if let Some(item) = catalog.categories.get(*c).and_then(|cat| cat.items.get(*i)) {
                let x = if n == 0 { -2.4 } else { 2.4 };
                stage.push((*c, *i, item, Vec3::new(x, item.anchor_y, 0.0)));
            }
        }
    } else if state.gallery {
        const SPACING: f32 = 3.4;
        let set: Vec<(usize, usize)> = hits.unwrap_or_else(|| {
            (0..catalog.categories.get(state.category).map(|c| c.items.len()).unwrap_or(0))
                .map(|i| (state.category, i))
                .collect()
        });
        if set.is_empty() {
            return;
        }
        let cols = (set.len() as f32).sqrt().ceil().max(1.0) as usize;
        let rows = (set.len() + cols - 1) / cols;
        for (n, &(c, i)) in set.iter().enumerate() {
            let Some(item) = catalog.categories.get(c).and_then(|cat| cat.items.get(i)) else {
                continue;
            };
            let (col, row) = (n % cols, n / cols);
            let x = (col as f32 - (cols - 1) as f32 / 2.0) * SPACING;
            let z = (row as f32 - (rows - 1) as f32 / 2.0) * SPACING;
            stage.push((c, i, item, Vec3::new(x, item.anchor_y, z)));
        }
        if let Some(orbit) = orbit.as_mut() {
            let extent = cols.max(rows) as f32 * SPACING;
            orbit.dist = (extent * 1.35).clamp(6.5, 15.5);
        }
        info!(
            "[sm] gallery: {} items on stage ({cols}x{rows})",
            set.len()
        );
    } else {
        let Some(item) = catalog
            .categories
            .get(state.category)
            .and_then(|c| c.items.get(state.item))
        else {
            return;
        };
        stage.push((
            state.category,
            state.item,
            item,
            Vec3::new(0.0, item.anchor_y, 0.0),
        ));
    }

    for (n, (cat_idx, idx, item, pos)) in stage.into_iter().enumerate() {
        let mut ec = commands.spawn((
            ShowroomExhibit,
            Visibility::Visible,
            Transform::from_translation(pos),
        ));
        let root = ec.id();
        if state.gallery {
            ec.insert(ShowroomGalleryItem {
                category: cat_idx,
                item: idx,
            });
        }
        if let Some(gltf) = item.gltf.clone() {
            ec.insert(ShowroomAnim {
                gltf,
                index: 0,
                wired: None,
            });
        }
        (item.spawn)(&mut commands, root, &mut ctx);
        // N5-②: mark composition roots; the parts system pairs their
        // top-level children with part indices on first sight.
        if item.parts.len() >= 2 {
            commands.entity(root).insert(ShowroomPartsTag(item.parts.len()));
        }
        // Name plate (item.plate in focus; short name in gallery; A/B in compare).
        let text = if state.compare.is_some() {
            format!("{} · {}", if n == 0 { "A" } else { "B" }, item.name)
        } else if state.gallery {
            item.name.clone()
        } else {
            item.plate.clone().unwrap_or_else(|| item.name.clone())
        };
        let plate_y = item.plate_y;
        let font_size = if state.gallery { 16.0 } else { 22.0 };
        commands.entity(root).with_children(|p| {
            p.spawn((
                ShowroomPlate,
                Text2d::new(text),
                TextFont {
                    font: plate_font.clone(),
                    font_size: FontSize::Px(font_size),
                    ..default()
                },
                TextColor(Color::srgb(0.1, 0.1, 0.12)),
                // Tilt toward the camera so the plate reads upright.
                Transform::from_xyz(0.0, plate_y, 0.0)
                    .with_rotation(Quat::from_rotation_x(-0.67)),
            ));
        });
        if !state.gallery {
            info!("[sm] showing {} · {}", item.name, item.plate.as_deref().unwrap_or(""));
        }
    }
    state.stage = Some(wanted);
}

/// Click-to-focus (gallery mode): a left click on/near an exhibit picks the
/// nearest one on the ground plane and returns to focus mode with it. A
/// plain click drags nothing, so it does not fight the orbit.
pub fn showroom_pick(
    buttons: Res<ButtonInput<bevy::input::mouse::MouseButton>>,
    windows: Query<&Window>,
    camera: Query<(&Camera, &GlobalTransform)>,
    panel_pointer: Option<Res<PanelPointer>>,
    mut state: Option<ResMut<ShowroomState>>,
    gallery_items: Query<(&Transform, &ShowroomGalleryItem), With<ShowroomExhibit>>,
) {
    let Some(mut state) = state else {
        return;
    };
    if !state.gallery {
        return;
    }
    if panel_pointer.map(|p| p.0).unwrap_or(false) {
        return;
    }
    if !buttons.just_pressed(bevy::input::mouse::MouseButton::Left) {
        return;
    }
    // Ground point under the cursor (y=0 plane), same projection the games
    // use for world picking.
    let Ok((cam, cam_tf)) = camera.single() else {
        return;
    };
    let Some(cursor) = windows.single().ok().and_then(|w| w.cursor_position()) else {
        return;
    };
    let Ok(ray) = cam.viewport_to_world(cam_tf, cursor) else {
        return;
    };
    if ray.direction.y.abs() < 1e-6 {
        return;
    }
    let t = -ray.origin.y / ray.direction.y;
    if t < 0.0 {
        return;
    }
    let point = ray.origin + *ray.direction * t;
    let mut best: Option<(f32, usize, usize)> = None;
    for (tf, gi) in &gallery_items {
        let dx = tf.translation.x - point.x;
        let dz = tf.translation.z - point.z;
        let d = dx * dx + dz * dz;
        if d < 1.7_f32 * 1.7 && best.map(|(bd, ..)| d < bd).unwrap_or(true) {
            best = Some((d, gi.category, gi.item));
        }
    }
    if let Some((_, category, item)) = best {
        state.select(category, item);
        state.gallery = false;
        info!("[sm] focus picked from gallery: {category}/{item}");
    }
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

/// SM2: drive the exhibit's animation. On the first wire the clip whose
/// name contains "idle" wins (else clip 0), looping; Space cycles to the
/// next clip. Wiring follows the ART11-proven pattern: wait for the Gltf
/// asset AND the scene's own AnimationPlayer (both land asynchronously),
/// then build a one-clip graph and drive the loader's player.
pub fn showroom_animate(
    keys: Res<ButtonInput<KeyCode>>,
    state: Option<Res<ShowroomState>>,
    mut commands: Commands,
    mut graphs: ResMut<Assets<AnimationGraph>>,
    gltfs: Res<Assets<bevy::gltf::Gltf>>,
    exhibits: Query<(Entity, &mut ShowroomAnim)>,
    children: Query<&Children>,
    mut players: Query<&mut AnimationPlayer>,
) {
    for (root, mut anim) in exhibits {
        // Space cycles clips in focus mode only (a gallery-wide synchronized
        // hop would be noise; gallery items just auto-idle).
        let focus = state.as_ref().map(|s| !s.gallery).unwrap_or(true);
        if focus && keys.just_pressed(KeyCode::Space) && anim.wired != Some(usize::MAX) {
            anim.index += 1;
            anim.wired = None; // rewire with the next clip
        }
        if anim.wired == Some(anim.index) {
            continue;
        }
        let Some(gltf) = gltfs.get(&anim.gltf) else {
            continue; // still loading — retry next frame
        };
        // HashMap — sort by name once per wire for a stable cycle order.
        let mut clips: Vec<(&Box<str>, &Handle<AnimationClip>)> =
            gltf.named_animations.iter().collect();
        clips.sort_by(|a, b| a.0.cmp(b.0));
        if clips.is_empty() {
            anim.wired = Some(usize::MAX); // no clips — stop retrying
            info!("[sm] exhibit has no animation clips");
            continue;
        }
        // First wire prefers an idle-ish clip; Space cycles from there.
        if anim.wired.is_none() && anim.index == 0 {
            if let Some(idle) = clips
                .iter()
                .position(|(name, _)| name.to_lowercase().contains("idle"))
            {
                anim.index = idle;
            }
        }
        let index = anim.index % clips.len();
        let (clip_name, clip) = clips[index];
        // The scene instantiates a frame or more after the assets land —
        // keep retrying until the loader's player exists in the subtree.
        let Some(player_entity) = find_animation_player(root, &children, &players) else {
            continue;
        };
        let (graph, node) = AnimationGraph::from_clip(clip.clone());
        let handle = graphs.add(graph);
        let Ok(mut player) = players.get_mut(player_entity) else {
            continue;
        };
        player.play(node).repeat();
        commands
            .entity(player_entity)
            .insert(AnimationGraphHandle(handle));
        anim.wired = Some(anim.index);
        info!("[sm] clip playing: {clip_name}");
    }
}

/// Depth-first search for the scene's own `AnimationPlayer` below `root`.
fn find_animation_player(
    root: Entity,
    children: &Query<&Children>,
    players: &Query<&mut AnimationPlayer>,
) -> Option<Entity> {
    if players.get(root).is_ok() {
        return Some(root);
    }
    let kids = children.get(root).ok()?;
    kids.iter()
        .find_map(|k| find_animation_player(k, children, players))
}

/// SM5: apply the playback time control to the virtual clock while the
/// studio is active. Pause freezes everything coherently (animations,
/// turntable); `step` lets exactly N frames through; `speed` scales them.
/// Mirrors warden's DB3 mechanism (same clock, same semantics).
/// N5-② part breakdown: pair a tagged composition root's top-level
/// children with part indices, then isolate the selected part (the rest
/// hidden). Fresh exhibits start untagged; children spawn deferred, so
/// pairing runs every frame until done.
pub fn showroom_parts(
    state: Option<Res<ShowroomState>>,
    mut commands: Commands,
    roots: Query<(Entity, &ShowroomPartsTag, &Children)>,
    parts: Query<&ShowroomPart>,
) {
    let Some(state) = state else {
        return;
    };
    for (root, tag, children) in &roots {
        // Pair once: mark every still-unmarked top-level child by order.
        let mut next = 0usize;
        for child in children.iter() {
            if parts.get(child).is_ok() {
                continue;
            }
            if next >= tag.0 {
                break;
            }
            commands.entity(child).insert(ShowroomPart(next));
            next += 1;
        }
        let _ = root;
        // Isolate: only the selected part stays visible.
        for child in children.iter() {
            if let Ok(p) = parts.get(child) {
                let vis = if state.selected_part.map(|s| s != p.0).unwrap_or(false) {
                    Visibility::Hidden
                } else {
                    Visibility::Inherited
                };
                commands.entity(child).insert(vis);
            }
        }
    }
}

pub fn showroom_time_control(
    state: Option<Res<ShowroomState>>,
    mut tm: Option<ResMut<ShowroomTime>>,
    mut virt: ResMut<Time<Virtual>>,
) {
    if state.is_none() {
        return;
    }
    let Some(mut tm) = tm else {
        return;
    };
    if tm.paused {
        if tm.step > 0 {
            tm.step -= 1;
            virt.unpause();
            virt.set_relative_speed(tm.speed);
        } else {
            virt.pause();
        }
    } else {
        virt.unpause();
        virt.set_relative_speed(tm.speed);
    }
}

/// Orbit + zoom camera: left-drag orbits around the exhibit, wheel zooms.
/// Pointer over any egui panel disables both (the panel sets the capture).
pub fn showroom_orbit(
    time: Res<Time>,
    buttons: Res<ButtonInput<bevy::input::mouse::MouseButton>>,
    motion: Res<AccumulatedMouseMotion>,
    mut wheel: MessageReader<MouseWheel>,
    mut panel_pointer: Option<ResMut<PanelPointer>>,
    state: Option<Res<ShowroomState>>,
    turntable: Option<Res<ShowroomTurntable>>,
    mut orbit: Option<ResMut<ShowroomOrbit>>,
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
    if turntable.map(|t| t.on).unwrap_or(false) {
        orbit.yaw_deg += time.delta_secs() * 30.0; // virtual dt: pauses with playback
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

/// Keyboard-focus-on-egui flag (typing in the search box suspends the
/// browse/pick hotkeys).
#[derive(Resource, Default)]
pub struct PanelKeyboard(pub bool);

/// The egui operation panel (SM5 v2): the single complete surface for the
/// tool — search, mode/view controls, playback time control, category list
/// (filtered), the current asset's card (info rows + origins + tags) and
/// the clip browser. Hotkeys stay as accelerators.
pub fn showroom_panel_ui(
    mut contexts: bevy_egui::EguiContexts,
    mut panel_pointer: Option<ResMut<PanelPointer>>,
    mut panel_keyboard: Option<ResMut<PanelKeyboard>>,
    mut state: Option<ResMut<ShowroomState>>,
    mut query: Option<ResMut<SearchQuery>>,
    mut time: Option<ResMut<ShowroomTime>>,
    mut turntable: Option<ResMut<ShowroomTurntable>>,
    mut orbit: Option<ResMut<ShowroomOrbit>>,
    catalog: Option<Res<ShowroomCatalog>>,
    gltfs: Res<Assets<bevy::gltf::Gltf>>,
    mut anims: Query<&mut ShowroomAnim>,
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
    let typing = ctx.egui_wants_keyboard_input();
    if let Some(k) = panel_keyboard.as_mut() {
        k.0 = typing;
    }
    use bevy_egui::egui;
    egui::Window::new("Showroom")
        .anchor(egui::Align2::LEFT_TOP, egui::vec2(0.0, 0.0))
        .movable(false)
        .resizable(false)
        .collapsible(false)
        .default_width(250.0)
        .show(ctx, |ui| {
            ui.heading("Showroom");
            // --- search (SM6) ---
            if let Some(q) = query.as_mut() {
                ui.add(egui::TextEdit::singleline(&mut q.text).hint_text("search name/tags..."));
            }
            // --- mode & view (SM5) ---
            ui.horizontal(|ui| {
                if ui
                    .button(if state.gallery { "Focus [G]" } else { "Gallery [G]" })
                    .clicked()
                {
                    state.gallery = !state.gallery;
                }
                if ui.button("Reset view [R]").clicked() {
                    if let Some(o) = orbit.as_mut() {
                        **o = ShowroomOrbit::default();
                    }
                }
                if let Some(t) = turntable.as_mut() {
                    if ui
                        .button(if t.on { "Turntable ON [T]" } else { "Turntable [T]" })
                        .clicked()
                    {
                        t.on = !t.on;
                    }
                }
            });
            // --- playback time control (SM5) ---
            if let Some(tm) = time.as_mut() {
                ui.horizontal(|ui| {
                    if ui
                        .button(if tm.paused { "Resume" } else { "Pause" })
                        .clicked()
                    {
                        tm.paused = !tm.paused;
                    }
                    if ui.button("Step").clicked() {
                        tm.step = 1;
                    }
                    for s in [0.25, 0.5, 1.0, 2.0] {
                        if ui
                            .selectable_label(tm.speed == s, format!("{s}x"))
                            .clicked()
                        {
                            tm.speed = s;
                        }
                    }
                });
            }
            ui.separator();
            // --- category list / search results (SM6) ---
            let q = query
                .as_ref()
                .map(|q| q.text.trim().to_lowercase())
                .unwrap_or_default();
            if q.is_empty() {
                for (c, cat) in catalog.categories.iter().enumerate() {
                    egui::CollapsingHeader::new(cat.name.clone())
                        .default_open(true)
                        .show(ui, |ui| {
                            for (i, item) in cat.items.iter().enumerate() {
                                let selected = state.category == c && state.item == i;
                                if ui
                                    .selectable_label(selected, format!("  {}", item.name))
                                    .clicked()
                                {
                                    if state.compare_pick_b {
                                        if let Some((a, _)) = state.compare {
                                            state.compare = Some((a, (c, i)));
                                        }
                                        state.compare_pick_b = false;
                                    } else {
                                        state.select(c, i);
                                    }
                                }
                            }
                        });
                }
            } else {
                let hits = matching(&catalog, &q);
                ui.label(format!("Results ({})", hits.len()));
                for &(c, i) in &hits {
                    let name = &catalog.categories[c].items[i].name;
                    let selected = state.category == c && state.item == i;
                    if ui
                        .selectable_label(selected, format!("  {name}"))
                        .clicked()
                    {
                        if state.compare_pick_b {
                            if let Some((a, _)) = state.compare {
                                state.compare = Some((a, (c, i)));
                            }
                            state.compare_pick_b = false;
                        } else {
                            state.select(c, i);
                        }
                    }
                }
            }
            ui.separator();
            // --- asset card (SM6/SM7) ---
            if let Some(item) = catalog
                .categories
                .get(state.category)
                .and_then(|c| c.items.get(state.item))
            {
                ui.label("On the pad");
                for (k, v) in &item.info_rows {
                    ui.horizontal(|ui| {
                        ui.label(k.clone());
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.monospace(v.clone());
                        });
                    });
                }
                if !item.origin.is_empty() {
                    ui.label("Origins");
                    for o in &item.origin {
                        ui.small(o.row());
                    }
                }
                if !item.tags.is_empty() {
                    ui.small(format!("tags: {}", item.tags.join(", ")));
                }
                // N5-② part breakdown: isolate one part of the composition.
                if item.parts.len() >= 2 {
                    ui.label("Parts");
                    ui.horizontal(|ui| {
                        if ui
                            .selectable_label(state.selected_part.is_none(), "All")
                            .clicked()
                        {
                            state.selected_part = None;
                        }
                        for (pi, pname) in item.parts.iter().enumerate() {
                            if ui
                                .selectable_label(state.selected_part == Some(pi), pname.clone())
                                .clicked()
                            {
                                state.selected_part =
                                    if state.selected_part == Some(pi) { None } else { Some(pi) };
                            }
                        }
                    });
                }
                // SM8 compare: A = current entry, B = next list click.
                ui.horizontal(|ui| {
                    if ui.button("Compare: A = current").clicked() {
                        state.compare = Some(((state.category, state.item), (state.category, state.item)));
                        state.gallery = false;
                        state.selected_part = None;
                        state.compare_pick_b = true;
                    }
                    if state.compare_pick_b && ui.button("Cancel pick B").clicked() {
                        state.compare_pick_b = false;
                        state.compare = None;
                    } else if state.compare.is_some() && !state.compare_pick_b && ui.button("Exit compare").clicked() {
                        state.compare = None;
                    }
                });
                if state.compare_pick_b {
                    ui.small("pick B: click any list entry");
                }
                // --- clip browser (SM2) ---
                if let Ok(mut anim) = anims.single_mut() {
                    if let Some(gltf) = gltfs.get(&anim.gltf) {
                        let mut clips: Vec<(&Box<str>, &Handle<AnimationClip>)> =
                            gltf.named_animations.iter().collect();
                        clips.sort_by(|a, b| a.0.cmp(b.0));
                        if clips.is_empty() {
                            ui.small("(no clips)");
                        } else {
                            ui.label("Clips (Space = next)");
                            for (i, (name, _)) in clips.iter().enumerate() {
                                let selected = anim.wired == Some(i) && anim.index == i;
                                if ui
                                    .selectable_label(selected, format!("  {name}"))
                                    .clicked()
                                {
                                    anim.index = i;
                                    anim.wired = None; // rewire
                                }
                            }
                        }
                    }
                }
            }
            ui.separator();
            ui.small("Tab/arrows · G gallery · R reset view · T turntable · drag orbit · wheel zoom");
        });
}
