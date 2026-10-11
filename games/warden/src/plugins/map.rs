//! MapPlugin: spawns the map visuals. Since WX6-6f the map itself is the
//! RON bundle (warden.ron): `load_map` assembles it (path entities, build
//! grid, registry, waves, rules, econ) and derives the warden views
//! (PathInfo); the spawners below are pure presentation on top.
//! Capability cards: MA1 (path+base), MA2 (8 slots).
//! WX6-1: the path also lives as a base `PathDef` entity (the `Paths`
//! resource points at it) — enemy movement runs on the base's
//! `advance_followers`. `PathInfo` stays as the warden-side view (visual
//! slabs, spawn anchoring) derived from the same waypoints.

use bevy::prelude::*;

use crate::systems;

pub struct MapPlugin;

impl Plugin for MapPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<warbase::prelude::Arrived>()
            .add_message::<warbase::prelude::LapCompleted>()
            .add_systems(
                Startup,
                (
                    // WX6-6f: the RON bundle is the map — load + validate +
                    // assemble first; the visual spawners read its derived
                    // resources (PathInfo, BuildGrid).
                    systems::map::load_map,
                    systems::map::spawn_ground,
                    systems::map::spawn_path,
                    systems::map::spawn_slots,
                    systems::map::spawn_props,
                    systems::map::spawn_structures,
                )
                    .chain(),
            );
    }
}
