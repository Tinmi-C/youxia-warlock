//! Map: the arena terrain, the L-shaped path, the base, and the 8 tower slots.
//! Capability cards: MA1 (path+base), MA2 (8 slots), ART3 (terrain + paving).

use bevy::image::{ImageLoaderSettings, ImageSampler, ImageSamplerDescriptor};
use bevy::math::Affine2;
use bevy::prelude::*;
use bevy::render::render_resource::AddressMode;

use crate::components::{MapGround, TowerSlot};
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
    // Base at the final waypoint (B-palette stone; gatehouse model = ART4).
    if let Some(&p) = path.waypoints.last() {
        commands.spawn((
            Mesh3d(meshes.add(Cuboid::new(2.0, 1.2, 2.0))),
            MeshMaterial3d(flat_mat(
                &mut materials,
                Color::srgb(0.34, 0.30, 0.27),
            )),
            Transform::from_xyz(p.x, 0.6, p.z),
        ));
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
