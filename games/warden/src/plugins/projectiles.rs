//! ProjectilesPlugin: rule-layer homing rounds (PR1/PR2, requirements §5.1)
//! + the unified damage resolution (SK7, ADR-0011). Ranged towers launch in
//! `tower_fire` and melee hits queue there too (Simulate); the rounds fly
//! and queue their impacts right after; `resolve_damage` — the only writer
//! of Enemy.hp — drains everything at the end of that chain. The wave-end
//! clear runs in Cleanup.

use bevy::prelude::*;

use crate::sets::GameSet;
use crate::states::GameState;
use crate::systems;

pub struct ProjectilesPlugin;

impl Plugin for ProjectilesPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<systems::projectiles::ProjectileImpact>()
            .add_message::<warbase::prelude::DamageRequest>()
            .add_message::<warbase::prelude::DamageDealt>()
            .add_message::<warbase::prelude::UnitDied>()
            .add_systems(
                Update,
                (
                    systems::projectiles::fly_projectiles,
                    // SK7 (WX6-4b): every writer (melee in tower_fire,
                    // impacts here) is upstream in the same frame; the
                    // BASE's resolver drains last, then warden's flash
                    // reaction rides its DamageDealt.
                    warbase::prelude::resolve_damage,
                    systems::damage::flash_hits,
                )
                    .chain()
                    .in_set(GameSet::Simulate)
                    .after(systems::tower::tower_fire)
                    .run_if(in_state(GameState::Playing)),
            )
            .add_systems(
                Update,
                systems::projectiles::clear_projectiles_on_wave_end
                    .in_set(GameSet::Cleanup)
                    .run_if(in_state(GameState::Playing)),
            );
    }
}
