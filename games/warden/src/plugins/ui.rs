//! UiPlugin: live HUD status line + the mouse shop bar + the UI2 panels
//! (start-wave button, choice cards, tower info + range ring).
//! Capability cards: UI1 (shop), UI2 (panel + Chinese copy), UI4 (HUD). The
//! shop bar rebuilds itself from `Hand`/`ShopOffers` (AC1 deal / AC2 refresh /
//! AC3 drops feed it).

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
                    systems::pointer::refresh_control_bar,
                    systems::pointer::refresh_choice_cards,
                    systems::pointer::refresh_tower_info,
                    systems::pointer::update_hover_ghost,
                )
                    .in_set(GameSet::Observe),
            );
    }
}
