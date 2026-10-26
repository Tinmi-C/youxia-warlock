//! VisualPlugin: greybox visuals (real game only — headless tests never add
//! this plugin). Ortho top-down camera + directional light, unit meshes by
//! def, selection outlines, HUD text. Everything bevy-native, zero assets
//! (D9 discipline).

use bevy::prelude::*;

use crate::systems::{camera, hud, visuals};

pub struct VisualPlugin;

impl Plugin for VisualPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, (camera::spawn_camera, visuals::spawn_scene, hud::spawn_hud))
            .add_systems(
                Update,
                (visuals::visualize_units, visuals::sync_selection_outlines, hud::refresh_hud),
            );
    }
}
