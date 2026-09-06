//! MapPlugin: L-shaped path + base + 8 tower slots. Inserts the path geometry
//! and the base HP resource, and spawns the map visuals.
//! Capability cards: MA1 (path+base), MA2 (8 slots).

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
        app.insert_resource(PathInfo { waypoints })
            .init_resource::<BaseHp>()
            .add_systems(
                Startup,
                (systems::map::spawn_path, systems::map::spawn_slots),
            );
    }
}
