//! Greybox visuals (real game only — headless tests never add this plugin):
//! ground plane, lane stripes, deploy-zone pads, unit cubes by def.
//! Everything bevy-native, zero assets (D9). 视觉未过人工验收（H1/H3）。

use bevy::prelude::*;
use warbase::prelude::*;

use crate::components::Visualized;

/// Marker on a unit's body-cube child, linking it back to the unit.
#[derive(Component, Debug)]
pub struct UnitBody {
    pub unit: Entity,
}

/// Greybox visual spec per def: (cube edge, color).
fn def_visual(def: &str) -> (f32, Color) {
    match def {
        "keep" => (2.2, Color::srgb(0.45, 0.42, 0.38)),
        "merc_cheap" => (0.55, Color::srgb(0.80, 0.80, 0.55)),
        "merc_swarm" => (0.42, Color::srgb(0.95, 0.75, 0.30)),
        "merc_tank" => (0.85, Color::srgb(0.55, 0.60, 0.70)),
        "merc_ranged" => (0.55, Color::srgb(0.60, 0.85, 0.50)),
        "merc_aoe" => (0.62, Color::srgb(0.85, 0.45, 0.25)),
        "merc_support" => (0.55, Color::srgb(0.55, 0.85, 0.85)),
        _ => (0.60, Color::srgb(0.80, 0.80, 0.80)),
    }
}

/// Static scene: ground, four lane stripes (team-tinted), deploy-zone pads.
/// Lane/slot numbers are read from the assembled map (CurrentMap) — map
/// content drives the greybox, nothing is duplicated here.
pub fn spawn_scene(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    map: Option<Res<crate::resources::CurrentMap>>,
) {
    // Ground covers the arena plus a margin.
    commands.spawn((
        Mesh3d(meshes.add(Plane3d::default().mesh().size(34.0, 20.0))),
        MeshMaterial3d(materials.add(Color::srgb(0.13, 0.17, 0.13))),
    ));
    let Some(map) = map else { return };

    // Lane stripes: flat cuboids from path start to end, tinted by the
    // marching player's team (lanes 0/2 = team 0 blue-ish, 1/3 = team 1 red-ish).
    for (i, path) in map.0.world.paths.iter().enumerate() {
        let a = path.points.first().map(|p| Vec2::new(p[0], p[1])).unwrap_or_default();
        let b = path.points.last().map(|p| Vec2::new(p[0], p[1])).unwrap_or_default();
        let mid = (a + b) / 2.0;
        let len = (b - a).length();
        let color = if i % 2 == 0 {
            Color::srgb(0.16, 0.26, 0.40)
        } else {
            Color::srgb(0.40, 0.20, 0.16)
        };
        commands.spawn((
            Mesh3d(meshes.add(Cuboid::new(len, 0.04, 0.9))),
            MeshMaterial3d(materials.add(color)),
            Transform::from_xyz(mid.x, 0.02, mid.y),
        ));
    }

    // Deploy-zone pads from the FixedSlots build def (48 cells, 12 per lane).
    if let warbase::bundle::BuildDomainDef::FixedSlots { slots } = &map.0.world.build {
        let pad = materials.add(Color::srgb(0.22, 0.30, 0.22));
        let mesh = meshes.add(Cuboid::new(0.86, 0.03, 0.86));
        for &[x, z] in slots {
            commands.spawn((
                Mesh3d(mesh.clone()),
                MeshMaterial3d(pad.clone()),
                Transform::from_xyz(x, 0.015, z),
            ));
        }
    }
}

/// Attach a body cube to every unit that doesn't have one yet. The mesh is a
/// CHILD at local y = size/2 — the unit's own Transform stays on the y=0
/// gameplay plane forever (MoveAgent waypoints are y=0; lifting the body
/// would fight locomotion).
pub fn visualize_units(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    units: Query<(Entity, &UnitInstance), Without<Visualized>>,
) {
    for (entity, inst) in &units {
        let (size, color) = def_visual(&inst.def);
        commands
            .entity(entity)
            .insert(Visualized)
            .with_child((
                Mesh3d(meshes.add(Cuboid::new(size, size, size))),
                MeshMaterial3d(materials.add(StandardMaterial { base_color: color, ..default() })),
                Transform::from_xyz(0.0, size / 2.0, 0.0),
                UnitBody { unit: entity },
            ));
    }
}
