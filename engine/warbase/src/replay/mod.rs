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

/// Version of the input-log format itself. SEAM3c kept it at 1: the
/// envelope vocabulary only GREW (`Key` became live, `Card` added) — a
/// superset, so pre-SEAM3c logs still verify byte for byte; bump only on a
/// breaking (re-encoding) change.
pub const INPUT_LOG_VERSION: u32 = 1;

/// Hash checkpoint cadence, in frames (plan §10 RPL1: every 30 frames).
pub const HASH_INTERVAL: u32 = 30;

/// `CommandEnvelope::Ext` kind tag for econ commands (RON payload).
pub const ECON_KIND: &str = "econ";

/// Parse a recorded key word ("KeyE" / "Digit1" / "Space"...) back into a
/// `KeyCode` (SEAM3c): covers the letters, digits and the LT2 navigation
/// specials — the vocabulary the recorder writes. Unknown words return None
/// (the replayer reports a loud error, never a silent drop).
pub fn parse_key_code(code: &str) -> Option<KeyCode> {
    let letters = [
        ("KeyA", KeyCode::KeyA), ("KeyB", KeyCode::KeyB), ("KeyC", KeyCode::KeyC),
        ("KeyD", KeyCode::KeyD), ("KeyE", KeyCode::KeyE), ("KeyF", KeyCode::KeyF),
        ("KeyG", KeyCode::KeyG), ("KeyH", KeyCode::KeyH), ("KeyI", KeyCode::KeyI),
        ("KeyJ", KeyCode::KeyJ), ("KeyK", KeyCode::KeyK), ("KeyL", KeyCode::KeyL),
        ("KeyM", KeyCode::KeyM), ("KeyN", KeyCode::KeyN), ("KeyO", KeyCode::KeyO),
        ("KeyP", KeyCode::KeyP), ("KeyQ", KeyCode::KeyQ), ("KeyR", KeyCode::KeyR),
        ("KeyS", KeyCode::KeyS), ("KeyT", KeyCode::KeyT), ("KeyU", KeyCode::KeyU),
        ("KeyV", KeyCode::KeyV), ("KeyW", KeyCode::KeyW), ("KeyX", KeyCode::KeyX),
        ("KeyY", KeyCode::KeyY), ("KeyZ", KeyCode::KeyZ),
        ("Digit1", KeyCode::Digit1), ("Digit2", KeyCode::Digit2),
        ("Digit3", KeyCode::Digit3), ("Digit4", KeyCode::Digit4),
        ("Digit5", KeyCode::Digit5), ("Digit6", KeyCode::Digit6),
        ("Digit7", KeyCode::Digit7), ("Digit8", KeyCode::Digit8),
        ("Digit9", KeyCode::Digit9), ("Digit0", KeyCode::Digit0),
        ("Space", KeyCode::Space), ("Enter", KeyCode::Enter),
        ("Escape", KeyCode::Escape), ("Tab", KeyCode::Tab),
        ("ArrowUp", KeyCode::ArrowUp), ("ArrowDown", KeyCode::ArrowDown),
        ("ArrowLeft", KeyCode::ArrowLeft), ("ArrowRight", KeyCode::ArrowRight),
    ];
    letters
        .iter()
        .find(|(word, _)| *word == code)
        .map(|(_, k)| *k)
}

/// Append one envelope to a recording log at the CURRENT frame (SEAM3c):
/// same-frame entries merge into one list (the replay driver applies one
/// entry per frame). Game-side recorders use this for vocabulary the base
/// recorder does not know (loop-td's E key) — same precedent as the
/// game-side econ filtering at write time.
pub fn append_envelope(log: &mut RecordingLog, env: CommandEnvelope) {
    let frame = log.frame + 1; // 1-based number of THIS update
    match log.log.frames.last_mut() {
        Some((f, envs)) if *f == frame => envs.push(env),
        _ => log.log.frames.push((frame, vec![env])),
    }
}

/// One recorded player input (frame index lives in the log entry).
/// RPL1 may refine variants, but they stay serde-only, Entity-free.
///
/// ENTITY-FREE CONTRACT (hole-10, SEAM2b — pinned on the enum): command
/// messages (`UnitCommand` & co.) carry `Entity` subjects/targets which are
/// neither serializable nor stable across runs. The recorder therefore
/// encodes every command by SELECTION SEMANTICS: subjects are re-derived at
/// replay time from the replayer's own `SelectionState` (the current
/// selection set / its first entity / the subgroup `subgroup_cursor` points
/// at — the same derivation `selection::smart_resolve` applies), and
/// targets ride as ground points (`to`) or authored ids (`kind`/`unit`/
/// `id`/`slot`). What those resolve to must be a pure function of the
/// replayed world state, which determinism (D6) guarantees. The vocabulary
/// below is the pinned surface; the full recorder/replayer wiring for these
/// variants is RPL follow-up — SEAM only nails the words.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum CommandEnvelope {
    Pick {
        world: [f32; 2],
        button: PickButton,
        clicks: u8,
        ctrl: bool,
        shift: bool,
    },
    /// A raw key press (SEAM3c, the LT2 letter keys E/Q/C/V vocabulary and
    /// any game key a game-side recorder appends). Re-injected as a
    /// `KeyboardInput` MESSAGE — bevy's own PreUpdate input system turns it
    /// into the same `ButtonInput::just_pressed` the original run saw, so
    /// game consumers observe the press in the SAME numbered frame.
    Key {
        code: String,
    },
    /// A command-card action dispatch (SEAM3c): the ui vocabulary's
    /// `CommandAction` encoded as a stable word ("build:arrow" / "upgrade"
    /// / "sell" / "cast:2" / "custom:dig") — see `ui::CommandAction`'s word
    /// codec. Selection is re-derived from the replayer's own `Selection`
    /// resource at apply time (the envelope stays Entity-free).
    Card {
        action: String,
    },
    WaveStart,
    /// Selection-set operation. `op` is a small stable opcode: 0 = clear,
    /// 1 = select same type on screen (double-click), 2 = box select
    /// follows as a `Pick` pair in a later revision; game-specific ops
    /// start at 16. Subjects/extent are re-derived from selection state.
    Select {
        op: u8,
    },
    /// A unit order issued to the current selection (subjects = the
    /// replayer's current selection, filtered like smart_resolve).
    /// `kind` is the order word: "move", "attack_move", "attack_target",
    /// "hold", "stop", "patrol", "pickup", "follow", "harvest",
    /// "return_cargo", "build" (SEAM2d Order vocabulary); `to` is the
    /// ground point (or the ordered pair's first point for patrol).
    Order {
        kind: String,
        to: [f32; 2],
    },
    /// Enqueue training of `unit` on the CURRENT SELECTION's first
    /// production building (re-derived at replay time).
    Train {
        unit: String,
    },
    /// Start research `id` on the player research column.
    Research {
        id: String,
    },
    /// Use inventory `slot` of the current selection's first inventory
    /// holder.
    UseItem {
        slot: u8,
    },
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
/// update (the same visibility the original consumer saw). SEAM3c: pub —
/// the spectate driver reuses exactly this re-injection; `Key` rides a
/// `KeyboardInput` message (real-input channel, see the variant docs) and
/// `Card` re-dispatches through the ui command stream.
pub fn apply_envelope(world: &mut World, env: &CommandEnvelope) -> Result<(), String> {
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
            let key = parse_key_code(code)
                .ok_or_else(|| format!("unknown key word {code:?} (vocabulary: KeyA-Z/Digit0-9/space-enter-escape-tab-arrows)"))?;
            world.write_message(bevy::input::keyboard::KeyboardInput {
                key_code: key,
                logical_key: bevy::input::keyboard::Key::Character(code.clone().into()),
                state: bevy::input::ButtonState::Pressed,
                window: bevy::ecs::entity::Entity::PLACEHOLDER,
                text: None,
                repeat: false,
            });
        }
        CommandEnvelope::Card { action } => {
            #[cfg(feature = "ui")]
            {
                let action = crate::ui::CommandAction::from_envelope_word(action)
                    .ok_or_else(|| format!("unknown card action word {action:?}"))?;
                let selection = world
                    .get_resource::<crate::ui::Selection>()
                    .map(|s| s.mode.clone())
                    .unwrap_or_default();
                world.write_message(crate::ui::CommandCardAction { action, selection });
            }
            #[cfg(not(feature = "ui"))]
            {
                return Err(format!(
                    "unhandled envelope Card({action}) — replay needs the ui feature"
                ));
            }
        }
        // Hole-10 vocabulary: serde-stable and round-trip pinned, but no
        // recorder writes them yet — replaying one is a loud error rather
        // than silent divergence (the RPL follow-up wires both ends).
        CommandEnvelope::Select { op } => {
            return Err(format!("unhandled envelope Select({op}) — vocabulary pinned, recorder lands with RPL follow-up"));
        }
        CommandEnvelope::Order { kind, .. } => {
            return Err(format!("unhandled envelope Order({kind}) — vocabulary pinned, recorder lands with RPL follow-up"));
        }
        CommandEnvelope::Train { unit } => {
            return Err(format!("unhandled envelope Train({unit}) — vocabulary pinned, recorder lands with RPL follow-up"));
        }
        CommandEnvelope::Research { id } => {
            return Err(format!("unhandled envelope Research({id}) — vocabulary pinned, recorder lands with RPL follow-up"));
        }
        CommandEnvelope::UseItem { slot } => {
            return Err(format!("unhandled envelope UseItem({slot}) — vocabulary pinned, recorder lands with RPL follow-up"));
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
        // SEAM3c: the raw-key re-injection channel — registered here so
        // `apply_envelope(Key)` is writable on ANY composition (headless
        // test apps never add bevy's InputPlugin; the message just sits
        // unread there, harmless).
        app.add_message::<bevy::input::keyboard::KeyboardInput>();
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
        append_envelope, apply_envelope, parse_key_code, replay, state_hash, CommandEnvelope,
        ECON_KIND, HASH_INTERVAL, MatchInputLog, RecordingLog, ReplayPlugin, INPUT_LOG_VERSION,
    };
}
