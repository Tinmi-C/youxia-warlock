//! UiPlugin: live HUD status line + the mouse shop bar.
//! Capability cards: UI1 (shop), UI4 (HUD). The shop bar reads `Hand`, which
//! AC1 deals at Startup — so it is built strictly after the deal.

use bevy::prelude::*;

use crate::resources::SelectedTower;
use crate::sets::GameSet;
use crate::systems;

pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SelectedTower>()
            .add_systems(Startup, systems::hud::spawn_hud)
            .add_systems(
                Startup,
                systems::pointer::spawn_shop_bar
                    .after(systems::acquisition::deal_opening_hand),
            )
            .add_systems(Update, systems::hud::update_hud.in_set(GameSet::Observe));
    }
}
