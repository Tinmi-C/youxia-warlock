//! GamePlugin: headless-safe core — map load + per-player wallets, unit kit
//! attachment (MoveAgent on movers). Rendering/UI deps stay in VisualPlugin.

use bevy::prelude::*;

use crate::systems::gameplay;

/// Headless-safe core (no window/input systems inside).
pub struct GamePlugin;

impl Plugin for GamePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Startup,
            // The map assembles first (starting_forces spawn inside assemble);
            // wallets init next; the kit observer runs from the first Update.
            (gameplay::load_map, gameplay::init_purses).chain(),
        )
        .add_systems(Update, gameplay::attach_legion_kit);
    }
}
