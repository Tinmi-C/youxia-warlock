//! Map: the arena terrain, the L-shaped path, the base, and the 8 tower slots.
//! Capability cards: MA1 (path+base), MA2 (8 slots), ART3 (terrain + paving).

use bevy::image::{ImageLoaderSettings, ImageSampler, ImageSamplerDescriptor};
use bevy::math::Affine2;
use bevy::prelude::*;
use bevy::render::render_resource::AddressMode;

use crate::components::{MapGround, MapProp, MapStructure, TowerSlot};
use crate::resources::PathInfo;

/// Tower slot positions (XZ), placed beside the path. The path is an L:
/// entry (-13,9) -> (-4,9) -> (-4,-9) -> base (12,-9).
const SLOT_POSITIONS: [(f32, f32); 8] = [
    (-11.5, 11.0),
    (-7.5, 11.0),
    (-6.0, 6.0),
    (-6.0, 0.0),
    (-6.0, -6.0),
    (-1.5, -11.0),
    (4.0, -11.0),
    (9.0, -11.0),
];

/// ART3 arena diorama (direction B style preview): a 31 x 25.5 top face
/// centered on the play area with a 2.5-unit cliff skirt below. Purely
/// visual — nothing in gameplay reads these meshes (picking projects slot
/// positions, never raycasts the terrain).
const GROUND_HALF: Vec2 = Vec2::new(31.0 / 2.0, 25.5 / 2.0);
const GROUND_CENTER: (f32, f32) = (-0.5, 0.0);
const SKIRT_DEPTH: f32 = 2.5;
/// Ground top sits just under the path slabs (slab bottoms are at y = 0).
const GROUND_TOP_Y: f32 = -0.01;

/// A tiling sampler: UV repeats need Repeat address modes, but the default
/// image sampler clamps (verified against bevy 0.19.1 — no `repeat()` helper).
fn tiling_sampler() -> ImageSampler {
    ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: AddressMode::Repeat.into(),
        address_mode_v: AddressMode::Repeat.into(),
        ..Default::default()
    })
}

fn load_tiling(server: &AssetServer, path: &str) -> Handle<Image> {
    server
        .load_builder()
        .with_settings(|s: &mut ImageLoaderSettings| {
            s.sampler = tiling_sampler();
        })
        .load(path.to_owned())
}

fn flat_mat(materials: &mut Assets<StandardMaterial>, color: Color) -> Handle<StandardMaterial> {
    materials.add(StandardMaterial {
        base_color: color,
        perceptual_roughness: 1.0,
        ..default()
    })
}

/// ART3: the arena terrain — textured top face + cliff skirt. Without an
/// AssetServer (headless regression worlds) both fall back to flat
/// direction-B palette colors (docs/art-direction.md), so gameplay tests
/// stay art-independent.
pub fn spawn_ground(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    server: Option<Res<AssetServer>>,
) {
    let (cx, cz) = GROUND_CENTER;
    // Cliff skirt: the diorama edge from the style preview.
    commands.spawn((
        Mesh3d(meshes.add(Cuboid::new(
            GROUND_HALF.x * 2.0,
            SKIRT_DEPTH,
            GROUND_HALF.y * 2.0,
        ))),
        MeshMaterial3d(flat_mat(
            &mut materials,
            Color::srgb(0.24, 0.16, 0.11),
        )),
        Transform::from_xyz(cx, GROUND_TOP_Y - SKIRT_DEPTH / 2.0, cz),
    ));
    let top = match server.as_deref() {
        Some(server) => materials.add(StandardMaterial {
            base_color_texture: Some(load_tiling(
                server,
                "textures/ground_wasteland_dirt_diff_1k.jpg",
            )),
            normal_map_texture: Some(load_tiling(
                server,
                "textures/ground_wasteland_dirt_nor_gl_1k.jpg",
            )),
            uv_transform: Affine2::from_scale(Vec2::new(10.0, 8.0)),
            perceptual_roughness: 1.0,
            ..default()
        }),
        None => flat_mat(&mut materials, Color::srgb(0.42, 0.32, 0.19)),
    };
    commands.spawn((
        MapGround,
        Mesh3d(meshes.add(Plane3d::new(Vec3::Y, GROUND_HALF))),
        MeshMaterial3d(top),
        Transform::from_xyz(cx, GROUND_TOP_Y, cz),
    ));
}

pub fn spawn_path(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    path: Res<PathInfo>,
    server: Option<Res<AssetServer>>,
) {
    // Draw each axis-aligned segment as a slab. Waypoints are axis-aligned, so
    // orient the box along the dominant axis. ART3: cobblestone paving, ~2
    // units per tile, repeats rounded so slab seams land near tile boundaries.
    for w in path.waypoints.windows(2) {
        let a = w[0];
        let b = w[1];
        let dx = (b.x - a.x).abs();
        let dz = (b.z - a.z).abs();
        let len = dx.max(dz);
        let (sx, sz) = if dx > dz { (len, 1.6) } else { (1.6, len) };
        let mid = (a + b) / 2.0;
        let mat = match server.as_deref() {
            Some(server) => materials.add(StandardMaterial {
                base_color_texture: Some(load_tiling(
                    server,
                    "textures/path_cobblestone_diff_1k.jpg",
                )),
                normal_map_texture: Some(load_tiling(
                    server,
                    "textures/path_cobblestone_nor_gl_1k.jpg",
                )),
                uv_transform: Affine2::from_scale(Vec2::new(
                    (sx / 2.0).round().max(1.0),
                    (sz / 2.0).round().max(1.0),
                )),
                perceptual_roughness: 1.0,
                ..default()
            }),
            None => flat_mat(&mut materials, Color::srgb(0.42, 0.39, 0.35)),
        };
        commands.spawn((
            Mesh3d(meshes.add(Cuboid::new(sx, 0.1, sz))),
            MeshMaterial3d(mat),
            Transform::from_xyz(mid.x, 0.05, mid.z),
        ));
    }
    // The base waypoint visual lives in `spawn_structures` since ART4
    // (gatehouse GLB / massing fallback); enemies walk waypoints either way.
}

/// ART4: masonry structures — the gatehouse on the base waypoint and the
/// entry arch straddling the path start. Vault GLBs when an AssetServer
/// exists; headless keeps one B-palette massing block per structure so both
/// worlds carry exactly two `MapStructure` roots. Purely visual (BaseHp is a
/// resource; enemies walk waypoints, meshes carry no gameplay).
pub fn spawn_structures(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    path: Res<PathInfo>,
    server: Option<Res<AssetServer>>,
) {
    let (Some(&base), Some(&entry)) = (path.waypoints.last(), path.waypoints.first()) else {
        return;
    };
    match server.as_deref() {
        Some(server) => {
            // Gatehouse: local passage runs along X, so no rotation faces the
            // arriving path; centered on the endpoint so enemies walk into
            // the gate tunnel. Entry arch: rotated a quarter turn so its
            // pillars straddle the path instead of sitting on it.
            for (model, tf) in [
                (
                    "models/structure_gatehouse.glb",
                    Transform::from_translation(base),
                ),
                (
                    "models/structure_entry_arch.glb",
                    Transform::from_translation(entry)
                        .with_rotation(Quat::from_rotation_y(std::f32::consts::FRAC_PI_2)),
                ),
            ] {
                let scene =
                    server.load(bevy::gltf::GltfAssetLabel::Scene(0).from_asset(model));
                commands.spawn((MapStructure, bevy::world_serialization::WorldAssetRoot(scene), tf));
            }
        }
        None => {
            commands.spawn((
                MapStructure,
                Mesh3d(meshes.add(Cuboid::new(2.0, 1.2, 2.0))),
                MeshMaterial3d(flat_mat(&mut materials, Color::srgb(0.34, 0.30, 0.27))),
                Transform::from_xyz(base.x, 0.6, base.z),
            ));
            commands.spawn((
                MapStructure,
                Mesh3d(meshes.add(Cuboid::new(0.7, 2.6, 3.0))),
                MeshMaterial3d(flat_mat(&mut materials, Color::srgb(0.26, 0.23, 0.20))),
                Transform::from_xyz(entry.x, 1.3, entry.z),
            ));
        }
    }
}

pub fn spawn_slots(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // ART3: B-palette pale stone pads (docs/art-direction.md).
    let mat = flat_mat(&mut materials, Color::srgb(0.48, 0.44, 0.39));
    for (i, (x, z)) in SLOT_POSITIONS.iter().enumerate() {
        commands.spawn((
            TowerSlot {
                index: i,
                occupied: false,
            },
            Mesh3d(meshes.add(Cuboid::new(1.25, 0.2, 1.25))),
            MeshMaterial3d(mat.clone()),
            Transform::from_xyz(*x, 0.1, *z),
        ));
    }
}

/// ART5: wasteland scatter kinds. One GLB per kind from the vault (PolyHaven
/// CC0, origin normalized to bbox bottom-center by pack_gltf_folder.py), so
/// engine placement is a plain ground translation + scale.
#[derive(Clone, Copy)]
enum PropKind {
    Tree,
    Boulder,
    Stump,
    Branches,
}

impl PropKind {
    fn model(self) -> &'static str {
        match self {
            PropKind::Tree => "models/prop_dead_quiver_trunk.glb",
            PropKind::Boulder => "models/prop_boulder_01.glb",
            PropKind::Stump => "models/prop_tree_stump_01.glb",
            PropKind::Branches => "models/prop_dry_branches_medium_01.glb",
        }
    }

    /// B-palette flat fallback for headless worlds (docs/art-direction.md).
    fn fallback_color(self) -> Color {
        match self {
            PropKind::Tree | PropKind::Branches => Color::srgb(0.26, 0.20, 0.14),
            PropKind::Boulder | PropKind::Stump => Color::srgb(0.40, 0.33, 0.26),
        }
    }
}

/// (kind, x, z, rot_y, uniform scale) — positions mirror the style-preview
/// dressing and keep clear of the path bands (z = ±9 / x = −4 rows), every
/// slot pad, the base, and the entry arch (readability rule, art-direction
/// §3). Purely visual; scale = target size ÷ native bbox (tree native 1.99 m).
const PROP_LAYOUT: [(PropKind, f32, f32, f32, f32); 12] = [
    (PropKind::Tree, -14.6, -3.4, 0.4, 1.30),
    (PropKind::Tree, -14.4, 5.0, 2.1, 1.10),
    (PropKind::Tree, 14.3, 4.2, 4.0, 1.45),
    (PropKind::Tree, 13.8, -3.6, 5.3, 1.20),
    (PropKind::Boulder, -9.2, -3.8, 1.0, 1.00),
    (PropKind::Boulder, 2.6, 4.2, 2.6, 1.20),
    (PropKind::Boulder, 12.2, 6.4, 0.7, 1.10),
    (PropKind::Stump, 4.6, -1.4, 0.0, 1.00),
    (PropKind::Stump, -8.5, -5.4, 1.9, 0.80),
    (PropKind::Branches, -2.0, -5.6, 0.8, 1.20),
    (PropKind::Branches, 5.6, 6.4, 3.4, 1.30),
    (PropKind::Branches, -11.8, -6.8, 5.0, 1.40),
];

/// ART5: scatter the wasteland dressing. With an AssetServer each instance
/// is the vault GLB (shared handle per kind); headless keeps world-shape
/// parity with a B-palette box so pin tests see the same instance count.
pub fn spawn_props(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    server: Option<Res<AssetServer>>,
) {
    for (kind, x, z, rot, s) in PROP_LAYOUT {
        let tf = Transform::from_xyz(x, 0.0, z)
            .with_rotation(Quat::from_rotation_y(rot))
            .with_scale(Vec3::splat(s));
        match server.as_deref() {
            Some(server) => {
                let scene = server
                    .load(bevy::gltf::GltfAssetLabel::Scene(0).from_asset(kind.model()));
                commands.spawn((MapProp, bevy::world_serialization::WorldAssetRoot(scene), tf));
            }
            None => {
                commands.spawn((
                    MapProp,
                    Mesh3d(meshes.add(Cuboid::new(0.6, 1.0, 0.6))),
                    MeshMaterial3d(flat_mat(&mut materials, kind.fallback_color())),
                    tf,
                ));
            }
        }
    }
}
