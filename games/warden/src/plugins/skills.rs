//! SkillsPlugin: the generic per-entity skill framework (ADR-0011, SK5) +
//! the commander skills on it (SK1-SK4 behaviour, migrated by SK8). The
//! commander singleton spawns at Startup; hotkeys/buttons write CastSkill
//! in Input; effects dispatch and cooldowns tick in Simulate; buttons
//! refresh in Observe.

use bevy::prelude::*;

use crate::resources::{SkillAiming, WarFrenzy};
use crate::sets::GameSet;
use crate::states::GameState;
use crate::systems;

pub struct SkillsPlugin;

impl Plugin for SkillsPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(systems::skills::production_skill_defs())
            .insert_resource(systems::skills::production_skill_balance())
            .init_resource::<WarFrenzy>()
            .init_resource::<SkillAiming>()
            .add_message::<systems::skills::CastSkill>()
            .add_message::<systems::skills::SkillCast>()
            // SK8: the persistent commander owns the three skills.
            .add_systems(Startup, systems::skills::spawn_commander)
            .add_systems(
                Update,
                (
                    // Point aiming owns the world click while active: it
                    // runs before select_object (no re-select behind the
                    // cast) and before the cast handlers (the click that
                    // pressed a skill button cannot confirm itself). Then
                    // hotkeys and buttons queue CastSkill requests, and the
                    // validator resolves them — all in the same frame.
                    systems::skills::update_aim_picking,
                    systems::skills::skill_hotkeys,
                    systems::skills::handle_skill_buttons,
                    systems::skills::handle_cast_requests,
                )
                    .chain()
                    .in_set(GameSet::Input)
                    .before(systems::command::select_object)
                    .run_if(in_state(GameState::Playing)),
            )
            .add_systems(
                Update,
                (
                    // Confirmed casts dispatch to their effects; the meteor
                    // volley releases right after, so the first shot drops
                    // on the cast frame (phase-1 timing).
                    systems::skills::commander_skill_effects,
                    systems::skills::release_meteor_shots
                        .after(systems::skills::commander_skill_effects),
                    // SK9: the archer kit's effects + the fire-arrow
                    // autocast trigger (needs this frame's tower target).
                    systems::archer_skills::archer_skill_effects,
                    systems::archer_skills::archer_autocast_fire_arrows,
                    // SK12: the fusion signatures' effects.
                    systems::fusion_skills::fusion_skill_effects,
                    // SK4: frenzy countdown.
                    systems::skills::tick_war_frenzy,
                    // SK5: per-entity slot cooldowns, same law.
                    systems::skills::tick_caster_cooldowns,
                )
                    .in_set(GameSet::Simulate)
                    .run_if(in_state(GameState::Playing)),
            )
            // SK9: eagle-eye passive entries re-assert before the stats
            // aggregation folds them (same-frame values).
            .add_systems(
                Update,
                systems::archer_skills::sync_archer_passives
                    .in_set(GameSet::Simulate)
                    .before(systems::stats::tick_stat_mods)
                    .before(systems::stats::aggregate_stats)
                    .run_if(in_state(GameState::Playing)),
            )
            .add_systems(
                Update,
                systems::skills::refresh_skill_buttons.in_set(GameSet::Observe),
            );
    }
}
