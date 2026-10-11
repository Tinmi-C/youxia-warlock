//! WavesPlugin (WX6-6d): the BASE's wave director runs the show — the wave
//! table is the base `WaveTable` (code bridge from the warden schedule
//! until warden.ron lands), `run_director` spawns/settles/rewards, and
//! warden brackets it with the enemy presentation layer (models/bars on
//! director-spawned units), the WaveState mirror (HUD/gating), and the
//! game hooks on WaveCleared (三选一 / Win). `apply_econ` lands the
//! director's reward grants in the shared Resources.
//! Capability cards: WA1/WA2 (director), WA3 (check_lose until WX6-6e),
//! AC2 (shop refresh rides the mirror's change detection); WX6-6e: the
//! base's MatchRules/MatchFlow/rules_tick own the outcomes.

use bevy::prelude::*;

use crate::resources::{WaveChoice, WaveSchedule, WaveState};
use crate::sets::GameSet;
use crate::states::GameState;
use crate::systems;

pub struct WavesPlugin;

impl Plugin for WavesPlugin {
    fn build(&self, app: &mut App) {
        let schedule = WaveSchedule::schedule();
        let wave_count = schedule.waves.len() as u32;
        let defs = crate::resources::EnemyDefs::palette();
        app.insert_resource(systems::wave::base_wave_table(&schedule, &defs))
            .insert_resource(schedule)
            .init_resource::<WaveState>()
            .init_resource::<WaveChoice>()
            // WX6-6e: the base's rules domain runs the outcomes — 10 lives
            // (requirements §4), intermission-only building mirrors the
            // dual-speed rhythm, surviving the schedule wins, the TO7
            // refund ratio. LivesLeft initializes lazily in rules_tick.
            .insert_resource(warbase::prelude::MatchRules {
                leak: warbase::prelude::LeakPolicy::Lives { lives: 10 },
                build_window: warbase::prelude::BuildWindow::PrepOnly,
                lap_buff: None,
                win: warbase::prelude::WinCondition::SurviveWaves { waves: wave_count },
                sell_refund_pct: crate::resources::SELL_REFUND_RATIO,
            })
            // Eager so Input-set readers (debug panel) see it on frame 1;
            // rules_tick's lazy insert becomes a no-op.
            .insert_resource(warbase::prelude::LivesLeft(10))
            .init_resource::<warbase::prelude::MatchFlow>()
            .init_resource::<warbase::prelude::StockCount>()
            .add_message::<warbase::prelude::MatchEnded>()
            .init_resource::<warbase::prelude::WaveDirector>()
            .init_resource::<warbase::prelude::EconRules>()
            .init_resource::<warbase::prelude::Population>()
            .init_resource::<warbase::prelude::PopPurchases>()
            .add_message::<warbase::prelude::StartWave>()
            .add_message::<warbase::prelude::WaveStarted>()
            .add_message::<warbase::prelude::WaveCleared>()
            .add_message::<warbase::prelude::BossTimedOut>()
            .add_message::<warbase::prelude::WavePreview>()
            .add_message::<warbase::prelude::EconCommand>()
            .add_message::<warbase::prelude::EconChanged>()
            .add_message::<warbase::prelude::EconRejected>()
            .add_systems(
                Update,
                (
                    warbase::prelude::run_director,
                    // Warden layer on director-spawned units (same frame:
                    // the chain puts the spawn commands on-board first).
                    systems::enemy::attach_enemy_layer,
                    // Reward grants from the director land in Resources.
                    warbase::prelude::apply_econ,
                    systems::wave::on_wave_cleared,
                    // Mirror LAST: HUD/gating read the settled state.
                    systems::wave::sync_wave_state,
                )
                    .chain()
                    .in_set(GameSet::Spawn)
                    .run_if(in_state(GameState::Playing)),
            )
            .add_systems(
                Update,
                (
                    // WX6-6e: the base's rule engine (lives out / survived
                    // all waves) and the warden state-machine mapping.
                    warbase::prelude::rules_tick,
                    systems::wave::apply_match_ended,
                )
                    .chain()
                    .in_set(GameSet::Cleanup)
                    .run_if(in_state(GameState::Playing)),
            );
    }
}
