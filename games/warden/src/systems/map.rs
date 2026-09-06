//! Map: draw the L-shaped path, the base, and the 8 tower slots.
//! Capability cards: MA1 (path+base), MA2 (8 slots).

use bevy::prelude::*;

use crate::components::TowerSlot;
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

pub fn spawn_path(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    path: Res<PathInfo>,
) {
    let mat = materials.add(Color::srgb(0.35, 0.35, 0.42));
    // Draw each axis-aligned segment as a slab. Waypoints are axis-aligned, so
    // orient the box along the dominant axis.
    for w in path.waypoints.windows(2) {
        let a = w[0];
        let b = w[1];
        let dx = (b.x - a.x).abs();
        let dz = (b.z - a.z).abs();
        let len = dx.max(dz);
        let (sx, sz) = if dx > dz { (len, 1.6) } else { (1.6, len) };
        let mid = (a + b) / 2.0;
        commands.spawn((
            Mesh3d(meshes.add(Cuboid::new(sx, 0.1, sz))),
            MeshMaterial3d(mat.clone()),
            Transform::from_xyz(mid.x, 0.05, mid.z),
        ));
    }
    // Base at the final waypoint.
    if let Some(&p) = path.waypoints.last() {
        commands.spawn((
            Mesh3d(meshes.add(Cuboid::new(2.0, 1.2, 2.0))),
            MeshMaterial3d(materials.add(Color::srgb(0.4, 0.3, 0.2))),
            Transform::from_xyz(p.x, 0.6, p.z),
        ));
    }
}

pub fn spawn_slots(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let mat = materials.add(Color::srgb(0.2, 0.4, 0.25));
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
