//! AcquisitionPlugin: how towers enter the player's pool (requirements §8).
//! Capability cards: AC1 (opening hand; AC2 shop / AC3 drops fill this in).
//! Resource ownership: `Hand` and `RunRng` live here; the UI reads them.

use bevy::prelude::*;

use crate::resources::{Hand, RunRng};
use crate::systems;

pub struct AcquisitionPlugin;

impl Plugin for AcquisitionPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<RunRng>()
            .init_resource::<Hand>()
            .add_systems(Startup, systems::acquisition::deal_opening_hand);
    }
}
