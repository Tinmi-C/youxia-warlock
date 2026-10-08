//! Rules (BX8): pluggable pressure & outcome. Leak policy (Lives for
//! classic/maze, Stockpile for loop-TD, PassToNeighbor reserved for
//! multiplayer), build window (Always vs PrepOnly — warden's dual-speed),
//! lap buffs as the anti-stall soft cap, boss-timeout loss, and win
//! conditions. Everything here is parameters on the map bundle.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::path::{Arrived, LapCompleted, MoveSpeed, PathFollower};
use crate::unit::{Allegiance, Hp, MaxHp};
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

#[derive(Resource, Debug, Clone, Copy)]
pub struct LivesLeft(pub u32);

#[derive(Resource, Debug, Clone, Copy, Default)]
pub struct StockCount(pub u32);

/// Cleanup-set rule engine. Reads last frame's events (messages), counts
/// live attackers per frame, and transitions MatchFlow exactly once.
pub fn rules_tick(
    mut commands: Commands,
    rules: Res<MatchRules>,
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
            let leaks = arrived.read().count();
            if leaks > 0 {
                if let Some(l) = lives.as_deref_mut() {
                    l.0 = l.0.saturating_sub(leaks as u32);
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

    // Keep resource defaults initialized lazily for UI consumers.
    let init_lives = match &rules.leak {
        LeakPolicy::Lives { lives } => Some(*lives),
        _ => None,
    };
    commands.queue(move |world: &mut World| {
        if let Some(lives) = init_lives {
            world.get_resource_or_insert_with(|| LivesLeft(lives));
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
