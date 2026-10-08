//! Greybox visuals (real game only — headless tests never add this plugin):
//! ground, path ring, unit meshes by def, build-cursor highlight.
//! 视觉未过人工验收（H3 关卡后置），先灰盒可玩。

use bevy::prelude::*;
use warbase::prelude::*;

use crate::systems::gameplay::BuildCursor;

#[derive(Component)]
pub struct CursorHighlight;

#[derive(Component)]
pub struct Visualized;

fn def_color(def: &str) -> Color {
    match def {
        "arrow" | "grunt" => Color::srgb(0.3, 0.6, 1.0),
        "cannon" | "tank" => Color::srgb(0.9, 0.4, 0.2),
        "mage" => Color::srgb(0.7, 0.4, 0.9),
        "sentry" | "fast" => Color::srgb(0.3, 0.8, 0.4),
        "flyer" => Color::srgb(0.9, 0.8, 0.3),
        "boss" => Color::srgb(0.6, 0.1, 0.1),
        _ => Color::srgb(0.8, 0.8, 0.8),
    }
}

pub fn spawn_scene(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    map: Option<Res<crate::systems::gameplay::CurrentMap>>,
) {
    commands.spawn((
        Mesh3d(meshes.add(Plane3d::default().mesh().size(40.0, 26.0))),
        MeshMaterial3d(materials.add(Color::srgb(0.16, 0.2, 0.16))),
    ));
    commands.spawn((
        CursorHighlight,
        Mesh3d(meshes.add(Cuboid::new(0.9, 0.05, 0.9))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(1.0, 0.9, 0.2),
            emissive: LinearRgba::rgb(0.4, 0.35, 0.0),
            ..default()
        })),
        Transform::from_xyz(0.0, 0.05, 0.0),
    ));
    if let Some(map) = map {
        let grey = materials.add(Color::srgb(0.42, 0.42, 0.46));
        for path in &map.0.world.paths {
            let len = path.length();
            let mut d = 0.0;
            while d < len {
                let p = path.sample(d);
                commands.spawn((
                    Mesh3d(meshes.add(Cuboid::new(0.92, 0.02, 0.92))),
                    MeshMaterial3d(grey.clone()),
                    Transform::from_xyz(p.x, 0.01, p.y),
                ));
                d += 0.5;
            }
        }
    }
}

pub fn move_cursor_highlight(
    grid: Res<BuildGrid>,
    cursor: Res<BuildCursor>,
    mut q: Query<&mut Transform, With<CursorHighlight>>,
) {
    if let Ok(mut tf) = q.single_mut() {
        let c = grid.cell_center(cursor.0);
        tf.translation.x = c.x;
        tf.translation.z = c.y;
    }
}

/// Attach a mesh to every unit that doesn't have one yet (towers = cubes,
/// creeps = spheres) and lift the render y once — gameplay keeps writing
/// only x/z afterwards.
pub fn visualize_units(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut units: Query<(Entity, &UnitInstance, &Allegiance, &mut Transform), Without<Visualized>>,
) {
    for (e, inst, side, mut tf) in &mut units {
        let color = def_color(&inst.def);
        let mesh = if *side == Allegiance::Defender {
            meshes.add(Cuboid::new(0.7, 0.7, 0.7))
        } else {
            meshes.add(Sphere::new(0.3))
        };
        tf.translation.y = 0.35;
        commands.entity(e).insert((
            Mesh3d(mesh),
            MeshMaterial3d(materials.add(StandardMaterial { base_color: color, ..default() })),
            Visualized,
        ));
    }
}
