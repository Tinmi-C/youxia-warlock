//! GamePlugin: headless-safe core — map load + starting layout, RTS kit
//! attachment, train input, harvest loop, and the game-side pointer bridge
//! (channel-C patch for the base's pick-plane seam; see input_bridge.rs).
//! Rendering/UI asset deps stay in VisualPlugin.

use bevy::prelude::*;
use warbase::WarbaseSet;

use crate::systems::{gameplay, input_bridge};

/// Headless-safe core (the pointer bridge no-ops without a window/input).
pub struct GamePlugin;

impl Plugin for GamePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Startup,
            // Layout spawns AFTER the map assembles (registry/resources ready);
            // the base pointer layer is silenced via its own swap seam.
            (gameplay::load_map, gameplay::spawn_starting_units, input_bridge::silence_base_picker)
                .chain(),
        )
        .add_systems(
            Update,
            (
                gameplay::attach_rts_kit,
                gameplay::train_input,
                gameplay::harvest_tick,
                // D2-style placement: raw input translation at the very front.
                input_bridge::rts_pointer_bridge.before(WarbaseSet::Tick),
            ),
        );
    }
}
