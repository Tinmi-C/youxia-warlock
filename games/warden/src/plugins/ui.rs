//! UiPlugin: live HUD status line + the placeholder selection state.
//! Capability cards: UI1 (shop — follow-on), UI4 (HUD). The bevy_ui shop is a
//! later card; the closed loop uses keyboard input (systems/input.rs).

use bevy::prelude::*;

use crate::resources::{Hand, SelectedTower};
use crate::sets::GameSet;
use crate::systems;

pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SelectedTower>()
            .init_resource::<Hand>()
            .add_systems(Startup, (systems::hud::spawn_hud, systems::pointer::spawn_shop_bar))
            .add_systems(Update, systems::hud::update_hud.in_set(GameSet::Observe));
    }
}
