//! Greybox visuals (real game only — headless tests never add this plugin):
//! ground plane, unit meshes by def, square selection outlines (4 beams),
//! fog-of-war tinting (RL3), and the minimap panel. Everything bevy-native,
//! zero assets (D9). 视觉未过人工验收（H1/H3）。

use bevy::prelude::*;
use warbase::prelude::*;

use crate::components::{Carrying, SelectionOutline, Visualized};

/// Marker on a unit's body-cube child, linking it back to the unit and
/// remembering its authored base color (fog tint restore).
#[derive(Component, Debug)]
pub struct UnitBody {
    pub unit: Entity,
    pub base: Color,
    /// Current dim state (skip no-op material rewrites).
    pub dimmed: bool,
}

/// Graybox visual spec per def: (cube edge, color).
fn def_visual(def: &str) -> (f32, Color) {
    match def {
        "main_hall" => (2.2, Color::srgb(0.20, 0.45, 0.95)),
        "barracks" => (1.6, Color::srgb(0.15, 0.35, 0.75)),
        "gold_mine" => (1.8, Color::srgb(0.95, 0.72, 0.15)),
        "peasant" => (0.55, Color::srgb(0.55, 0.80, 1.00)),
        "footman" => (0.65, Color::srgb(0.85, 0.85, 0.55)),
        "archer" => (0.55, Color::srgb(0.60, 0.85, 0.50)),
        "paladin" => (0.85, Color::srgb(1.00, 0.90, 0.40)), // hero: bigger, gold
        "shop" => (1.4, Color::srgb(0.55, 0.30, 0.70)),
        "grunt" => (0.70, Color::srgb(0.80, 0.18, 0.15)),
        _ => (0.60, Color::srgb(0.80, 0.80, 0.80)),
    }
}

pub fn spawn_scene(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // Ground covers the 24x24 arena plus a margin (border cells are walls).
    commands.spawn((
        Mesh3d(meshes.add(Plane3d::default().mesh().size(26.0, 26.0))),
        MeshMaterial3d(materials.add(Color::srgb(0.13, 0.17, 0.13))),
    ));
    // Perimeter wall stripe (visual twin of the blocked border cells).
    let wall = materials.add(Color::srgb(0.28, 0.30, 0.33));
    for (x, z, w, d) in [
        (0.0, -12.0, 25.2, 0.8),
        (0.0, 12.0, 25.2, 0.8),
        (-12.0, 0.0, 0.8, 25.2),
        (12.0, 0.0, 0.8, 25.2),
    ] {
        commands.spawn((
            Mesh3d(meshes.add(Cuboid::new(w, 0.5, d))),
            MeshMaterial3d(wall.clone()),
            Transform::from_xyz(x, 0.25, z),
        ));
    }
}

/// Attach a body cube to every unit that doesn't have one yet. The mesh is
/// a CHILD at local y = size/2 — the unit's own Transform stays on the y=0
/// gameplay plane forever (MoveAgent waypoints are y=0; lifting the body
/// would fight locomotion and sink it back). Carrying peasants get a gold
/// cap cube so the loop is visible at a glance.
pub fn visualize_units(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    units: Query<(Entity, &UnitInstance, Option<&Carrying>), Without<Visualized>>,
) {
    for (entity, inst, carrying) in &units {
        let (size, color) = def_visual(&inst.def);
        let mut ec = commands.entity(entity);
        ec.insert(Visualized)
            .with_child((
                Mesh3d(meshes.add(Cuboid::new(size, size, size))),
                MeshMaterial3d(materials.add(StandardMaterial { base_color: color, ..default() })),
                Transform::from_xyz(0.0, size / 2.0, 0.0),
                UnitBody { unit: entity, base: color, dimmed: false },
            ));
        if carrying.is_some() {
            // Gold cap riding on top: the "full hands" tell for H1.
            ec.with_child((
                Mesh3d(meshes.add(Cuboid::new(size * 0.5, 0.12, size * 0.5))),
                MeshMaterial3d(materials.add(Color::srgb(1.0, 0.85, 0.2))),
                Transform::from_xyz(0.0, size + 0.06, 0.0),
            ));
        }
    }
}

/// Fog-of-war tint (RL3, D5 "rendering consumes the mark downstream"): an
/// ENEMY unit carrying `InFog` dims to 35% brightness + translucency; our
/// own units stay bright even in dark cells (己方永亮 — the base marks
/// every UnitInstance, ours included; the friendly filter is game-side).
/// Each body cube owns a unique material asset (materials.add per spawn),
/// so mutating the asset can never leak across units.
pub fn sync_fog_tint(
    units: Query<(Entity, &Allegiance, Has<InFog>)>,
    mut bodies: Query<(&mut UnitBody, &MeshMaterial3d<StandardMaterial>)>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for (mut body, handle) in &mut bodies {
        let Ok((_, side, fog)) = units.get(body.unit) else { continue };
        let dim = fog && matches!(side, Allegiance::Attacker);
        if dim == body.dimmed {
            continue;
        }
        body.dimmed = dim;
        if let Some(mut mat) = materials.get_mut(&handle.0) {
            if dim {
                let b = body.base.to_srgba();
                mat.base_color =
                    Color::srgba(b.red * 0.35, b.green * 0.35, b.blue * 0.35, 0.35);
                mat.alpha_mode = AlphaMode::Blend;
            } else {
                mat.base_color = body.base;
                mat.alpha_mode = AlphaMode::Opaque;
            }
        }
    }
}

/// Ground items (RL2): a small green flask-cube riding on every fresh
/// `GroundItem`; pickup/drop despawns the entity, children go with it.
pub fn visualize_ground_items(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    items: Query<Entity, (With<GroundItem>, Without<Visualized>)>,
) {
    for entity in &items {
        commands
            .entity(entity)
            .insert(Visualized)
            .with_child((
                Mesh3d(meshes.add(Cuboid::new(0.3, 0.35, 0.3))),
                MeshMaterial3d(materials.add(StandardMaterial {
                    base_color: Color::srgb(0.2, 0.95, 0.4),
                    emissive: LinearRgba::rgb(0.05, 0.4, 0.1),
                    ..default()
                })),
                Transform::from_xyz(0.0, 0.18, 0.0),
            ));
    }
}

/// Square selection outline = 4 ground beams. They are INDEPENDENT entities
/// tagged with the unit they follow (no parent/child bookkeeping); each frame
/// beams of still-selected units copy x/z, beams of deselected/gone units
/// despawn, and freshly selected units spawn their four beams.
pub fn sync_selection_outlines(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    selected: Query<(Entity, &Transform, &UnitInstance), With<Selected>>,
    // Without<Selected> keeps this query disjoint from the one above (B0001):
    // beams write Transform, selected units only read it.
    mut outlines: Query<(Entity, &SelectionOutline, &mut Transform), Without<Selected>>,
    registry: Res<UnitRegistry>,
) {
    // Sync + reap.
    let mut followed: Vec<Entity> = Vec::new();
    for (beam, outline, mut tf) in &mut outlines {
        match selected.get(outline.unit) {
            Ok((_, unit_tf, _)) => {
                followed.push(outline.unit);
                tf.translation.x = unit_tf.translation.x;
                tf.translation.z = unit_tf.translation.z;
                tf.translation.y = 0.04;
            }
            Err(_) => commands.entity(beam).despawn(),
        }
    }
    // Spawn beams for newly selected units.
    for (unit, tf, inst) in &selected {
        if followed.contains(&unit) {
            continue;
        }
        let radius = registry.get(&inst.def).map(|d| d.attrs.body_radius).unwrap_or(0.3);
        let side = radius * 2.0 + 0.3;
        let half = side / 2.0 + 0.08;
        let ns = meshes.add(Cuboid::new(side + 0.32, 0.04, 0.08));
        let ew = meshes.add(Cuboid::new(0.08, 0.04, side + 0.32));
        let mat = materials.add(StandardMaterial {
            base_color: Color::srgb(0.2, 1.0, 0.4),
            emissive: LinearRgba::rgb(0.05, 0.55, 0.12),
            ..default()
        });
        for (dx, dz, mesh) in [
            (0.0, half, ns.clone()),
            (0.0, -half, ns),
            (half, 0.0, ew.clone()),
            (-half, 0.0, ew),
        ] {
            commands.spawn((
                SelectionOutline { unit },
                Mesh3d(mesh),
                MeshMaterial3d(mat.clone()),
                Transform::from_xyz(tf.translation.x + dx, 0.04, tf.translation.z + dz),
            ));
        }
    }
}
