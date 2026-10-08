//! UiPlugin stub (BX0): registered by WarbasePlugin under its cargo feature.
//! Content arrives with BX4 (command card) / WX4 (generic battle FX).

use bevy::prelude::*;

pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, _app: &mut App) {}
}
