//! Game state machine: a switchboard for systems.
//! (tag = who to process, state = whether to run — two orthogonal layers)

use bevy::prelude::*;

#[derive(States, Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum GameState {
    #[default]
    Playing,
    Paused,
    GameOver,
}
