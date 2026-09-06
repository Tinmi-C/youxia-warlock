//! EnemiesPlugin: enemy definitions + movement/leak and kill/gold systems.
//! Capability cards: EN1 (defs), EN2 (move/leak), EN3 (death/gold).

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
                systems::enemy::move_enemy
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
