//! VisualPlugin: greybox visuals (real game only — headless tests never add
//! this plugin). Ortho top-down camera + directional light, lane stripes,
//! deploy pads, unit meshes by def. HUD lives in the PanelsPlugin (LG3,
//! headless-safe); everything here needs render assets (D9 discipline).

use bevy::prelude::*;

use crate::systems::{camera, visuals};

pub struct VisualPlugin;

impl Plugin for VisualPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Startup,
            (
                camera::spawn_camera,
                visuals::spawn_scene,
            ),
        )
        .add_systems(Update, visuals::visualize_units);
    }
}
