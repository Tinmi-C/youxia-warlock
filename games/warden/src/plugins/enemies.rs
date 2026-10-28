//! EnemiesPlugin: enemy definitions + movement/leak + kill/gold + healer aura.
//! Capability cards: EN1 (defs), EN2 (move/leak), EN3 (death/gold), EN5 (healer).
//! WX6-1: the walk itself is the base's `advance_followers`; warden brackets
//! it with the slow-factor sync (before) and the facing + leak systems
//! (after, same frame).

use bevy::prelude::*;

use crate::resources::EnemyDefs;
use crate::sets::GameSet;
use crate::states::GameState;
use crate::systems;

pub struct EnemiesPlugin;

impl Plugin for EnemiesPlugin {
    fn build(&self, app: &mut App) {
        let defs = EnemyDefs::palette();
        app.insert_resource(systems::damage::warden_armor_table())
        // WX6-6c: ONE registry for the whole game — enemy rows (spawn +
        // attribution) plus tower rows (card / balance / RON data layer).
        .insert_resource(systems::enemy::warden_registry(
            &defs,
            &crate::resources::TowerDefs::palette(),
            &crate::resources::FusionDefs::palette(),
        ))
        .insert_resource(defs)
            .add_systems(
                Update,
                (
                    // Roster v2: the base splash seam (`area_targets`) answers
                    // through the Hostility oracle, which reads `Allegiance`
                    // — every warden Enemy is an attacker by definition, so
                    // this health-check backfills the marker on any enemy
                    // that entered the world without it (the spawn gateway
                    // always attaches one; synthetic/test spawns may not).
                    // No-op after the first frame in production.
                    systems::enemy::ensure_enemy_allegiance,
                    systems::enemy::sync_move_speed,
                    warbase::prelude::advance_followers,
                    systems::enemy::face_movement,
                    systems::enemy::leak_arrived,
                )
                    .chain()
                    .in_set(GameSet::Simulate)
                    .run_if(in_state(GameState::Playing)),
            )
            // ART11: wire walk/run loops once each model's Gltf is loaded.
            .add_systems(
                Update,
                systems::enemy::start_enemy_walk
                    .in_set(GameSet::Simulate)
                    .run_if(in_state(GameState::Playing)),
            )
            .add_systems(
                Update,
                systems::enemy::heal_aura
                    .in_set(GameSet::Simulate)
                    .run_if(in_state(GameState::Playing)),
            )
            .add_systems(
                Update,
                systems::enemy::resolve_death
                    .in_set(GameSet::Cleanup)
                    .run_if(in_state(GameState::Playing)),
            );
    }
}
