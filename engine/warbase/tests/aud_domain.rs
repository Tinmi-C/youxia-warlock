//! AUD1 acceptance: the sfx channel is a reactive, zero-intrusion translator
//! — it reads existing gameplay messages and emits semantic SfxRequests,
//! with the per-kind 0.1s throttle hard-coded. Assertions (spec §9 AUD1):
//! single hit -> exactly one request; same-frame pair throttled; window
//! elapse -> passes again; direct-fire per kind.

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use warbase::prelude::*;
use warbase::testing::test_app;

/// Frame-stepping collector with ONE persistent cursor per test (messages
/// are double-buffered and survive one extra frame — a fresh cursor per
/// call would re-read the previous call's messages; testing.rs pitfall).
#[derive(Default)]
struct SfxCollector {
    cursor: MessageCursor<SfxRequest>,
}

impl SfxCollector {
    /// Advance `frames` updates and collect every SfxRequest kind written.
    fn collect(&mut self, app: &mut App, frames: usize) -> Vec<SfxKind> {
        let mut kinds = Vec::new();
        for _ in 0..frames {
            app.update();
            let msgs = app.world().resource::<Messages<SfxRequest>>();
            kinds.extend(self.cursor.read(msgs).map(|r| r.kind));
        }
        kinds
    }
}

/// Write one combat hit directly at the seam (DamageDealt, not a full
/// combat setup — the sfx layer must not care how the message was made).
fn write_hit(app: &mut App) {
    let target = app.world_mut().spawn_empty().id();
    app.world_mut().write_message(DamageDealt {
        target,
        source: DamageOrigin::System,
        amount: 10.0,
        crit: false,
        killed: false,
    });
}

/// Write one death directly at the seam.
fn write_death(app: &mut App) {
    let victim = app.world_mut().spawn_empty().id();
    app.world_mut().write_message(UnitDied {
        entity: victim,
        def: "grunt".into(),
        gold: 0,
        wood: 0,
        xp: 0,
        killer: None,
    });
}

/// AUD1-1: one DamageDealt -> exactly one SfxRequest{Hit}.
#[test]
fn single_damage_dealt_emits_exactly_one_hit() {
    let mut app = test_app();
    let mut sfx = SfxCollector::default();
    write_hit(&mut app);
    assert_eq!(sfx.collect(&mut app, 1), vec![SfxKind::Hit]);
}

/// AUD1-2: two DamageDealt in the SAME frame -> still exactly one Hit
/// (the 0.1s per-kind throttle collapses same-frame spam).
#[test]
fn same_frame_damage_pair_is_throttled_to_one_hit() {
    let mut app = test_app();
    let mut sfx = SfxCollector::default();
    write_hit(&mut app);
    write_hit(&mut app);
    assert_eq!(sfx.collect(&mut app, 1), vec![SfxKind::Hit]);
}

/// AUD1-3: 0.1s later (7 frames at 1/60) a third DamageDealt passes again
/// — one more Hit, total exactly two.
#[test]
fn hit_passes_again_after_the_throttle_window() {
    let mut app = test_app();
    let mut sfx = SfxCollector::default();

    write_hit(&mut app);
    assert_eq!(sfx.collect(&mut app, 1), vec![SfxKind::Hit]);

    // 7 quiet frames = 7/60 s > 0.1 s window; nothing may be emitted.
    assert!(sfx.collect(&mut app, 7).is_empty());

    write_hit(&mut app);
    assert_eq!(sfx.collect(&mut app, 1), vec![SfxKind::Hit], "window elapsed");
}

/// AUD1-4a: UnitDied -> Death (direct fire).
#[test]
fn unit_died_emits_death() {
    let mut app = test_app();
    let mut sfx = SfxCollector::default();
    write_death(&mut app);
    assert_eq!(sfx.collect(&mut app, 1), vec![SfxKind::Death]);
}

/// AUD1-4b: WaveStarted -> WaveStart (direct fire).
#[test]
fn wave_started_emits_wave_start() {
    let mut app = test_app();
    let mut sfx = SfxCollector::default();
    app.world_mut().write_message(WaveStarted(3));
    assert_eq!(sfx.collect(&mut app, 1), vec![SfxKind::WaveStart]);
}

/// AUD1-4c: EconRejected -> Error (direct fire; the econ command's single
/// Ack-or-error per the pinned contract).
#[test]
fn econ_rejected_emits_error() {
    let mut app = test_app();
    let mut sfx = SfxCollector::default();
    app.world_mut()
        .write_message(EconRejected { reason: "insufficient funds".into() });
    assert_eq!(sfx.collect(&mut app, 1), vec![SfxKind::Error]);
}

/// AUD1-4d: MatchEnded -> MatchEnd (the AUD1-added kind; collected over two
/// frames — rules_tick is unordered inside Cleanup, one frame of latency is
/// audio-tolerated).
#[test]
fn match_ended_emits_match_end() {
    let mut app = test_app();
    let mut sfx = SfxCollector::default();
    app.world_mut().write_message(MatchEnded { outcome: Ok("survived all waves".into()) });
    assert_eq!(sfx.collect(&mut app, 2), vec![SfxKind::MatchEnd]);
}

/// Throttle is PER KIND: a hit and a death in the same frame each pass —
/// collapsing one kind must not silence the others.
#[test]
fn different_kinds_are_throttled_independently() {
    let mut app = test_app();
    let mut sfx = SfxCollector::default();
    write_hit(&mut app);
    write_death(&mut app);
    let kinds = sfx.collect(&mut app, 1);
    assert_eq!(kinds.len(), 2, "both kinds pass in the same frame");
    assert!(kinds.contains(&SfxKind::Hit));
    assert!(kinds.contains(&SfxKind::Death));
}
