//! MetaPlugin: cross-run progression (requirements §13). Capability card ME1.
//! Startup chain: load the save -> apply run-start effects (upgrade 2 gold),
//! both strictly before the opening hand is dealt (upgrade 1 affects it).

use bevy::prelude::*;

use crate::resources::{MetaSavePath, MetaState};
use crate::states::GameState;
use crate::systems;

pub struct MetaPlugin;

impl Plugin for MetaPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MetaState>()
            .init_resource::<MetaSavePath>()
            .add_systems(
                Startup,
                (
                    systems::meta::load_meta,
                    systems::meta::apply_meta_on_run_start,
                )
                    .chain(),
            )
            .add_systems(OnEnter(GameState::GameOver), systems::meta::award_on_death)
            .add_systems(OnEnter(GameState::Win), systems::meta::award_on_win)
            .add_systems(Update, systems::meta::buy_upgrades);
    }
}
