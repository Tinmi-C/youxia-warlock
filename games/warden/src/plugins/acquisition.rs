//! AcquisitionPlugin: how towers enter the player's pool (requirements §8).
//! Capability cards: AC1 (opening hand), AC2 (shop refresh), AC3 (kill drops).
//! Resource ownership: `Hand`, `RunRng`, `ShopOffers` live here.

use bevy::prelude::*;

use crate::resources::{Hand, RunRng, ShopOffers};
use crate::sets::GameSet;
use crate::states::GameState;
use crate::systems;

pub struct AcquisitionPlugin;

impl Plugin for AcquisitionPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<RunRng>()
            .init_resource::<Hand>()
            .init_resource::<ShopOffers>()
            .add_systems(Startup, systems::acquisition::deal_opening_hand)
            .add_systems(
                Startup,
                systems::acquisition::setup_shop_offers
                    .after(systems::acquisition::deal_opening_hand),
            )
            .add_systems(
                Update,
                systems::acquisition::refresh_shop_on_intermission
                    .in_set(GameSet::Observe)
                    .run_if(in_state(GameState::Playing)),
            )
            // Drops read dead enemies before resolve_death despawns them.
            .add_systems(
                Update,
                systems::acquisition::roll_drops
                    .in_set(GameSet::Cleanup)
                    .before(systems::enemy::resolve_death)
                    .run_if(in_state(GameState::Playing)),
            );
    }
}
