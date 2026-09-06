//! TowersPlugin: tower archetypes + fusion recipes + auto-fire/targeting + slow
//! ticking. Placement and the keyboard fuse action live in systems/input.rs so
//! they can gate on the build window.
//! Capability cards: TO1 (defs), TO3 (fire), TO4 (targeting), TO5 (fusion),
//! TO6 (slow).

use bevy::prelude::*;

use crate::resources::{Boosts, FusionDefs, TowerDefs};
use crate::sets::GameSet;
use crate::states::GameState;
use crate::systems;

pub struct TowersPlugin;

impl Plugin for TowersPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(TowerDefs::palette())
            .insert_resource(FusionDefs::palette())
            .init_resource::<Boosts>()
            .add_systems(
                Update,
                (
                    systems::tower::tower_fire,
                    systems::tower::tick_slow,
                )
                    .chain()
                    .in_set(GameSet::Simulate)
                    .run_if(in_state(GameState::Playing)),
            )
            .add_systems(
                Update,
                systems::input::fuse_towers
                    .in_set(GameSet::Placement)
                    .run_if(in_state(GameState::Playing)),
            );
    }
}
