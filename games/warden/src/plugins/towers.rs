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
            .add_message::<systems::tower::TowerFired>()
            .add_systems(
                Update,
                (
                    // ART7: mirror the rule target into the mount, THEN turn
                    // the mount, THEN fire (the muzzle reads this frame's
                    // facing). Presentation leads, rhythm stays with fire.
                    // ART8: recoil reacts to the fire message right after it,
                    // so the kick lands on the fire frame.
                    systems::tower::tower_aim_bridge,
                    systems::tower::tick_aim_trackers,
                    systems::tower::tower_fire,
                    systems::tower::tick_fire_recoil,
                    systems::tower::tick_slow,
                )
                    .chain()
                    .in_set(GameSet::Simulate)
                    .run_if(in_state(GameState::Playing)),
            )
            // ART12: the ranger defender — rig wiring is one-shot (marker
            // consumed), attack reacts to the fire message, recovery counts
            // back to idle, the body faces the current target.
            .add_systems(
                Update,
                systems::tower::wire_ranger.run_if(in_state(GameState::Playing)),
            )
            .add_systems(
                Update,
                (
                    systems::tower::ranger_play_attack,
                    systems::tower::ranger_tick_recover,
                    systems::tower::ranger_face_targets,
                )
                    .chain()
                    .in_set(GameSet::Simulate)
                    .run_if(in_state(GameState::Playing)),
            );
    }
}
