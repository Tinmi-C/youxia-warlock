//! VisualPlugin: greybox visuals (real game only — headless tests never add
//! this plugin). Ortho top-down camera + directional light, unit meshes by
//! def, selection outlines, fog-of-war tint, the minimap panel, HUD text.
//! Everything bevy-native, zero assets (D9 discipline).

use bevy::prelude::*;

use crate::systems::{camera, hud, minimap, visuals};

pub struct VisualPlugin;

impl Plugin for VisualPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Startup,
            (
                camera::spawn_camera,
                visuals::spawn_scene,
                hud::spawn_hud,
                minimap::spawn_minimap,
            ),
        )
        .add_systems(
            Update,
            (
                visuals::visualize_units,
                visuals::visualize_ground_items,
                visuals::sync_selection_outlines,
                visuals::sync_fog_tint,
                minimap::refresh_minimap_view,
                hud::refresh_hud,
                // Click-to-camera reads raw mouse before the world-pick bridge
                // could consume the same click (the panel swallows it).
                minimap::minimap_click_jumps_camera.before(crate::systems::input_bridge::rts_pointer_bridge),
            ),
        );
    }
}
