//! Wave director (BX6): table-driven spawning with Auto (loop-TD: intermission
//! countdown, overlaps are fine — stockpile pressure comes from leaks, not
//! wave gating) and Manual (warden: wait for StartWave between waves)
//! advance policies, previews for prep-phase decisions, early-call bonus,
//! and boss timers.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::econ::EconCommand;
use crate::movement::{AgentOrderState, MoveAgent, Order};
use crate::path::{PathDef, PathFollower};
use crate::unit::{
    spawn_unit_deferred, Allegiance, MatchDiplomacy, Owner, PlayerId, UnitId, UnitRegistry,
};

fn default_interval() -> f32 {
    1.0
}

fn default_false() -> bool {
    false
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
    /// Gold granted when this wave is fully cleared (numbers ship per map).
    #[serde(default)]
    pub reward_gold: u32,
    /// Optional-challenge wave (SEAM3b): the director tags this wave's
    /// spawns `WaveTag::Challenge` instead of `Normal` — the wave-kind
    /// vocabulary games (loop-td's challenge offers) used to fake with
    /// game-side marker components. Data-only flag; the base never sets it.
    #[serde(default = "default_false")]
    pub challenge: bool,
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
///
/// SEAM3b variant typing: `Normal(w)` for plain table waves,
/// `Challenge(w)` for optional-challenge waves (the `WaveEntry::challenge`
/// flag arms it at spawn). Custom serde keeps the wire format
/// backward-compatible: a plain number deserializes as `Normal` (old RON
/// data / hand-written fixtures), and `Normal(w)` serializes BACK as that
/// plain number, so old consumers of the numeric form see no drift;
/// `Challenge(w)` round-trips in the explicit variant form.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WaveTag {
    Normal(usize),
    Challenge(usize),
}

impl WaveTag {
    /// The wave index regardless of kind (cleared detection's join key).
    pub fn index(self) -> usize {
        match self {
            WaveTag::Normal(w) | WaveTag::Challenge(w) => w,
        }
    }

    pub fn is_challenge(self) -> bool {
        matches!(self, WaveTag::Challenge(_))
    }
}

impl serde::Serialize for WaveTag {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            WaveTag::Normal(w) => serializer.serialize_u64(*w as u64),
            WaveTag::Challenge(w) => {
                use serde::ser::SerializeTupleVariant;
                let mut tv = serializer.serialize_tuple_variant("WaveTag", 1, "Challenge", 1)?;
                tv.serialize_field(&(*w as u64))?;
                tv.end()
            }
        }
    }
}

impl<'de> serde::Deserialize<'de> for WaveTag {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        // Wire forms: a bare number (legacy) OR an explicit variant. The
        // untagged wire enum keeps both readable without hand-writing a
        // visitor; order matters (number first, so `5` never tries the
        // variant path).
        #[derive(serde::Deserialize)]
        #[serde(untagged)]
        enum Wire {
            Num(u64),
            Named(Named),
        }
        #[derive(serde::Deserialize)]
        enum Named {
            Normal(u64),
            Challenge(u64),
        }
        match Wire::deserialize(deserializer)? {
            Wire::Num(w) => Ok(WaveTag::Normal(w as usize)),
            Wire::Named(Named::Normal(w)) => Ok(WaveTag::Normal(w as usize)),
            Wire::Named(Named::Challenge(w)) => Ok(WaveTag::Challenge(w as usize)),
        }
    }
}

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

/// Endless cycling: each full pass over the table multiplies hp by this.
pub const ENDLESS_GROWTH: f32 = 1.15;

#[derive(Resource, Debug)]
pub struct WaveDirector {
    pub next_wave: usize,
    pub running: Option<usize>,
    pub phase: DirectorPhase,
    pub phase_timer: f32,
    boss_timer: Option<f32>,
    /// Completed passes over the wave table (endless mode; 0 on linear maps).
    pub cycles: u32,
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
            cycles: 0,
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

// ---------------------------------------------------------------------------
// L8 (round-4 LDEBT): owned-unit injection seam
// ---------------------------------------------------------------------------

/// How an injected unit marches once spawned. `run_director`'s spawn path
/// is the TD model (unowned Attacker + `PathFollower`); legion-style games
/// need OWNED units that fight through contested ground instead.
#[derive(Debug, Clone)]
pub enum MarchMode {
    /// TD creep track: attach a `PathFollower` on `Paths[path_index]`.
    FollowPath { path: usize },
    /// RTS attack-move: attach a `MoveAgent` with the order ARMED at
    /// spawn (waypoint seeded straight-line, MOV2 best-effort). Arming at
    /// spawn is the fix for the same-frame-order pitfall (kpi L6/L8): an
    /// observer-attached agent eats a same-frame order written before the
    /// observer ran.
    AttackMove { to: Vec3 },
}

/// One owned-unit injection: spawn def `def` at `pos` for `owner`
/// (Allegiance derived via `MatchDiplomacy`), marching per `march`.
/// Games needing game-side markers on the entity use the function form
/// [`spawn_owned_deferred`] (it returns the entity); this message form is
/// the declarative seam (consumed by [`process_spawn_requests`]).
#[derive(Message, Debug)]
pub struct SpawnRequest {
    pub def: UnitId,
    pub owner: PlayerId,
    pub pos: Vec3,
    pub hp_scale: f32,
    pub march: MarchMode,
}

/// Spawn an owned, marching unit (L8 core; both the `SpawnRequest`
/// consumer and direct game callers funnel here). Returns the entity for
/// game-side marker attachment, or None when the def / path index is
/// unknown (nothing spawned).
pub fn spawn_owned_deferred(
    commands: &mut Commands,
    registry: &UnitRegistry,
    diplomacy: &MatchDiplomacy,
    paths: &Paths,
    request: &SpawnRequest,
) -> Option<Entity> {
    let def = registry.get(&request.def)?;
    let path_entity = match request.march {
        MarchMode::FollowPath { path } => Some(*paths.0.get(path)?),
        MarchMode::AttackMove { .. } => None,
    };
    let side = diplomacy.allegiance_of(request.owner);
    let entity = spawn_unit_deferred(
        commands,
        registry,
        &request.def,
        request.pos,
        side,
        request.hp_scale,
    )?;
    let mut ec = commands.entity(entity);
    ec.insert(Owner(request.owner));
    match (&request.march, path_entity) {
        (MarchMode::FollowPath { .. }, Some(path)) => {
            ec.insert(PathFollower { path, progress: 0.0, laps: 0, finished: false });
        }
        (MarchMode::AttackMove { to }, _) => {
            ec.insert((
                MoveAgent {
                    speed: def.attrs.move_speed,
                    arrival_radius: 0.1,
                    waypoints: vec![*to],
                    velocity: Vec3::ZERO,
                    body_radius: def.attrs.body_radius,
                    order: Some(Order::AttackMove { to: *to }),
                },
                AgentOrderState::Moving,
            ));
        }
        // Unreachable: FollowPath resolved its entity above or returned.
        (MarchMode::FollowPath { .. }, None) => {}
    }
    Some(entity)
}

/// Declarative consumer of [`SpawnRequest`] (Director set, right after
/// `run_director`). Marker-free injections (reinforcements, summons)
/// use this directly; marker-carrying games prefer the function form.
pub fn process_spawn_requests(
    mut requests: MessageReader<SpawnRequest>,
    registry: Res<UnitRegistry>,
    diplomacy: Res<MatchDiplomacy>,
    paths: Res<Paths>,
    mut commands: Commands,
) {
    for request in requests.read() {
        let _ = spawn_owned_deferred(&mut commands, &registry, &diplomacy, &paths, request);
    }
}

impl WaveDirector {
    fn enter_intermission(&mut self, intermission: f32) {
        self.phase = DirectorPhase::Intermission;
        self.phase_timer = intermission;
    }

    /// Debug/cheat seam: begin wave `w` with an EMPTY spawn ledger — the
    /// wave exists for tagging and cleared detection, but the table spawns
    /// nothing itself (summoned units are the wave's content).
    pub fn force_begin(&mut self, w: usize) {
        self.running = Some(w);
        self.phase = DirectorPhase::Spawning;
        self.boss_timer = None;
        self.spawned_clean_frames = 0;
        self.ledger = Vec::new();
    }

    /// Debug/cheat seam: pretend the running wave fully spawned and its
    /// units are gone — the next run_director tick takes the NORMAL cleared
    /// path (reward grant, WaveCleared, advance), no duplicated logic.
    pub fn force_settle(&mut self) {
        self.ledger.clear();
        self.spawned_clean_frames = 2;
    }

    /// Units the current wave still owes (sum of ledger remainders) — the
    /// HUD-facing "wave progress" read.
    pub fn pending_spawns(&self) -> u32 {
        self.ledger.iter().map(|t| t.remaining).sum()
    }
}

/// Director-set state machine. 13 params is deliberate (under the 16 tuple
/// wall); splitting the phase machine across systems would re-conflict on
/// `WaveDirector` state.
#[allow(clippy::type_complexity)]
pub fn run_director(
    time: Res<Time>,
    rules: Res<crate::rule::MatchRules>,
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

    // A wave that begins this frame runs its spawn tick in the SAME frame
    // (warden's start button promises the first enemy on the starting
    // frame), so the Spawning work sits below the match instead of inside
    // a Spawning-only arm.
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
        DirectorPhase::Spawning | DirectorPhase::Done => {}
    }

    if director.phase == DirectorPhase::Spawning {
        // Boss timer runs from wave start, spawning or not.
        if let Some(t) = director.boss_timer.as_mut() {
            *t -= dt;
            if *t <= 0.0 {
                let w = director.running.unwrap_or(director.next_wave.saturating_sub(1));
                boss_out.write(BossTimedOut(w));
                director.boss_timer = None; // fire once
            }
        }
        let (running, cycle_growth) = {
            let r = director.running.unwrap_or(0);
            (r, ENDLESS_GROWTH.powi(director.cycles as i32))
        };
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
                        spec.hp_scale * cycle_growth,
                    ) {
                        commands.entity(u)
                            .insert(PathFollower { path: path_e, progress: 0.0, laps: 0, finished: false })
                            // SEAM3b: entry-level challenge flag decides the
                            // tag kind (game-side marker components retired).
                            .insert(if table.waves[running].challenge {
                                WaveTag::Challenge(running)
                            } else {
                                WaveTag::Normal(running)
                            });
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
                if !tagged.iter().any(|t| t.index() == w) {
                    cleared.write(WaveCleared(w));
                    let reward = table.waves[w].reward_gold;
                    if reward > 0 {
                        econ.write(EconCommand::Grant { gold: reward, wood: 0 });
                    }
                    director.running = None;
                    director.next_wave = w + 1;
                    if director.next_wave >= table.waves.len() {
                        if matches!(
                            rules.win,
                            crate::rule::WinCondition::Endless
                        ) && !table.waves.is_empty()
                        {
                            // Endless: wrap around, one growth pass tougher.
                            director.cycles += 1;
                            director.next_wave = 0;
                            if let Some(t) = intermission {
                                director.enter_intermission(t);
                                preview.write(WavePreview(0));
                            } else {
                                director.phase = DirectorPhase::Idle;
                            }
                        } else {
                            director.phase = DirectorPhase::Done;
                        }
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
