//! SEAM0 regression (ADR-0016 alignment plan): the seam increment must be
//! invisible to existing consumers — zero drift — while pinning the new
/// domain contracts. These tests pin the seam itself.

use bevy::math::Vec2;
use warbase::prelude::*;

/// Default Hostility reproduces the historical two-sided semantics exactly
/// (the zero-drift contract every combat/buff call site now routes through).
#[test]
fn hostility_default_is_two_sided() {
    let h = Hostility;
    use Allegiance::{Attacker, Defender};
    assert!(h.hostile(Defender, Attacker));
    assert!(h.hostile(Attacker, Defender));
    assert!(!h.hostile(Defender, Defender));
    assert!(!h.hostile(Attacker, Attacker));
    assert!(h.allied(Defender, Defender));
    assert!(h.allied(Attacker, Attacker));
    assert!(!h.allied(Defender, Attacker));
}

/// Default diplomacy = single player vs environment (the historical
/// Defender/Attacker split in team form).
#[test]
fn default_diplomacy_single_player() {
    let d = MatchDiplomacy::default();
    assert_eq!(d.players, vec![(PlayerId(0), TeamId(0))]);
    assert_eq!(d.hostile_pairs, vec![(TeamId(0), TeamId(1))]);
}

/// A pre-ADR-0016 map file (no new optional sections) still loads and
/// validates — the superset-compatibility contract of MAP_FORMAT_VERSION.
#[test]
fn old_map_without_new_sections_loads() {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    for rel in [
        "../../games/warden/maps/warden.ron",
        "../../games/loop-td/maps/loop.ron",
    ] {
        let path = std::path::Path::new(manifest_dir).join(rel);
        let bundle = warbase::bundle::load_bundle(&path)
            .unwrap_or_else(|e| panic!("{} failed to load: {e}", rel));
        // New sections materialized as defaults.
        assert!(bundle.nav.blocked.is_empty());
        assert!(bundle.items.items.is_empty());
        assert!(bundle.production.research.is_empty());
        assert!(bundle.teams.players.is_empty());
        assert_eq!(bundle.vision.cell, 1.0);
    }
}

/// The seam registers every new message/resource on a bare app: consumers
/// may write them without manual setup.
#[test]
fn seam_messages_and_resources_registered() {
    let mut app = warbase::testing::test_app();
    // Resources present out of the box.
    app.world().resource::<Hostility>();
    app.world().resource::<SelectionState>();
    app.world().resource::<ControlGroups>();
    app.world().resource::<ItemRegistry>();
    app.world().resource::<NavGrid>();
    app.world().resource::<MatchDiplomacy>();
    // Messages are writable without panicking (registration check).
    warbase::testing::send(
        &mut app,
        PickAttempt { world: Vec2::ZERO, button: PickButton::Left, click_count: 1, ctrl: false, shift: false },
    );
    warbase::testing::send(&mut app, SfxRequest { kind: SfxKind::WaveStart });
    warbase::testing::send(
        &mut app,
        UnitCommand { order: Order::HoldPosition, subjects: Vec::new() },
    );
}

/// Pinned vocabulary is serde-clean: the replay envelope round-trips (it
/// must never carry an Entity — replay determinism contract).
#[test]
fn command_envelope_roundtrip() {
    let e = CommandEnvelope::Pick {
        world: [1.5, -2.0],
        button: PickButton::Right,
        clicks: 2,
        ctrl: true,
        shift: false,
    };
    let text = ron::to_string(&e).unwrap();
    let back: CommandEnvelope = ron::from_str(&text).unwrap();
    assert_eq!(e, back);
    let log = MatchInputLog {
        version: INPUT_LOG_VERSION,
        map_hash: 7,
        seed: 42,
        frames: vec![(3, vec![CommandEnvelope::WaveStart, e])],
        hashes: vec![(3, 99)],
    };
    let text = ron::to_string(&log).unwrap();
    let back: MatchInputLog = ron::from_str(&text).unwrap();
    assert_eq!(back.frames.len(), 1);
    assert_eq!(back.hashes, vec![(3, 99)]);
}
