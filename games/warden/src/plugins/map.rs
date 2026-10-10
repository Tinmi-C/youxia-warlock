//! MapPlugin: L-shaped path + base + 8 tower slots. Inserts the path geometry
//! and the base HP resource, and spawns the map visuals.
//! Capability cards: MA1 (path+base), MA2 (8 slots).
//! WX6-1: the path also lives as a base `PathDef` entity (the `Paths`
//! resource points at it) — enemy movement runs on the base's
//! `advance_followers`. `PathInfo` stays as the warden-side view (visual
//! slabs, spawn anchoring) derived from the same waypoints.

use bevy::prelude::*;

use crate::resources::{BaseHp, PathInfo};
use crate::systems;

pub struct MapPlugin;

impl Plugin for MapPlugin {
    fn build(&self, app: &mut App) {
        // L-shaped path (entry top-left -> base bottom-right). Waypoints are
        // axis-aligned so map.rs draws slabs along the dominant axis.
        let waypoints = vec![
            Vec3::new(-13.0, 0.0, 9.0),
            Vec3::new(-4.0, 0.0, 9.0),
            Vec3::new(-4.0, 0.0, -9.0),
            Vec3::new(12.0, 0.0, -9.0),
        ];
        // The same polyline as the base's open-chain PathDef (x,z pairs).
        let path_def = warbase::prelude::PathDef::open(
            waypoints
                .iter()
                .map(|w| [w.x, w.z])
                .collect(),
        );
        let path_e = app
            .world_mut()
            .spawn((path_def, Transform::default()))
            .id();
        app.insert_resource(PathInfo { waypoints })
            .insert_resource(warbase::prelude::Paths(vec![path_e]))
            .add_message::<warbase::prelude::Arrived>()
            .add_message::<warbase::prelude::LapCompleted>()
            .init_resource::<BaseHp>()
            .add_systems(
                Startup,
                (
                    systems::map::spawn_ground,
                    systems::map::spawn_path,
                    systems::map::spawn_slots,
                    systems::map::spawn_props,
                    systems::map::spawn_structures,
                ),
            );
    }
}
