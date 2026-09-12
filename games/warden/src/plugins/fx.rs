//! FxPlugin: combat presentation (FX1–FX3, requirements batch 2). Pure
//! visuals — rule logic, gold and despawn timing stay in their own domains.

use bevy::prelude::*;

use crate::sets::GameSet;
use crate::systems;

pub struct FxPlugin;

impl Plugin for FxPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
                Update,
                (
                    systems::fx::refresh_health_bars,
                    systems::fx::tick_hit_flash,
                    systems::fx::spawn_blast_rings,
                    systems::fx::tick_blast_rings,
                    systems::fx::tick_death_shells,
                    systems::fx::tint_slowed,
                    systems::fx::attach_shield_badges,
                )
                    .in_set(GameSet::Observe),
            );
    }
}
