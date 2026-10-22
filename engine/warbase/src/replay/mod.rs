//! Replay (WC3 alignment, ADR-0016): record the player INPUT stream and
//! replay it against the deterministic base (MatchRng + fixed timestep +
//! D6 discipline are the foundations this harvests).
//!
//! Input-level, not command-level: the envelope never carries Entity ids
//! (not serializable, not stable across runs) — replays re-inject inputs and
//! let the logic layers derive commands, WC3-style. RPL1 lands the headless
//! recorder + replay driver; RPL2 the real-machine entry points.
//!
//! Scheduling (plan §2 D2): the two recorder systems run FIRST in Update,
//! `before(WarbaseSet::Tick)`, chained (hash, then inputs):
//! - `record_state_hash` pins the world fingerprint every `HASH_INTERVAL`
//!   frames — entry `(F, h)` = world settled AFTER update `F` (the recorder
//!   observes it at the start of update `F + 1`; frame 0 = assembled initial
//!   state).
//! - `record_inputs` appends the messages first seen this update — entry
//!   `(F, envs)` = inputs consumed BY update `F`. Same-frame inputs keep
//!   arrival order within a message type; across types the order is fixed
//!   (Pick, WaveStart, Econ) since true cross-store arrival order is
//!   unobservable.
//! Both short-circuit unless a `RecordingLog` resource is present: no
//! resource, no recording, zero drift for warden/loop-td (opt-in).
//!
//! Replay symmetry: inputs are re-written BETWEEN updates (exactly how the
//! original run received them — externally, between frames), so Tick-side
//! consumers see them in the same frame both times.
//!
//! Three seams to keep in mind (RPL2 must revisit):
//! - Engine-internal `EconCommand` writers (`run_director`'s reward / early
//!   call grants) are re-DERIVED during replay; recording them would
//!   double-apply. RPL1 scenarios therefore run with `reward_gold: 0` and
//!   `early_call_bonus: None`; RPL2 must tag player-origin at the write site.
//! - The base is currently RNG-free (only game-side code consumes MatchRng),
//!   so a wrong seed would not diverge behaviorally. `state_hash`
//!   fingerprints the seed resource to catch mis-seeded replays at the first
//!   checkpoint regardless; drop that term once a base mechanism consumes
//!   MatchRng.
//! - bevy 0.19 allocates a bare bookkeeping ENTITY on first-time resource
//!   registration, so the recording app's resource set must match the
//!   replay app's exactly or every later entity index (and the (index, hp)
//!   fingerprint pairs) shifts — see the symmetry note in `replay`.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::bundle::MapBundle;
use crate::econ::{EconCommand, Population, Resources};
use crate::rng::MatchRng;
use crate::selection::{PickAttempt, PickButton};
use crate::unit::Hp;
use crate::wave::StartWave;

/// Version of the input-log format itself (bump on envelope changes).
pub const INPUT_LOG_VERSION: u32 = 1;

/// Hash checkpoint cadence, in frames (plan §10 RPL1: every 30 frames).
pub const HASH_INTERVAL: u32 = 30;

/// `CommandEnvelope::Ext` kind tag for econ commands (RON payload).
pub const ECON_KIND: &str = "econ";

/// One recorded player input (frame index lives in the log entry).
/// RPL1 may refine variants, but they stay serde-only, Entity-free.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum CommandEnvelope {
    Pick {
        world: [f32; 2],
        button: PickButton,
        clicks: u8,
        ctrl: bool,
        shift: bool,
    },
    Key {
        code: String,
    },
    WaveStart,
    /// Escape hatch for domain-specific inputs until RPL1 finalizes the
    /// vocabulary (kind + RON/JSON payload string).
    Ext { kind: String, payload: String },
}

/// The recorded log: map fingerprint, seed, and per-frame inputs.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MatchInputLog {
    pub version: u32,
    pub map_hash: u64,
    pub seed: u64,
    /// (frame, inputs applied at that frame).
    pub frames: Vec<(u32, Vec<CommandEnvelope>)>,
    /// State fingerprint every `HASH_INTERVAL` frames (replay reconciliation).
    pub hashes: Vec<(u32, u64)>,
}

/// Opt-in recording state: insert this resource on a running app to start
/// recording, remove it to stop. `frame` counts COMPLETED updates and is
/// maintained by the recorder systems; the caller fills
/// `log.version/map_hash/seed` at insert time.
#[derive(Resource, Debug, Default)]
pub struct RecordingLog {
    /// Updates completed so far (0 before the first update runs).
    pub frame: u32,
    /// The growing input log.
    pub log: MatchInputLog,
}

/// FNV-1a (64-bit): fixed algorithm, no per-run keys — the fingerprint must
/// never depend on process randomness (D6 discipline).
struct Fnv1a(u64);

impl Fnv1a {
    fn new() -> Self {
        Fnv1a(0xcbf2_9ce4_8422_2325)
    }

    fn mix(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 ^= u64::from(b);
            self.0 = self.0.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }

    fn u8(&mut self, v: u8) {
        self.mix(&[v]);
    }

    fn u32(&mut self, v: u32) {
        self.mix(&v.to_le_bytes());
    }

    fn u64(&mut self, v: u64) {
        self.mix(&v.to_le_bytes());
    }
}

/// World fingerprint for replay reconciliation. Pinned recipe (documented
/// deviation from the plan's minimal list: the MatchRng seed resource rides
/// along — see the module header):
/// - total entity count,
/// - Resources(gold, wood) + Population(used, cap),
/// - the match seed,
/// - every entity's Hp as (Entity::index, f32 bits) pairs.
///
/// Deterministic order (D6): Query iteration order is unspecified, so Hp
/// entries are collected and sorted by entity index before hashing —
/// identical spawn/despawn histories yield identical sequences. Hashing the
/// (index, bits) PAIRS (instead of a sum) also makes the fingerprint
/// insensitive to float summation order entirely.
pub fn state_hash(world: &World) -> u64 {
    let mut h = Fnv1a::new();
    h.u64(world.entities().len() as u64);
    let res = world.resource::<Resources>();
    h.u32(res.gold);
    h.u32(res.wood);
    let pop = world.resource::<Population>();
    h.u8(pop.used);
    h.u8(pop.cap);
    h.u64(world.resource::<MatchRng>().seed());
    let mut hps: Vec<(u32, u32)> = world
        .iter_entities()
        .filter_map(|e| e.get::<Hp>().map(|hp| (e.id().index_u32(), hp.0.to_bits())))
        .collect();
    hps.sort_unstable();
    h.u64(hps.len() as u64);
    for (index, bits) in hps {
        h.u32(index);
        h.u32(bits);
    }
    h.0
}

/// Exclusive (full-World access, no other params possible): appends the
/// `(frame, hash)` checkpoint when `frame` (= completed updates) hits the
/// cadence. Short-circuits without a `RecordingLog` — one resource lookup,
/// zero world mutation, zero drift for non-recording apps.
fn record_state_hash(world: &mut World) {
    let Some(log) = world.get_resource::<RecordingLog>() else {
        return;
    };
    let frame = log.frame;
    if frame % HASH_INTERVAL != 0 {
        return;
    }
    let hash = state_hash(world);
    world.resource_mut::<RecordingLog>().log.hashes.push((frame, hash));
}

/// Input recorder: reads the three input-level message types (anything
/// Entity-carrying is out by the envelope contract) and appends this frame's
/// envelopes. Short-circuits without a `RecordingLog`.
fn record_inputs(
    log: Option<ResMut<RecordingLog>>,
    mut picks: MessageReader<PickAttempt>,
    mut starts: MessageReader<StartWave>,
    mut econ: MessageReader<EconCommand>,
) {
    let Some(mut log) = log else {
        return;
    };
    let frame = log.frame + 1; // 1-based number of THIS update
    let mut envs = Vec::new();
    for p in picks.read() {
        envs.push(CommandEnvelope::Pick {
            world: p.world.to_array(),
            button: p.button,
            clicks: p.click_count,
            ctrl: p.ctrl,
            shift: p.shift,
        });
    }
    for _ in starts.read() {
        envs.push(CommandEnvelope::WaveStart);
    }
    for c in econ.read() {
        // EconCommand is Entity-free and serde (derive authorized by the
        // RPL1 task); everything else about it stays opaque in the payload.
        let payload = ron::to_string(c).expect("EconCommand serializes");
        envs.push(CommandEnvelope::Ext { kind: ECON_KIND.into(), payload });
    }
    if !envs.is_empty() {
        log.log.frames.push((frame, envs));
    }
    log.frame = frame;
}

/// Convert an envelope back into its message and write it for the upcoming
/// update (the same visibility the original consumer saw).
fn apply_envelope(world: &mut World, env: &CommandEnvelope) -> Result<(), String> {
    match env {
        CommandEnvelope::Pick { world: pos, button, clicks, ctrl, shift } => {
            world.write_message(PickAttempt {
                world: Vec2::new(pos[0], pos[1]),
                button: *button,
                click_count: *clicks,
                ctrl: *ctrl,
                shift: *shift,
            });
        }
        CommandEnvelope::Key { code } => {
            return Err(format!("unhandled envelope Key({code}) — RPL1 records no keys"));
        }
        CommandEnvelope::WaveStart => {
            world.write_message(StartWave);
        }
        CommandEnvelope::Ext { kind, payload } => {
            if kind != ECON_KIND {
                return Err(format!("unhandled Ext kind {kind:?}"));
            }
            let cmd: EconCommand =
                ron::from_str(payload).map_err(|e| format!("bad {ECON_KIND} payload {payload:?}: {e}"))?;
            world.write_message(cmd);
        }
    }
    Ok(())
}

/// Hash checkpoint: record the replay's own chain into `actual` and compare
/// against the next log entry when the frames align. `hi` is the cursor of
/// the first not-yet-reconciled log entry.
fn reconcile(
    log: &MatchInputLog,
    hi: &mut usize,
    frame: u32,
    world: &World,
    actual: &mut Vec<(u32, u64)>,
) -> Result<(), String> {
    let hash = state_hash(world);
    actual.push((frame, hash));
    while *hi < log.hashes.len() && log.hashes[*hi].0 < frame {
        *hi += 1;
    }
    if let Some(&(f, expect)) = log.hashes.get(*hi) {
        if f == frame && hash != expect {
            return Err(format!(
                "state hash mismatch at frame {frame}: recorded {expect:#x}, replayed {hash:#x}"
            ));
        }
    }
    Ok(())
}

/// Replay a recorded input log against a fresh headless app (RPL1). Same
/// recipe as the harness: `test_app` + `MatchRng::seeded(log.seed)` +
/// `assemble` + `build_grid`; then per frame: apply that frame's envelopes
/// (written between updates, like real input) -> `update()` -> every
/// `HASH_INTERVAL` frames reconcile `state_hash` against `log.hashes`. The
/// first divergent frame is reported in the `Err`. Returns the replay's own
/// `(frame, hash)` checkpoint chain on success.
pub fn replay(bundle: &MapBundle, log: &MatchInputLog) -> Result<Vec<(u32, u64)>, String> {
    if log.version != INPUT_LOG_VERSION {
        return Err(format!(
            "input log version {} does not match {} — re-record the match",
            log.version, INPUT_LOG_VERSION
        ));
    }
    let mut app = crate::testing::test_app();
    app.insert_resource(MatchRng::seeded(log.seed));
    crate::bundle::assemble(&mut app.world_mut(), bundle);
    app.insert_resource(crate::bundle::build_grid(&bundle.world));
    // Structural symmetry (bevy 0.19): first-time resource registration
    // allocates a bare bookkeeping ENTITY, so the recording app's
    // `RecordingLog` shifted every later entity index by one — the
    // fingerprint's (index, hp) pairs would never align. Replay inserts a
    // RecordingLog too, reproducing the recording app's allocation sequence
    // exactly; its recorder just re-records the replayed stream (unused).
    let mut rec = RecordingLog::default();
    rec.log.version = log.version;
    rec.log.seed = log.seed;
    app.insert_resource(rec);

    let mut actual = Vec::new();
    let mut hi = 0usize;
    // Frame-0 checkpoint: the assembled initial state must match before any
    // input applies (pins bundle + seed determinism up front).
    reconcile(log, &mut hi, 0, app.world(), &mut actual)?;

    // Run far enough to reconcile every recorded input AND hash entry.
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
        if frame % HASH_INTERVAL == 0 {
            reconcile(log, &mut hi, frame, app.world(), &mut actual)?;
        }
    }
    Ok(actual)
}

/// Domain systems (registered once by WarbasePlugin; RPL1 adds the recorder
/// HERE, never in lib.rs). Recording is pure logic — feature-free.
pub struct ReplayPlugin;

impl Plugin for ReplayPlugin {
    fn build(&self, app: &mut App) {
        // Hash first (it snapshots the PREVIOUS update's settled state under
        // the pre-increment frame counter), then inputs (which advance it).
        app.add_systems(
            Update,
            (record_state_hash, record_inputs)
                .chain()
                .before(crate::WarbaseSet::Tick),
        );
    }
}

pub mod prelude {
    pub use super::{
        replay, state_hash, CommandEnvelope, ECON_KIND, HASH_INTERVAL, MatchInputLog, RecordingLog,
        ReplayPlugin, INPUT_LOG_VERSION,
    };
}
