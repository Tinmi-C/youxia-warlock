//! FxPlugin: combat presentation (FX1–FX3, requirements batch 2). Pure
//! visuals — rule logic, gold and despawn timing stay in their own domains.
//! WX4: the generic primitives (flash/rings/shells) are the base's systems;
//! warden keeps the Enemy-driven bars and the status tints/badges.

use bevy::prelude::*;

use crate::sets::GameSet;
use crate::systems;

pub struct FxPlugin;

impl Plugin for FxPlugin {
    fn build(&self, app: &mut App) {
        // The base's blast-ring input message (the bridge forwards warden's
        // ProjectileImpact into it each frame).
        app.add_message::<warbase::fx::FxBlast>().add_systems(
            Update,
            (
                // FX1: the base's bars (Hp/MaxHp source; warden styles
                // elites via HealthBarStyle at spawn — WX6-2).
                warbase::fx::refresh_health_bars,
                // FX2: base primitives — flash, rings (via the warden
                // impact bridge), shells.
                warbase::fx::tick_hit_flash,
                (systems::fx::forward_blast_feedback, warbase::fx::spawn_blast_rings).chain(),
                warbase::fx::tick_blast_rings,
                warbase::fx::tick_death_shells,
                // FX3: status visuals (game identity).
                systems::fx::tint_slowed,
                systems::fx::attach_shield_badges,
            )
                .in_set(GameSet::Observe),
        );
    }
}
