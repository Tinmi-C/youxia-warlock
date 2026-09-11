//! UiPlugin: live HUD status line + the mouse shop bar + the UI2 panels
//! (start-wave button, choice cards, tower info + range ring).
//! Capability cards: UI1 (shop), UI2 (panel + Chinese copy), UI4 (HUD),
//! UI6 (bundled font), UI7 (language toggle). The shop bar rebuilds itself
//! from `Hand`/`ShopOffers` (AC1 deal / AC2 refresh / AC3 drops feed it).

use bevy::prelude::*;

use crate::resources::{Lang, SelectedTower, UiPrefsPath};
use crate::sets::GameSet;
use crate::systems;

pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SelectedTower>()
            .init_resource::<Lang>()
            .init_resource::<UiPrefsPath>()
            .add_systems(
                Startup,
                (
                    systems::hud::setup_ui_font,
                    systems::l10n::load_ui_prefs, // before spawn_hud: copy follows the saved lang
                    systems::hud::spawn_hud,
                )
                    .chain(),
            )
            .add_systems(
                Update,
                (
                    systems::hud::update_hud,
                    systems::pointer::refresh_shop_bar,
                    systems::pointer::refresh_control_bar,
                    systems::pointer::refresh_choice_cards,
                    systems::pointer::refresh_tower_info,
                    systems::pointer::update_hover_ghost,
                    systems::pointer::refresh_end_screen,
                    systems::pointer::refresh_lang_button,
                )
                    .in_set(GameSet::Observe),
            )
            // Lang toggle + end-screen interactions run on any state (not
            // gated to Playing like the gameplay Input systems).
            .add_systems(
                Update,
                (
                    systems::l10n::toggle_lang,
                    systems::pointer::handle_lang_button,
                    systems::pointer::handle_retry_button,
                    systems::pointer::handle_meta_upgrade_buttons,
                ),
            );
    }
}
