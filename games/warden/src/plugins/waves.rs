//! WavesPlugin: wave schedule + spawn/resolve + lose/win.
//! Capability cards: WA1 (config/spawn), WA2 (reward + intermission), WA3 (lose/win).

use bevy::prelude::*;

use crate::resources::{WaveChoice, WaveSchedule, WaveState};
use crate::sets::GameSet;
use crate::states::GameState;
use crate::systems;

pub struct WavesPlugin;

impl Plugin for WavesPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(WaveSchedule::schedule())
            .init_resource::<WaveState>()
            .init_resource::<WaveChoice>()
            .add_systems(
                Update,
                (
                    systems::wave::wave_spawn.in_set(GameSet::Spawn),
                    systems::wave::wave_resolve.in_set(GameSet::Cleanup),
                    systems::wave::check_lose.in_set(GameSet::Cleanup),
                )
                    .run_if(in_state(GameState::Playing)),
            );
    }
}
