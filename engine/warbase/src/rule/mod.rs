//! Rules (BX8): pluggable pressure & outcome. Leak policy (Lives for
//! classic/maze, Stockpile for loop-TD, PassToNeighbor reserved for
//! multiplayer), build window (Always vs PrepOnly — warden's dual-speed),
//! lap buffs as the anti-stall soft cap, boss-timeout loss, and win
//! conditions. Everything here is parameters on the map bundle.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

use crate::combat::AttackKit;
use crate::path::{Arrived, LapCompleted, MoveSpeed, PathFollower};
use crate::unit::{Allegiance, Hp, MatchDiplomacy, MaxHp, Owner, TeamId, UnitInstance, UnitRegistry};
use crate::wave::{BossTimedOut, WaveCleared};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LeakPolicy {
    /// Classic/maze: a unit that finishes the path costs one life.
    Lives { lives: u32 },
    /// Loop-TD: leaked units stay on the loop; when the number of live
    /// attackers reaches `cap`, the match is lost.
    Stockpile { cap: u32 },
    /// Water-TD style: leaks pass to the next player (multiplayer, v2 —
    /// reserved schema slot, not implemented).
    PassToNeighbor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BuildWindow {
    Always,
    /// warden's dual-speed rhythm: building only between waves.
    PrepOnly,
}

/// Permanent per-lap gain for leaked units — the anti-stall soft cap. Too
/// weak and players infinitely stall the boss; too strong and a leak spiral
/// becomes unrecoverable (BAL anchor A2 watches exactly this).
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct LapBuffPolicy {
    pub speed_pct: f32,
    pub hp_pct: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WinCondition {
    SurviveWaves { waves: u32 },
    Endless,
    /// MP2 melee semantics (ADR-0017): the match runs while two or more
    /// MUTUALLY HOSTILE teams still have live combat units (an `AttackKit`
    /// or `PathFollower` carrier); once no hostile pair remains, the
    /// surviving team (smallest `TeamId` when several non-hostile teams
    /// stand — deterministic) wins. Units map to teams via `Owner` +
    /// `MatchDiplomacy`; unowned units fold through the MP1 rule
    /// (Defender -> team 0, Attacker -> environment team). A new enum
    /// variant is backward compatible: old RON bundles (SurviveWaves /
    /// Endless) deserialize unchanged and never take this branch.
    LastTeamStanding,
}

#[derive(Debug, Clone, Resource, Serialize, Deserialize)]
pub struct MatchRules {
    pub leak: LeakPolicy,
    pub build_window: BuildWindow,
    #[serde(default)]
    pub lap_buff: Option<LapBuffPolicy>,
    pub win: WinCondition,
    /// Refund fraction when selling (0.0..=1.0), from the bundle.
    pub sell_refund_pct: f32,
}

impl Default for MatchRules {
    fn default() -> Self {
        Self {
            leak: LeakPolicy::Lives { lives: 10 },
            build_window: BuildWindow::Always,
            lap_buff: None,
            win: WinCondition::Endless,
            sell_refund_pct: 0.7,
        }
    }
}

#[derive(Resource, Debug, PartialEq, Eq, Clone)]
pub enum MatchFlow {
    Running,
    Won(String),
    Lost(String),
}

impl Default for MatchFlow {
    fn default() -> Self {
        Self::Running
    }
}

#[derive(Message, Debug, Clone, PartialEq, Eq)]
pub struct MatchEnded {
    /// Ok(reason) = won, Err(reason) = lost.
    pub outcome: Result<String, String>,
}

/// Structured winner of a `LastTeamStanding` match (MP2): the surviving
/// `TeamId`, lazily inserted at the win transition (same pattern as
/// `LivesLeft`). Why not a `MatchEnded` field or a `MatchWon` message:
/// both need lib.rs edits — a struct field breaks every constructor
/// (including tests in frozen files) and a new message needs an
/// `add_message` registration; lib.rs is frozen for parallel domain
/// increments (plan D3). So `MatchEnded` carries the human-readable reason
/// (`"last team standing: team N wins"`) unchanged, and this resource
/// carries the winner as data. Reachable from the crate prelude via the
/// production module's prelude re-export (the one frozen-lib escape
/// hatch); the next SEAM revision should lift it into rule's own prelude
/// line.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub struct MatchWinner(pub TeamId);

#[derive(Resource, Debug, Clone, Copy)]
pub struct LivesLeft(pub u32);

#[derive(Resource, Debug, Clone, Copy, Default)]
pub struct StockCount(pub u32);

/// Cleanup-set rule engine. Reads last frame's events (messages), counts
/// live attackers per frame, and transitions MatchFlow exactly once.
pub fn rules_tick(
    mut commands: Commands,
    rules: Res<MatchRules>,
    registry: Res<UnitRegistry>,
    diplomacy: Res<MatchDiplomacy>,
    mut flow: ResMut<MatchFlow>,
    mut ended: MessageWriter<MatchEnded>,
    mut arrived: MessageReader<Arrived>,
    mut laps: MessageReader<LapCompleted>,
    mut boss_out: MessageReader<BossTimedOut>,
    mut cleared: MessageReader<WaveCleared>,
    mut lives: Option<ResMut<LivesLeft>>,
    mut stock: ResMut<StockCount>,
    // Hp is read for stock counting and written for lap buffs — ParamSet
    // keeps the two accesses disjoint (B0001).
    mut queries: ParamSet<(
        Query<(&Hp, &Allegiance), With<PathFollower>>,
        Query<(&mut MoveSpeed, &mut Hp, &mut MaxHp)>,
    )>,
    instances: Query<&UnitInstance>,
    // MP2: live combat units (fighters) for LastTeamStanding. cleanup_dead
    // runs earlier in the same Cleanup chain, so despawned units are gone
    // by the time this query is iterated.
    fighters: Query<(Option<&Owner>, &Allegiance), Or<(With<AttackKit>, With<PathFollower>)>>,
) {
    if *flow != MatchFlow::Running {
        return;
    }

    // Stockpile counting (only meaningful under that policy; cheap anyway).
    let count = queries
        .p0()
        .iter()
        .filter(|(_, side)| **side == Allegiance::Attacker)
        .count() as u32;

    match &rules.leak {
        LeakPolicy::Lives { .. } => {
            // Per-unit leak: the Arrived unit's def decides how many lives
            // it costs (boss 5, elite 3, plain 1). Units without a resolvable
            // def (despawned/bare test entities) cost the plain 1.
            let mut leaks = 0u32;
            for a in arrived.read() {
                let per = instances
                    .get(a.unit)
                    .ok()
                    .and_then(|i| registry.get(&i.def))
                    .map(|d| d.leak)
                    .unwrap_or(1);
                leaks = leaks.saturating_add(per);
            }
            if leaks > 0 {
                if let Some(l) = lives.as_deref_mut() {
                    l.0 = l.0.saturating_sub(leaks);
                    if l.0 == 0 {
                        lose(&mut flow, &mut ended, "no lives left");
                    }
                }
            }
        }
        LeakPolicy::Stockpile { cap } => {
            stock.0 = count;
            if count >= *cap {
                lose(&mut flow, &mut ended, &format!("stockpile full ({count}/{cap})"));
            }
        }
        LeakPolicy::PassToNeighbor => {
            let leaks = arrived.read().count();
            // v2 multiplayer seam: leaks would route to the neighbor's ring.
            let _ = leaks;
        }
    }

    // Lap buffs: permanent, multiplicative (anti-stall soft cap).
    if let Some(policy) = rules.lap_buff {
        for lap in laps.read() {
            if let Ok((mut speed, mut hp, mut max)) = queries.p1().get_mut(lap.unit) {
                speed.0 *= 1.0 + policy.speed_pct;
                let gain = max.0 * policy.hp_pct;
                max.0 += gain;
                hp.0 += gain;
            }
        }
    }

    if boss_out.read().count() > 0 {
        lose(&mut flow, &mut ended, "boss timeout");
    }

    if let WinCondition::SurviveWaves { waves } = &rules.win {
        for c in cleared.read() {
            if c.0 + 1 == *waves as usize {
                *flow = MatchFlow::Won("survived all waves".into());
                ended.write(MatchEnded { outcome: Ok("survived all waves".into()) });
                break;
            }
        }
    }

    // MP2: team-elimination semantics. Runs ONLY under
    // `WinCondition::LastTeamStanding` — every existing map (Endless /
    // SurviveWaves) skips this branch entirely: zero drift. The Running
    // re-check keeps a same-frame earlier transition (leak loss, boss
    // timeout) authoritative — team victory never overwrites it.
    let mut winner: Option<TeamId> = None;
    if *flow == MatchFlow::Running && matches!(rules.win, WinCondition::LastTeamStanding) {
        // Teams with at least one live combat unit. BTreeSet: deterministic
        // iteration order AND a deterministic "smallest surviving team"
        // pick when several non-hostile teams stand (D6).
        let mut present: BTreeSet<TeamId> = BTreeSet::new();
        for (owner, side) in fighters.iter() {
            let team = match owner {
                // Owned units fight for their owner's team; unlisted owners
                // conservatively fold to the home team (mirrors
                // MatchDiplomacy::allegiance_of).
                Some(o) => diplomacy.team_of(o.0).unwrap_or(TeamId(0)),
                // Unowned units follow the MP1 side mapping: Defender ->
                // team 0, Attacker -> environment team.
                None => match side {
                    Allegiance::Defender => TeamId(0),
                    Allegiance::Attacker => diplomacy.environment_team(),
                },
            };
            present.insert(team);
        }
        let still_fighting = present
            .iter()
            .any(|a| present.iter().any(|b| a != b && diplomacy.teams_hostile(*a, *b)));
        // Transition only on the positive outcome: at least one team
        // standing and no hostile pair left among the survivors. An empty
        // field (mutual destruction) leaves the flow to the other
        // policies — no surprise transitions.
        if !present.is_empty() && !still_fighting {
            let team = *present.iter().next().expect("non-empty checked above");
            let reason = format!("last team standing: team {} wins", team.0);
            *flow = MatchFlow::Won(reason.clone());
            ended.write(MatchEnded { outcome: Ok(reason) });
            winner = Some(team);
        }
    }

    // Keep resource defaults initialized lazily for UI consumers.
    let init_lives = match &rules.leak {
        LeakPolicy::Lives { lives } => Some(*lives),
        _ => None,
    };
    commands.queue(move |world: &mut World| {
        if let Some(lives) = init_lives {
            world.get_resource_or_insert_with(|| LivesLeft(lives));
        }
        if let Some(team) = winner {
            world.insert_resource(MatchWinner(team));
        }
    });
}

fn lose(flow: &mut MatchFlow, ended: &mut MessageWriter<MatchEnded>, reason: &str) {
    *flow = MatchFlow::Lost(reason.to_string());
    ended.write(MatchEnded { outcome: Err(reason.to_string()) });
}

/// Whether building is allowed right now (build module consults this).
pub fn building_open(rules: &MatchRules, director: &crate::wave::WaveDirector) -> bool {
    match rules.build_window {
        BuildWindow::Always => true,
        // PrepOnly: open while idle/in intermission (incl. manual waiting),
        // locked while a wave is spawning.
        BuildWindow::PrepOnly => !matches!(director.phase, crate::wave::DirectorPhase::Spawning),
    }
}
