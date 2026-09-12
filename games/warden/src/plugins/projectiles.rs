//! ProjectilesPlugin: rule-layer homing rounds (PR1/PR2, requirements §5.1).
//! Ranged towers launch in `tower_fire` (Simulate); the rounds fly and
//! resolve right after, and the wave-end clear runs in Cleanup.

use bevy::prelude::*;

use crate::sets::GameSet;
use crate::states::GameState;
use crate::systems;

pub struct ProjectilesPlugin;

impl Plugin for ProjectilesPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<systems::projectiles::ProjectileImpact>()
            .add_systems(
                Update,
                systems::projectiles::fly_projectiles
                    .in_set(GameSet::Simulate)
                    .after(systems::tower::tower_fire)
                    .run_if(in_state(GameState::Playing)),
            )
            .add_systems(
                Update,
                systems::projectiles::clear_projectiles_on_wave_end
                    .in_set(GameSet::Cleanup)
                    .run_if(in_state(GameState::Playing)),
            );
    }
}
