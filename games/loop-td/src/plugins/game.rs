//! GamePlugin: core (map load + input + HUD — headless-safe) and visuals
//! (real game only). One domain = one plugin.

use bevy::prelude::*;

use crate::systems::{gameplay, hud, visuals};

/// Headless-safe core: loads the map, drives build input, refreshes HUD.
pub struct GamePlugin;

impl Plugin for GamePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, gameplay::load_map)
            .add_systems(Startup, gameplay::load_meta_startup)
            .add_systems(Startup, hud::spawn_hud)
            .add_systems(Update, gameplay::build_input)
            .add_systems(Update, gameplay::command_dispatch)
            .add_systems(Update, gameplay::snipe_effect)
            .add_systems(Update, gameplay::hero_talents)
            .add_systems(Update, (gameplay::bank_run, gameplay::end_screen_input))
            .add_systems(Update, (gameplay::track_stats, gameplay::hero_acquisition, gameplay::refresh_end_screen))
            .add_systems(Update, hud::refresh_hud);
    }
}

/// Greybox visuals: camera, ground, path ring, unit meshes, cursor.
/// Never added in headless tests (needs render asset stores).
pub struct VisualPlugin;

impl Plugin for VisualPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Startup,
            (crate::systems::camera::spawn_camera, visuals::spawn_scene),
        )
        .add_systems(Update, (visuals::move_cursor_highlight, visuals::visualize_units));
    }
}
