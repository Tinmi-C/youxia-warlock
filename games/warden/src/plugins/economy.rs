//! EconomyPlugin: the gold pool. Seeded with 100 gold; earning (kills, wave
//! rewards) and spending (buy towers, fusion fee) live in their domain systems.
//! Capability card: EC1.

use bevy::prelude::*;

use crate::resources::Economy;

pub struct EconomyPlugin;

impl Plugin for EconomyPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Economy>();
    }
}
