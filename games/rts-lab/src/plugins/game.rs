//! GamePlugin: headless-safe core — map load (RL0) + starting layout, RTS
//! kit attachment, train input, harvest loop, left-click selection seam
//! (RL1). No rendering/UI asset deps (tests add this plugin headlessly).

use bevy::prelude::*;

use crate::systems::gameplay;

/// Headless-safe core.
pub struct GamePlugin;

impl Plugin for GamePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, gameplay::load_map);
    }
}
