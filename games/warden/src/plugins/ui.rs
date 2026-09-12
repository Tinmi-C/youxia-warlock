//! UiPlugin: live HUD status line + the UI2 panels (choice cards) + the UI9
//! command card (three-zone bottom bar).
//! Capability cards: UI1 (legacy shop bar, retires with UI10), UI2 (panel +
//! Chinese copy), UI4 (HUD), UI6 (bundled font), UI7 (language toggle),
//! UI8 (next-wave preview), UI9 (command card + selection state machine).

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
            .init_resource::<crate::resources::Selection>()
            .add_systems(
                Startup,
                (
                    systems::hud::setup_ui_font,
                    systems::l10n::load_ui_prefs, // before spawn_hud: copy follows the saved lang
                    systems::hud::spawn_hud,
                    systems::command::spawn_command_bar,
                )
                    .chain(),
            )
            .add_systems(
                Update,
                (
                    systems::hud::update_hud,
                    systems::pointer::refresh_choice_cards,
                    systems::pointer::update_hover_ghost,
                    systems::pointer::refresh_end_screen,
                    // Command card (UI9/UI10).
                    systems::command::refresh_control_zone,
                    systems::command::refresh_command_card
                        .after(systems::acquisition::refresh_shop_on_intermission),
                    systems::command::refresh_info_zone,
                    systems::command::refresh_selection_ring,
                    systems::command::refresh_fusion_candidates,
                    systems::command::refresh_tooltip,
                    systems::command::hover_build_preview,
                    systems::command::refresh_start_wave_button,
                    systems::command::refresh_bar_visibility,
                )
                    .in_set(GameSet::Observe),
            )
            // Cancel + lang + pause + end-screen interactions run on any state
            // (not gated to Playing like the gameplay Input systems).
            .add_systems(
                Update,
                (
                    systems::command::cancel_modes,
                    systems::l10n::toggle_lang,
                    systems::pointer::handle_lang_button,
                    systems::command::handle_pause_button,
                    systems::pointer::handle_retry_button,
                    systems::pointer::handle_meta_upgrade_buttons,
                ),
            );
    }
}
