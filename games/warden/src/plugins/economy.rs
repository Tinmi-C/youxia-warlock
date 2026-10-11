//! EconomyPlugin: the gold pool. Seeded with 100 gold; earning (kills, wave
//! rewards) and spending (buy towers, fusion fee) live in their domain systems.
//! Capability card: EC1.

use bevy::prelude::*;

pub struct EconomyPlugin;

impl Plugin for EconomyPlugin {
    fn build(&self, app: &mut App) {
        // WX6-6f: the starting pool ships in warden.ron (econ_start);
        // assemble lands it in the shared Resources. The empty pool is
        // registered here so assemble (and pre-update readers) always find
        // it. Earning and spending live in the domain systems.
        app.init_resource::<warbase::prelude::Resources>();
    }
}
