//! LT2-3 challenge waves (v2 proposal §2 #3): after every
//! `CHALLENGE_MILESTONE` waves cleared, the intermission offers ONE optional
//! challenge (key C on the build command card): the NEXT wave's spawn
//! hp_scale is multiplied by `CHALLENGE_HP_MULT` and its `reward_gold` by
//! `CHALLENGE_REWARD_MULT` (the base director spawns and grants through its
//! normal path — the game only mutates the wave-table entry and snapshots
//! the original). Clearing the challenge wave restores the entry and drops
//! exactly one guaranteed equipment `GroundItem` at the ring start (seeded
//! pick through `MatchRng`).
//!
//! SEAM3b: the challenge distinction rides the base vocabulary — arming sets
//! `WaveEntry::challenge`, the director tags spawns `WaveTag::Challenge`,
//! and the retired game-side `ChallengeSpawn` marker is gone.

use bevy::prelude::*;
use warbase::prelude::*;

/// One challenge offer per this many cleared waves.
pub const CHALLENGE_MILESTONE: usize = 10;
/// hp_scale multiplier while the challenge is armed.
pub const CHALLENGE_HP_MULT: f32 = 2.0;
/// reward_gold multiplier while the challenge is armed.
pub const CHALLENGE_REWARD_MULT: u32 = 3;

#[derive(Resource, Debug, Default)]
pub struct ChallengeState {
    /// Wave index currently armed (table mutated, wave not yet started).
    pub armed: Option<usize>,
    /// Snapshot of the pristine entry for restore-on-clear.
    pub original: Option<WaveEntry>,
    /// Milestones (next_wave / CHALLENGE_MILESTONE) already spent.
    pub offered: Vec<usize>,
}

#[derive(Resource, Default)]
struct ClearCursor(bevy::ecs::message::MessageCursor<WaveCleared>);

/// Arm the next wave as a challenge. Valid only while Running, in an
/// intermission, exactly on a 10-wave milestone, and once per milestone.
pub fn arm_challenge(world: &mut World) -> Result<usize, String> {
    if *world.resource::<MatchFlow>() != MatchFlow::Running {
        return Err("match over".into());
    }
    let (intermission, w) = {
        let d = world.resource::<WaveDirector>();
        (matches!(d.phase, DirectorPhase::Intermission), d.next_wave)
    };
    if !intermission {
        return Err("challenge only during the intermission".into());
    }
    if w == 0 || w % CHALLENGE_MILESTONE != 0 {
        return Err(format!("no challenge offer at wave {w}"));
    }
    let milestone = w / CHALLENGE_MILESTONE;
    world.get_resource_or_insert_with(ChallengeState::default);
    let already = {
        let state = world.resource::<ChallengeState>();
        state.offered.contains(&milestone) || state.armed.is_some()
    };
    if already {
        return Err(format!("challenge for milestone {milestone} already spent"));
    }

    let original = {
        let mut table = world.resource_mut::<WaveTable>();
        let entry = table
            .waves
            .get_mut(w)
            .ok_or_else(|| format!("wave {w} out of table"))?;
        let snapshot = entry.clone();
        for spec in &mut entry.spawns {
            spec.hp_scale *= CHALLENGE_HP_MULT;
        }
        entry.reward_gold *= CHALLENGE_REWARD_MULT;
        entry.preview = format!("挑战! {}", entry.preview);
        // SEAM3b: the base director tags this wave's spawns
        // WaveTag::Challenge while the flag is set (restored on clear).
        entry.challenge = true;
        snapshot
    };
    let mut state = world.resource_mut::<ChallengeState>();
    state.armed = Some(w);
    state.original = Some(original);
    state.offered.push(milestone);
    info!("[loop-td] challenge armed for wave {} (hp x{CHALLENGE_HP_MULT}, reward x{CHALLENGE_REWARD_MULT})", w + 1);
    Ok(w)
}

/// Update: on the armed wave's clear, restore the table entry and drop one
/// guaranteed equipment GroundItem. (Spawn tagging is the base director's
/// job now — `WaveTag::Challenge` via `WaveEntry::challenge`.)
pub fn challenge_progress(world: &mut World) {
    world.get_resource_or_insert_with(ChallengeState::default);
    world.get_resource_or_insert_with(ClearCursor::default);

    // Restore + guaranteed drop on clear.
    let cleared_here: Vec<usize> = world.resource_scope(|w, mut c: Mut<ClearCursor>| {
        let msgs = w.resource::<bevy::ecs::message::Messages<WaveCleared>>();
        c.0.read(msgs).map(|m| m.0).collect()
    });
    for w in cleared_here {
        let matches_armed = world.resource::<ChallengeState>().armed == Some(w);
        if !matches_armed {
            continue;
        }
        let original = world.resource::<ChallengeState>().original.clone();
        if let Some(original) = original {
            let mut table = world.resource_mut::<WaveTable>();
            if let Some(entry) = table.waves.get_mut(w) {
                *entry = original;
            }
        }
        {
            let mut state = world.resource_mut::<ChallengeState>();
            state.armed = None;
            state.original = None;
        }
        drop_guaranteed_equipment(world);
        info!("[loop-td] challenge wave {} cleared — table restored, equipment dropped", w + 1);
    }
}

/// One guaranteed equipment drop at the first path's start point (seeded
/// pick through `MatchRng` from the sorted equipment list).
fn drop_guaranteed_equipment(world: &mut World) {
    let items = crate::systems::equip::equipment_ids(world);
    let Some(item) = ({ &mut *world.resource_mut::<MatchRng>() }.pick(&items).cloned()) else {
        info!("[loop-td] challenge drop skipped: no equipment in the item table");
        return;
    };
    let pos = world
        .resource::<Paths>()
        .0
        .first()
        .and_then(|&p| world.get::<PathDef>(p).map(|def| def.sample(0.0)))
        .unwrap_or_default();
    world.spawn((
        GroundItem { def: item, grace: 0.0 },
        Transform::from_translation(Vec3::new(pos.x, 0.0, pos.y)),
    ));
}

/// Run-restart hook: forget the arm/offer bookkeeping (re-assembly restores
/// the pristine table anyway; an armed-but-unstarted challenge must not
/// survive into a fresh match).
pub fn reset(world: &mut World) {
    world.remove_resource::<ChallengeState>();
    world.remove_resource::<ClearCursor>();
}
