//! Sfx semantic channel (WC3 alignment, ADR-0016): the base emits semantic
//! audio requests at gameplay seams; games attach listeners that map kinds
//! to assets. No audio code and no assets live here (art is out of scope).
//!
//! Phase 1 (AUD1) is REACTIVE ONLY — zero intrusion: the systems below READ
//! existing gameplay messages and translate them into `SfxRequest`; they
//! never mutate another domain. A world with no listener is unaffected
//! (written messages nobody reads are harmless — warden/loop-td zero drift).
//!
//! Command-Ack contract (PINNED; Phase 1 does not implement it): every
//! player command produces EXACTLY ONE Ack or Error message, emitted by the
//! OWNING command domain itself (the ClickAck/OrderAck/Error kinds exist
//! for those domains to route through). The sfx channel does not
//! synthesize per-command Acks in Phase 1.
//!
//! Throttle discipline: a second request of the same kind within
//! `THROTTLE_WINDOW` (0.1s of simulation time, hard-coded) is swallowed, so
//! frame-rate-driven spam (many hits in one frame) collapses to one request
//! per kind per window. D6 note: the throttle map is only probed and
//! written per key — no HashMap iteration drives logic, so a HashMap (not
//! BTreeMap) is allowed here.
//!
//! Scheduling (D2): the reactive systems run in Cleanup, after Resolve
//! writes DamageDealt/UnitDied and Director writes WaveStarted/WaveCleared
//! in the same frame. `rules_tick` (MatchEnded) is unordered inside
//! Cleanup, so MatchEnd may surface one frame late — audio tolerance.

use bevy::prelude::*;
use std::collections::HashMap;

/// Semantic sound kinds (grow per domain; names are the contract with the
/// game-side listener).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SfxKind {
    ClickAck,
    Select,
    OrderAck,
    Error,
    Cast,
    Hit,
    Death,
    WaveStart,
    WaveCleared,
    BuildComplete,
    ResearchComplete,
    LevelUp,
    /// rule::MatchEnded — win or lose sting (AUD1 addition).
    MatchEnd,
}

#[derive(Message, Debug, Clone, Copy)]
pub struct SfxRequest {
    pub kind: SfxKind,
}

/// Same-kind requests closer than this are swallowed (0.1s, hard-coded by
/// the AUD1 spec — WC3-style spam collapse).
pub const THROTTLE_WINDOW: f32 = 0.1;

/// Per-kind last-emission timestamps in simulation-time seconds. Written
/// only through [`emit_throttled`]; never iterated (D6).
#[derive(Resource, Debug, Default)]
pub struct SfxThrottle {
    pub last: HashMap<SfxKind, f32>,
}

/// Emit `kind` unless the same kind already passed within
/// [`THROTTLE_WINDOW`]. Shared by every reactive system below so the
/// throttle semantics live in exactly one place.
pub fn emit_throttled(
    kind: SfxKind,
    time: &Time,
    throttle: &mut SfxThrottle,
    sfx: &mut MessageWriter<SfxRequest>,
) {
    let now = time.elapsed_secs();
    let since_last = throttle.last.get(&kind).map(|&t| now - t);
    let passes = match since_last {
        None => true,
        Some(delta) => delta >= THROTTLE_WINDOW,
    };
    if passes {
        throttle.last.insert(kind, now);
        sfx.write(SfxRequest { kind });
    }
}

/// Cleanup (reactive): combat hit landed -> Hit.
pub fn sfx_on_damage(
    time: Res<Time>,
    mut throttle: ResMut<SfxThrottle>,
    mut dealt: MessageReader<crate::combat::DamageDealt>,
    mut sfx: MessageWriter<SfxRequest>,
) {
    for _ in dealt.read() {
        emit_throttled(SfxKind::Hit, &time, &mut throttle, &mut sfx);
    }
}

/// Cleanup (reactive): a unit died -> Death.
pub fn sfx_on_death(
    time: Res<Time>,
    mut throttle: ResMut<SfxThrottle>,
    mut died: MessageReader<crate::combat::UnitDied>,
    mut sfx: MessageWriter<SfxRequest>,
) {
    for _ in died.read() {
        emit_throttled(SfxKind::Death, &time, &mut throttle, &mut sfx);
    }
}

/// Cleanup (reactive): wave began -> WaveStart.
pub fn sfx_on_wave_started(
    time: Res<Time>,
    mut throttle: ResMut<SfxThrottle>,
    mut started: MessageReader<crate::wave::WaveStarted>,
    mut sfx: MessageWriter<SfxRequest>,
) {
    for _ in started.read() {
        emit_throttled(SfxKind::WaveStart, &time, &mut throttle, &mut sfx);
    }
}

/// Cleanup (reactive): wave fully cleared -> WaveCleared.
pub fn sfx_on_wave_cleared(
    time: Res<Time>,
    mut throttle: ResMut<SfxThrottle>,
    mut cleared: MessageReader<crate::wave::WaveCleared>,
    mut sfx: MessageWriter<SfxRequest>,
) {
    for _ in cleared.read() {
        emit_throttled(SfxKind::WaveCleared, &time, &mut throttle, &mut sfx);
    }
}

/// Cleanup (reactive): economy rejected a command -> Error (the command's
/// single Ack-or-Error, see the pinned contract above).
pub fn sfx_on_econ_rejected(
    time: Res<Time>,
    mut throttle: ResMut<SfxThrottle>,
    mut rejected: MessageReader<crate::econ::EconRejected>,
    mut sfx: MessageWriter<SfxRequest>,
) {
    for _ in rejected.read() {
        emit_throttled(SfxKind::Error, &time, &mut throttle, &mut sfx);
    }
}

/// Cleanup (reactive): match over (win or lose) -> MatchEnd.
pub fn sfx_on_match_ended(
    time: Res<Time>,
    mut throttle: ResMut<SfxThrottle>,
    mut ended: MessageReader<crate::rule::MatchEnded>,
    mut sfx: MessageWriter<SfxRequest>,
) {
    for _ in ended.read() {
        emit_throttled(SfxKind::MatchEnd, &time, &mut throttle, &mut sfx);
    }
}

/// Domain systems (registered once by WarbasePlugin; AUD increments add
/// systems HERE, never in lib.rs).
pub struct SfxPlugin;

impl Plugin for SfxPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SfxThrottle>().add_systems(
            Update,
            (
                sfx_on_damage,
                sfx_on_death,
                sfx_on_wave_started,
                sfx_on_wave_cleared,
                sfx_on_econ_rejected,
                sfx_on_match_ended,
            )
                .chain()
                .in_set(crate::WarbaseSet::Cleanup),
        );
    }
}

pub mod prelude {
    pub use super::{
        emit_throttled, sfx_on_damage, sfx_on_death, sfx_on_econ_rejected, sfx_on_match_ended,
        sfx_on_wave_cleared, sfx_on_wave_started, SfxKind, SfxPlugin, SfxRequest, SfxThrottle,
        THROTTLE_WINDOW,
    };
}
