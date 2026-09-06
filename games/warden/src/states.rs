//! Game state machine: a switchboard for systems.
//! (tag = who to process, state = whether to run — two orthogonal layers)
//!
//! `Playing` is subdivided by the dual-speed phase resource (`WavePhase`: build
//! intermission vs combat) so the slow/fast rhythm (requirements §3) can gate
//! systems cleanly without another state layer here.

use bevy::prelude::*;

#[derive(States, Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum GameState {
    #[default]
    Playing,
    Paused,
    GameOver,
    Win,
}
