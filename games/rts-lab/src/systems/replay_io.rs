//! RPL2-style record/replay entry points for rts-lab (RL3; pattern copied
//! from loop-td's `src/replay_io.rs`, adapted to this crate's composition).
//!
//! - `--record` (main.rs): adds `ReplayIoPlugin`, which arms the base
//!   `RecordingLog` at Startup and writes `runs/<timestamp>.jsonl` when the
//!   match ends (`MatchEnded` — win or lose). RON payload, .jsonl naming —
//!   same convention as loop-td.
//! - `--replay <file>` (main.rs): headless verification — loads the log,
//!   checks the map fingerprint, re-runs through [`replay_lab`] (per-frame
//!   input re-injection + `state_hash` reconciliation against the LAB
//!   composition: test_app + GamePlugin).
//!
//! PHASE-1 BOUNDARY (same stance as loop-td, and stricter here): the base
//! `warbase::replay()` rebuilds a BASE-ONLY app, but rts-lab's match state
//! (starting layout, harvest income, fog) is produced by GAME-side systems —
//! so replaying needs the LAB composition. [`replay_lab`] mirrors
//! `warbase::replay` with GamePlugin added; logs recorded in that same
//! composition reconcile exactly (the acceptance test below proves it).
//! Logs recorded from the FULL app (DefaultPlugins + visuals) fingerprint
//! renderer/HUD entities the lab app does not spawn, so those fail at the
//! first checkpoint by design; replaying them is spectator-mode work (RPL3+).
//!
//! FRAME-0 CHECKPOINT: the recorder's first hash (`(0, h)`) snapshots the
//! post-Startup world from INSIDE update #1 (Startup and Update run in the
//! same app.update() call — verified empirically: run_schedule(Startup) is
//! NOT deduped by a later update()). An outside-the-schedule read cannot
//! observe that midpoint, so [`replay_lab`] registers a one-shot probe
//! system at the recorder's own position (`.before(WarbaseSet::Tick)`),
//! holding its state in a captured Arc (NOT a world resource — a new
//! resource type would allocate a bookkeeping entity and shift every entity
//! index, breaking the (index, hp) fingerprint pairs; see the base replay
//! module's symmetry note).
//!
//! INPUT VOCABULARY GAP (channel C, RL3 hole #10): the base recorder only
//! captures PickAttempt / StartWave / EconCommand — rts-lab's whole command
//! surface is SelectCommand / UnitCommand / TrainCommand messages, which
//! record NO envelopes. rts-lab matches therefore record as input-free
//! deterministic simulations (the harvest economy still hashes); recording
//! real player commands needs a CommandEnvelope vocabulary for domain
//! commands in the base. ECON filtering (loop-td's constraint) is kept for
//! symmetry: engine-internal econ envelopes are dropped at write time.

use bevy::prelude::*;
use std::sync::{Arc, Mutex};
use warbase::bundle::MapBundle;
use warbase::prelude::{
    MatchFlow, MatchInputLog, MatchRng, PickAttempt, RecordingLog, ECON_KIND, INPUT_LOG_VERSION,
};
use warbase::testing::test_app;
use warbase::WarbaseSet;

use crate::plugins::game::GamePlugin;

/// Where `--record` drops finished runs (game crate dir; gitignored).
pub const RUNS_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/runs");

// ---------------------------------------------------------------------------
// Pure helpers (shared by the plugin, main.rs, and the tests)
// ---------------------------------------------------------------------------

/// FNV-1a 64 over a byte slice — same fixed fingerprint family the base
/// `state_hash` uses (no per-run keys, D6 discipline).
fn fnv1a(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in bytes {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

/// Map fingerprint: FNV-1a over the map FILE bytes (stable, order-free).
pub fn map_file_hash(path: &std::path::Path) -> Result<u64, String> {
    let bytes = std::fs::read(path)
        .map_err(|e| format!("cannot read map {}: {e}", path.display()))?;
    Ok(fnv1a(&bytes))
}

/// Arm recording on a world: inserts the base `RecordingLog` carrying the
/// match seed and the map fingerprint (idempotent — an existing log wins).
/// COMPOSITION SYMMETRY (bevy 0.19 resource-entity allocation): recording
/// and replay apps must insert the same resource types in the same order —
/// see [`recording_lab`] / [`replay_lab`], the only two constructors.
pub fn arm_recording(world: &mut World, map_path: &std::path::Path) -> Result<(), String> {
    if world.get_resource::<RecordingLog>().is_some() {
        return Ok(());
    }
    let seed = world.resource::<MatchRng>().seed();
    let map_hash = map_file_hash(map_path)?;
    let mut rec = RecordingLog::default();
    rec.log.version = INPUT_LOG_VERSION;
    rec.log.seed = seed;
    rec.log.map_hash = map_hash;
    world.insert_resource(rec);
    Ok(())
}

/// The recording composition (also the replay composition — must stay
/// byte-for-byte the same construction sequence):
/// test_app -> MatchRng::seeded -> GamePlugin -> RecordingLog (LAST new
/// resource type inserted; mirror it in [`replay_lab`]).
pub fn recording_lab(seed: u64, map_path: &std::path::Path) -> Result<App, String> {
    let mut app = test_app();
    app.insert_resource(MatchRng::seeded(seed));
    app.add_plugins(GamePlugin);
    arm_recording(&mut app.world_mut(), map_path)?;
    Ok(app)
}

/// Write a finished input log to `<dir>/<UTC timestamp>.jsonl` (RON payload)
/// with engine-internal econ envelopes filtered out. Returns the path.
pub fn write_input_log(
    dir: &std::path::Path,
    log: &MatchInputLog,
) -> std::io::Result<std::path::PathBuf> {
    let mut filtered = log.clone();
    filtered.frames = log
        .frames
        .iter()
        .map(|(frame, envs)| {
            (
                *frame,
                envs.iter()
                    .filter(|e| {
                        !matches!(e, warbase::prelude::CommandEnvelope::Ext { kind, .. } if kind == ECON_KIND)
                    })
                    .cloned()
                    .collect::<Vec<_>>(),
            )
        })
        .filter(|(_, envs)| !envs.is_empty())
        .collect();
    let text = ron::to_string(&filtered).map_err(|e| std::io::Error::other(e.to_string()))?;
    std::fs::create_dir_all(dir)?;
    let path = dir.join(utc_timestamp_name());
    std::fs::write(&path, text)?;
    Ok(path)
}

/// `YYYYMMDDTHHMMSS.<nanos>.jsonl` from the wall clock (std-only civil date:
/// Howard Hinnant's days-to-YYYY-MM-DD algorithm).
fn utc_timestamp_name() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = now.as_secs();
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    let (h, m, s) = (rem / 3600, (rem % 3600) / 60, rem % 60);
    let (y, mo, d) = civil_from_days(days);
    format!("{y:04}{mo:02}{d:02}T{h:02}{m:02}{s:02}.{}.jsonl", now.subsec_nanos())
}

/// Civil (year, month, day) from days since 1970-01-01 (Hinnant).
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

// ---------------------------------------------------------------------------
// The replay driver (lab composition)
// ---------------------------------------------------------------------------

/// Shared probe state (closure-captured: a world RESOURCE would allocate a
/// bookkeeping entity and shift entity indices — breaking the fingerprint).
#[derive(Default)]
struct ProbeState {
    done: bool,
    actual: Vec<(u32, u64)>,
    error: Option<String>,
}

/// Replay a log against the LAB composition (see the module header). Frames
/// advance exactly like `warbase::replay`: envelopes re-injected between
/// updates; every `HASH_INTERVAL` frames the world fingerprint reconciles
/// against the log. The frame-0 (post-Startup) checkpoint is compared by a
/// one-shot probe system at the recorder's own schedule position.
pub fn replay_lab(bundle_unused: &MapBundle, log: &MatchInputLog) -> Result<Vec<(u32, u64)>, String> {
    let _ = bundle_unused; // the lab composition loads its own map at Startup
    if log.version != INPUT_LOG_VERSION {
        return Err(format!(
            "input log version {} does not match {} — re-record the match",
            log.version, INPUT_LOG_VERSION
        ));
    }
    // EXACT construction sequence of `recording_lab` (symmetry, see header).
    let mut app = test_app();
    app.insert_resource(MatchRng::seeded(log.seed));
    app.add_plugins(GamePlugin);
    let map_path = crate::systems::gameplay::default_map_path();
    arm_recording(&mut app.world_mut(), &map_path)
        .map_err(|e| format!("replay-side arm failed: {e}"))?;

    // Frame-0 probe: reconcile the post-Startup checkpoint from the recorder's
    // own position (before Tick of update #1), state held out of the World.
    let probe = Arc::new(Mutex::new(ProbeState::default()));
    let probe_for_system = probe.clone();
    let expect0 = log.hashes.first().filter(|(f, _)| *f == 0).map(|(_, h)| *h);
    let probe_system = move |world: &mut World| {
        let mut st = probe_for_system.lock().expect("probe lock");
        if st.done {
            return;
        }
        st.done = true;
        let hash = warbase::prelude::state_hash(world);
        st.actual.push((0, hash));
        if let Some(expect) = expect0 {
            if hash != expect {
                st.error = Some(format!(
                    "state hash mismatch at frame 0: recorded {expect:#x}, replayed {hash:#x}"
                ));
            }
        }
    };
    app.add_systems(Update, probe_system.before(WarbaseSet::Tick));

    let mut hi = usize::from(log.hashes.first().is_some_and(|(f, _)| *f == 0));
    let last = log
        .frames
        .last()
        .map(|f| f.0)
        .unwrap_or(0)
        .max(log.hashes.last().map(|f| f.0).unwrap_or(0));
    let mut next_input = 0usize;
    for frame in 1..=last {
        while next_input < log.frames.len() && log.frames[next_input].0 < frame {
            next_input += 1;
        }
        if next_input < log.frames.len() && log.frames[next_input].0 == frame {
            for env in &log.frames[next_input].1 {
                apply_envelope(&mut app.world_mut(), env)?;
            }
            next_input += 1;
        }
        app.update();
        if frame % warbase::prelude::HASH_INTERVAL == 0 {
            let hash = warbase::prelude::state_hash(app.world());
            while hi < log.hashes.len() && log.hashes[hi].0 < frame {
                hi += 1;
            }
            let mut st = probe.lock().expect("probe lock");
            st.actual.push((frame, hash));
            if let Some(&(f, expect)) = log.hashes.get(hi) {
                if f == frame && hash != expect {
                    return Err(format!(
                        "state hash mismatch at frame {frame}: recorded {expect:#x}, replayed {hash:#x}"
                    ));
                }
            }
            if let Some(e) = st.error.take() {
                return Err(e);
            }
        }
    }
    let st = probe.lock().expect("probe lock");
    if let Some(e) = st.error.clone() {
        return Err(e);
    }
    Ok(st.actual.clone())
}

/// Convert an envelope back into its message: delegates to the BASE
/// implementation (SEAM3c made `replay::apply_envelope` public — this
/// local copy would have drifted from the Key/Card vocabulary).
fn apply_envelope(world: &mut World, env: &warbase::prelude::CommandEnvelope) -> Result<(), String> {
    warbase::prelude::apply_envelope(world, env)
}

/// The `--replay <file>` entry: parse the log, verify the map fingerprint,
/// and reconcile through [`replay_lab`]. Ok carries the checkpoint chain.
pub fn replay_file(
    log_path: &std::path::Path,
    map_path: &std::path::Path,
) -> Result<Vec<(u32, u64)>, String> {
    let text = std::fs::read_to_string(log_path)
        .map_err(|e| format!("cannot read replay {}: {e}", log_path.display()))?;
    let log: MatchInputLog = ron::from_str(&text).map_err(|e| {
        format!("cannot parse {} as a MatchInputLog (RON payload): {e}", log_path.display())
    })?;
    let hash = map_file_hash(map_path)?;
    if log.map_hash != hash {
        return Err(format!(
            "map fingerprint mismatch: log wants {:#x}, {} hashes {:#x} — pass the recording's map via --map <path>",
            log.map_hash,
            map_path.display(),
            hash
        ));
    }
    // Fingerprint check only — the lab composition loads the map itself.
    let bundle = warbase::bundle::load_bundle(map_path)
        .map_err(|e| format!("cannot load map {}: {e}", map_path.display()))?;
    replay_lab(&bundle, &log)
}

// ---------------------------------------------------------------------------
// The record-mode plugin (--record)
// ---------------------------------------------------------------------------

/// Records the next match: arms `RecordingLog` at Startup and auto-writes
/// `runs/<timestamp>.jsonl` when `MatchFlow` leaves `Running`. Added by
/// main.rs only under `--record` — absent otherwise, zero drift.
pub struct ReplayIoPlugin {
    pub map_path: Option<std::path::PathBuf>,
}

impl Plugin for ReplayIoPlugin {
    fn build(&self, app: &mut App) {
        let map_path = self
            .map_path
            .clone()
            .unwrap_or_else(crate::systems::gameplay::default_map_path);
        app.add_systems(Startup, move |world: &mut World| {
            if let Err(e) = arm_recording(world, &map_path) {
                eprintln!("[rts-lab] recording not armed: {e}");
            }
        })
        // After Tick: MatchFlow flipped this very update; the writer beats the
        // next frame's hash checkpoint (no post-end fingerprints may leak).
        .add_systems(Update, write_run_on_match_ended.after(warbase::WarbaseSet::Tick));
    }
}

/// Writes the finished run exactly once (resource removal is the idempotence).
fn write_run_on_match_ended(world: &mut World) {
    if world.get_resource::<RecordingLog>().is_none() {
        return;
    }
    if *world.resource::<MatchFlow>() == MatchFlow::Running {
        return;
    }
    let log = world
        .remove_resource::<RecordingLog>()
        .expect("presence checked above")
        .log;
    match write_input_log(std::path::Path::new(RUNS_DIR), &log) {
        Ok(path) => info!("[rts-lab] match recorded -> {}", path.display()),
        Err(e) => eprintln!("[rts-lab] recording failed to write: {e}"),
    }
}
