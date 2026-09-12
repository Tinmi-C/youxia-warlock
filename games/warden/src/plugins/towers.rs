//! TowersPlugin: tower archetypes + fusion recipes + auto-fire/targeting + slow
//! ticking. Fusion now runs through the command card (UI11, command.rs);
//! the F-key auto-fuse retired with it.
//! Capability cards: TO1 (defs), TO3 (fire), TO4 (targeting), TO5 (fusion,
//! trigger reworked by UI11), TO6 (slow).

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
            );
    }
}
