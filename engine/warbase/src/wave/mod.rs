//! Wave director (BX6): table-driven spawning with Auto (loop-TD: intermission
//! countdown, overlaps are fine — stockpile pressure comes from leaks, not
//! wave gating) and Manual (warden: wait for StartWave between waves)
//! advance policies, previews for prep-phase decisions, early-call bonus,
//! and boss timers.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::econ::EconCommand;
use crate::path::{PathDef, PathFollower};
use crate::unit::{spawn_unit_deferred, Allegiance, UnitId, UnitRegistry};

fn default_interval() -> f32 {
    1.0
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpawnSpec {
    pub unit: UnitId,
    pub count: u32,
    #[serde(default = "default_interval")]
    pub interval: f32,
    /// Index into the map bundle's path list.
    pub path: usize,
    #[serde(default)]
    pub delay: f32,
    /// Per-wave hp multiplier (wave-strength curve ships in the map file).
    #[serde(default = "one")]
    pub hp_scale: f32,
}

fn one() -> f32 {
    1.0
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WaveEntry {
    #[serde(default)]
    pub spawns: Vec<SpawnSpec>,
    #[serde(default)]
    pub boss_timeout: Option<f32>,
    /// Preview key/label for the prep-phase UI.
    #[serde(default)]
    pub preview: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Advance {
    /// Loop-TD style: next wave starts when the intermission countdown ends,
    /// regardless of whether the previous one was fully cleared.
    Auto { intermission: f32 },
    /// warden style: waits for an explicit StartWave between waves.
    Manual,
}

#[derive(Debug, Clone, Resource, Serialize, Deserialize)]
pub struct WaveTable {
    pub waves: Vec<WaveEntry>,
    pub advance: Advance,
    /// Gold granted for early-calling the next wave during an Auto
    /// intermission.
    #[serde(default)]
    pub early_call_bonus: Option<u32>,
}

impl Default for WaveTable {
    /// Empty manual table: a no-op director (test apps get this for free;
    /// real tables come from the map bundle).
    fn default() -> Self {
        Self { waves: Vec::new(), advance: Advance::Manual, early_call_bonus: None }
    }
}

/// Path entities in bundle order (the assembler records them here).
#[derive(Resource, Debug, Default)]
pub struct Paths(pub Vec<Entity>);

/// Which wave a unit belongs to (cleared detection counts these).
#[derive(Component, Debug)]
pub struct WaveTag(pub usize);

#[derive(Debug, PartialEq)]
pub enum DirectorPhase {
    /// Before the first wave (Auto begins its intermission on frame 1;
    /// Manual waits for StartWave). Also used between waves in Manual.
    Idle,
    Intermission,
    Spawning,
    Done,
}

#[derive(Debug)]
struct SpawnTrack {
    spec: usize,
    remaining: u32,
    timer: f32,
}

#[derive(Resource, Debug)]
pub struct WaveDirector {
    pub next_wave: usize,
    pub running: Option<usize>,
    pub phase: DirectorPhase,
    pub phase_timer: f32,
    boss_timer: Option<f32>,
    ledger: Vec<SpawnTrack>,
    /// Consecutive frames where every spec finished spawning. Cleared-wave
    /// detection waits >=2 so the last queued spawn (deferred Commands) is
    /// guaranteed to be on-board before we count survivors.
    spawned_clean_frames: u32,
}

impl Default for WaveDirector {
    fn default() -> Self {
        Self {
            next_wave: 0,
            running: None,
            phase: DirectorPhase::Idle,
            phase_timer: 0.0,
            boss_timer: None,
            ledger: Vec::new(),
            spawned_clean_frames: 0,
        }
    }
}

#[derive(Message, Debug)]
pub struct WavePreview(pub usize);

#[derive(Message, Debug)]
pub struct WaveStarted(pub usize);

#[derive(Message, Debug)]
pub struct WaveCleared(pub usize);

#[derive(Message, Debug)]
pub struct BossTimedOut(pub usize);

/// Manual trigger; in an Auto intermission it early-calls (plus bonus).
#[derive(Message, Debug)]
pub struct StartWave;

impl WaveDirector {
    fn enter_intermission(&mut self, intermission: f32) {
        self.phase = DirectorPhase::Intermission;
        self.phase_timer = intermission;
    }
}

/// Director-set state machine. 13 params is deliberate (under the 16 tuple
/// wall); splitting the phase machine across systems would re-conflict on
/// `WaveDirector` state.
#[allow(clippy::type_complexity)]
pub fn run_director(
    time: Res<Time>,
    mut director: ResMut<WaveDirector>,
    table: Res<WaveTable>,
    registry: Res<UnitRegistry>,
    paths: Res<Paths>,
    path_defs: Query<&PathDef>,
    mut start: MessageReader<StartWave>,
    mut preview: MessageWriter<WavePreview>,
    mut started: MessageWriter<WaveStarted>,
    mut cleared: MessageWriter<WaveCleared>,
    mut boss_out: MessageWriter<BossTimedOut>,
    mut econ: MessageWriter<EconCommand>,
    mut commands: Commands,
    tagged: Query<&WaveTag>,
) {
    let dt = time.delta_secs();
    let start_requested = start.read().count() > 0;
    let intermission = match table.advance {
        Advance::Auto { intermission } => Some(intermission),
        Advance::Manual => None,
    };

    match director.phase {
        DirectorPhase::Idle => {
            if director.next_wave >= table.waves.len() {
                director.phase = DirectorPhase::Done;
            } else if let Some(t) = intermission {
                // Auto: kick off the first intermission right away.
                director.enter_intermission(t);
                preview.write(WavePreview(director.next_wave));
            } else if start_requested {
                begin_wave(&mut director, &table, &mut started);
            }
        }
        DirectorPhase::Intermission => {
            if start_requested {
                // Early call: bonus applies only in Auto mode.
                if let (Advance::Auto { .. }, Some(bonus)) = (&table.advance, table.early_call_bonus) {
                    econ.write(EconCommand::Grant { gold: bonus, wood: 0 });
                }
                begin_wave(&mut director, &table, &mut started);
            } else {
                director.phase_timer -= dt;
                if director.phase_timer <= 0.0 {
                    begin_wave(&mut director, &table, &mut started);
                }
            }
        }
        DirectorPhase::Spawning => {
            // Boss timer runs from wave start, spawning or not.
            if let Some(t) = director.boss_timer.as_mut() {
                *t -= dt;
                if *t <= 0.0 {
                    let w = director.running.unwrap_or(director.next_wave.saturating_sub(1));
                    boss_out.write(BossTimedOut(w));
                    director.boss_timer = None; // fire once
                }
            }
            let running = director.running.unwrap_or(0);
            let mut all_spawned = true;
            for track in director.ledger.iter_mut() {
                if track.remaining == 0 {
                    continue;
                }
                track.timer -= dt;
                while track.timer <= 0.0 && track.remaining > 0 {
                    let spec = &table.waves[running].spawns[track.spec];
                    if let Some(&path_e) = paths.0.get(spec.path) {
                        let start = path_defs.get(path_e).map(|p| p.sample(0.0)).unwrap_or_default();
                        if let Some(u) = spawn_unit_deferred(
                            &mut commands,
                            &registry,
                            &spec.unit,
                            Vec3::new(start.x, 0.0, start.y),
                            Allegiance::Attacker,
                            spec.hp_scale,
                        ) {
                            commands.entity(u)
                                .insert(PathFollower { path: path_e, progress: 0.0, laps: 0, finished: false })
                                .insert(WaveTag(running));
                        }
                    }
                    track.remaining -= 1;
                    track.timer += spec.interval.max(0.0);
                }
                if track.remaining > 0 {
                    all_spawned = false;
                }
            }
            // Cleared = everything spawned (for 2+ frames, so deferred spawns
            // are on-board) and nobody alive with this tag.
            if all_spawned {
                director.spawned_clean_frames += 1;
            } else {
                director.spawned_clean_frames = 0;
            }
            if all_spawned && director.spawned_clean_frames >= 2 {
                if let Some(w) = director.running {
                    if !tagged.iter().any(|t| t.0 == w) {
                        cleared.write(WaveCleared(w));
                        director.running = None;
                        director.next_wave = w + 1;
                        if director.next_wave >= table.waves.len() {
                            director.phase = DirectorPhase::Done;
                        } else if let Some(t) = intermission {
                            director.enter_intermission(t);
                            preview.write(WavePreview(director.next_wave));
                        } else {
                            director.phase = DirectorPhase::Idle; // Manual waits again
                        }
                    }
                }
            }
        }
        DirectorPhase::Done => {}
    }
}

fn begin_wave(
    director: &mut WaveDirector,
    table: &WaveTable,
    started: &mut MessageWriter<WaveStarted>,
) {
    let w = director.next_wave;
    let entry = &table.waves[w];
    director.running = Some(w);
    director.phase = DirectorPhase::Spawning;
    director.boss_timer = entry.boss_timeout;
    director.spawned_clean_frames = 0;
    director.ledger = entry
        .spawns
        .iter()
        .enumerate()
        .map(|(i, s)| SpawnTrack { spec: i, remaining: s.count, timer: s.delay })
        .collect();
    started.write(WaveStarted(w));
}
