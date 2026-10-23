//! RPL2 (WC3 alignment, ADR-0016): the real-machine record/replay entry
//! points for loop-td.
//!
//! - `--record` (main.rs): adds `ReplayIoPlugin`, which arms the base
//!   `RecordingLog` at Startup and writes `runs/<timestamp>.jsonl` when the
//!   match ends (`MatchEnded` — win or lose). The file EXTENSION follows the
//!   runs/ convention; the PAYLOAD is the base RON `MatchInputLog` (same
//!   format `--replay` parses and RPL1 round-trips).
//! - `--replay <file>` (main.rs): headless verification entry — loads the
//!   log, checks the map fingerprint, and re-runs the match through
//!   `warbase::replay` (per-frame input re-injection + `StateHash`
//!   reconciliation). Prints the verdict and exits 0/1. Watching a replay
//!   with live rendering is the future spectator mode; phase 1 validates
//!   recording authenticity only.
//!
//! PHASE-1 BOUNDARY (documented): the base `replay()` reconstructs a
//! BASE-ONLY app (`test_app` + assemble + grid). Logs recorded in that same
//! composition (verification matches — the acceptance tests here) reconcile
//! exactly. Logs recorded from the FULL game app (DefaultPlugins + the
//! loop-td game systems) additionally fingerprint renderer/game-side
//! entities and effects (end-screen nodes, hero grants, snipe effects), so
//! base-only reconciliation of a real-app log fails at the first checkpoint
//! by design; replaying those needs the spectator composition (RPL3+).
//!
//! ECON FILTERING (the RPL1 engine-writer constraint): loop-td has NO
//! player-origin `EconCommand` writers — the build path (`try_place` /
//! `try_sell` / `try_upgrade`) mutates `Resources` directly, so every
//! recorded `EconCommand` message is engine-internal (wave rewards, early
//! call bonuses) and is re-DERIVED by `run_director` during replay. Recording
//! them would double-apply, so `write_input_log` drops `Ext { kind: ECON }`
//! envelopes at write time (the RPL1 module header's "filter at the source"
//! option, implemented game-side because the write site lives in the base).

use bevy::prelude::*;
use warbase::prelude::{
    MatchFlow, MatchInputLog, MatchRng, RecordingLog, ECON_KIND, INPUT_LOG_VERSION,
};

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

/// Map fingerprint: FNV-1a over the map FILE bytes (stable, order-free —
/// unlike serializing the bundle, whose HashMap iteration order varies).
pub fn map_file_hash(path: &std::path::Path) -> Result<u64, String> {
    let bytes = std::fs::read(path)
        .map_err(|e| format!("cannot read map {}: {e}", path.display()))?;
    Ok(fnv1a(&bytes))
}

/// Arm recording on a world: inserts the base `RecordingLog` carrying the
/// match seed and the map fingerprint (idempotent — an existing log wins).
/// Composition note (bevy 0.19 resource-entity symmetry, see the replay
/// module header): call this as the LAST new-resource insert of the app
/// setup, mirroring `warbase::replay`'s own RecordingLog insert.
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

/// Write a finished input log to `<dir>/<UTC timestamp>.jsonl` (RON payload)
/// with engine-internal econ envelopes filtered out (module header). Returns
/// the written path.
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
                    .filter(|e| !matches!(e, warbase::prelude::CommandEnvelope::Ext { kind, .. } if kind == ECON_KIND))
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
        .duration_since(std::time::UNIX_EPOCH)
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

/// The `--replay <file>` entry: parse the log, verify the map fingerprint,
/// and reconcile through the base headless replay driver. Ok carries the
/// replay's own `(frame, hash)` checkpoint chain.
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
    let bundle = warbase::bundle::load_bundle(map_path)
        .map_err(|e| format!("cannot load map {}: {e}", map_path.display()))?;
    warbase::prelude::replay(&bundle, &log)
}

// ---------------------------------------------------------------------------
// The record-mode plugin (--record)
// ---------------------------------------------------------------------------

/// Records the next match: arms `RecordingLog` at Startup and auto-writes
/// `runs/<timestamp>.jsonl` when `MatchFlow` leaves `Running`. Added by
/// main.rs only under `--record` — absent otherwise, zero drift for the
/// normal startup path. `map_path` overrides the map fingerprint source
/// (None = the same path `gameplay::load_map` resolves).
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
                eprintln!("[loop-td] recording not armed: {e}");
            }
        })
        // After Tick: MatchFlow has flipped this very update, so the writer
        // beats the next frame's hash checkpoint — no post-end fingerprints
        // (end-screen spawns etc.) may leak into the log.
        .add_systems(
            Update,
            write_run_on_match_ended.after(warbase::WarbaseSet::Tick),
        );
    }
}

/// Writes the finished run exactly once (resource removal is the
/// idempotence — the FIRST match of a session is the recorded one; restarts
/// start fresh matches and phase 1 keeps them unrecorded).
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
        Ok(path) => info!("[loop-td] match recorded -> {}", path.display()),
        Err(e) => eprintln!("[loop-td] recording failed to write: {e}"),
    }
}
