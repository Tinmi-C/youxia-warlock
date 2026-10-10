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
        app.insert_resource(EnemyDefs::palette())
            .add_systems(
                Update,
                (
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
