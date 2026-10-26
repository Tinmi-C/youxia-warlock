//! VisualPlugin: greybox visuals (real game only — headless tests never add
//! this plugin). Ortho top-down camera + directional light; unit meshes,
//! selection outlines and the HUD land with RL1. Everything bevy-native,
//! zero assets (D9 discipline).

use bevy::prelude::*;

use crate::systems::camera;

pub struct VisualPlugin;

impl Plugin for VisualPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, camera::spawn_camera);
    }
}
