//! UiPlugin: live HUD status line + the mouse shop bar.
//! Capability cards: UI1 (shop), UI4 (HUD). The shop bar rebuilds itself from
//! `Hand`/`ShopOffers` (AC1 deal / AC2 refresh / AC3 drops feed it).

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
                Update,
                (
                    systems::hud::update_hud,
                    systems::pointer::refresh_shop_bar,
                )
                    .in_set(GameSet::Observe),
            );
    }
}
