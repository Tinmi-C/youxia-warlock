//! MP1 acceptance (ADR-0017): diplomacy-backed `Hostility` oracle with zero
//! drift under the default single-player layout, team hostility matrix,
//! Owner-derived Allegiance, and `apply_teams` bundle dispatch.

use warbase::prelude::*;

/// Three-team layout: A(0) with P0+P1, B(1) with P2, C(2) with P3;
/// hostile pairs (A,B) and (B,C); A-C neutral.
fn three_team_diplomacy() -> MatchDiplomacy {
    MatchDiplomacy {
        players: vec![
            (PlayerId(0), TeamId(0)),
            (PlayerId(1), TeamId(0)),
            (PlayerId(2), TeamId(1)),
            (PlayerId(3), TeamId(2)),
        ],
        hostile_pairs: vec![(TeamId(0), TeamId(1)), (TeamId(1), TeamId(2))],
    }
}

/// The zero-drift contract: under default diplomacy the oracle answers
/// exactly as the historical two-sided comparison (mirror of
/// tests/seam.rs::hostility_default_is_two_sided, all ordered pairs, both
/// fns). Checked for a constructed default AND for the resource combat/buff
/// actually read in an assembled app.
fn assert_two_sided(h: &Hostility) {
    use Allegiance::{Attacker, Defender};
    assert!(h.hostile(Defender, Attacker));
    assert!(h.hostile(Attacker, Defender));
    assert!(!h.hostile(Defender, Defender));
    assert!(!h.hostile(Attacker, Attacker));
    assert!(h.allied(Defender, Defender));
    assert!(h.allied(Attacker, Attacker));
    assert!(!h.allied(Defender, Attacker));
    assert!(!h.allied(Attacker, Defender));
}

/// ① Default diplomacy = zero drift, construction paths agree, and the
/// registered resources on a bare app are the defaults.
#[test]
fn default_hostility_is_two_sided_zero_drift() {
    assert_two_sided(&Hostility::default());
    assert_two_sided(&Hostility::from(&MatchDiplomacy::default()));
    let app = warbase::testing::test_app();
    assert_eq!(
        app.world().resource::<MatchDiplomacy>().players,
        vec![(PlayerId(0), TeamId(0))]
    );
    assert_eq!(
        app.world().resource::<MatchDiplomacy>().hostile_pairs,
        vec![(TeamId(0), TeamId(1))]
    );
    assert_two_sided(app.world().resource::<Hostility>());
}

/// ② Three-team nine-cell hostility matrix is exactly the authored pairs
/// (symmetric, empty diagonal); A-C is neutral — neither hostile nor allied.
#[test]
fn three_team_hostility_matrix() {
    let h = Hostility::from(&three_team_diplomacy());
    let teams = [TeamId(0), TeamId(1), TeamId(2)];
    let grid = [
        [false, true, false],
        [true, false, true],
        [false, true, false],
    ];
    for (i, &a) in teams.iter().enumerate() {
        for (j, &b) in teams.iter().enumerate() {
            assert_eq!(h.teams_hostile(a, b), grid[i][j], "hostile({a:?},{b:?})");
            assert_eq!(h.teams_allied(a, b), i == j, "allied({a:?},{b:?})");
        }
    }
    // Neutral pair pinned explicitly: A-C neither hostile nor allied.
    assert!(!h.teams_hostile(TeamId(0), TeamId(2)));
    assert!(!h.teams_allied(TeamId(0), TeamId(2)));
    // Allegiance-level answers stay exact in this layout: Defender = team 0,
    // Attacker = environment team 1 (smallest team hostile to team 0).
    assert!(h.hostile(Allegiance::Defender, Allegiance::Attacker));
    assert!(!h.hostile(Allegiance::Attacker, Allegiance::Attacker));
    assert!(!h.hostile(Allegiance::Defender, Allegiance::Defender));
    assert!(h.allied(Allegiance::Attacker, Allegiance::Attacker));
    assert!(!h.allied(Allegiance::Defender, Allegiance::Attacker));
}

/// ③ Owner consumption: P0's unit (team 0) is hostile to the Attacker side
/// and not hostile to a same-team mate; an enemy-owned unit derives to
/// Attacker; a neutral-team owner folds to Defender (documented rule).
#[test]
fn owner_derived_allegiance_drives_hostility() {
    let d = three_team_diplomacy();
    let h = Hostility::from(&d);
    let p0 = d.allegiance_of(PlayerId(0));
    let mate = d.allegiance_of(PlayerId(1));
    let foe = d.allegiance_of(PlayerId(2));
    assert_eq!(p0, Allegiance::Defender);
    assert_eq!(mate, Allegiance::Defender);
    assert_eq!(foe, Allegiance::Attacker);
    // Owner(P0) unit vs Attacker side: hostile.
    assert!(h.hostile(p0, Allegiance::Attacker));
    // vs same-team mate: not hostile, allied.
    assert!(!h.hostile(p0, mate));
    assert!(h.allied(p0, mate));
    // Enemy-owned unit vs P0's unit: hostile (both directions).
    assert!(h.hostile(foe, p0));
    assert!(h.hostile(p0, foe));
    // Neutral team owner folds to Defender (non-combatant, not auto-target).
    assert_eq!(d.allegiance_of(PlayerId(3)), Allegiance::Defender);
    // Team lookup helpers: listed and unlisted players.
    assert_eq!(h.team_of(PlayerId(2)), Some(TeamId(1)));
    assert_eq!(h.team_of(PlayerId(9)), None);
    // Unlisted owner conservatively folds to Defender.
    assert_eq!(d.allegiance_of(PlayerId(9)), Allegiance::Defender);
}

fn warden_bundle() -> warbase::bundle::MapBundle {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../games/warden/maps/warden.ron");
    warbase::bundle::load_bundle(&path).expect("warden.ron loads")
}

/// ④a `apply_teams` with an empty teams section (every shipped map) leaves
/// the default diplomacy in place — the zero-drift path for real bundles.
#[test]
fn apply_teams_empty_section_yields_default_diplomacy() {
    let bundle = warden_bundle();
    assert!(bundle.teams.players.is_empty());
    let mut app = warbase::testing::test_app();
    warbase::bundle::assemble(&mut app.world_mut(), &bundle);
    assert_eq!(
        app.world().resource::<MatchDiplomacy>().players,
        vec![(PlayerId(0), TeamId(0))]
    );
    assert_eq!(
        app.world().resource::<MatchDiplomacy>().hostile_pairs,
        vec![(TeamId(0), TeamId(1))]
    );
    assert_two_sided(app.world().resource::<Hostility>());
}

/// ④b `apply_teams` with a populated section constructs the diplomacy (and
/// the Hostility snapshot answers from it) exactly as authored.
#[test]
fn apply_teams_populated_section_builds_table() {
    let mut bundle = warden_bundle();
    bundle.teams = TeamsDef {
        players: vec![(0, 0), (1, 0), (2, 1), (3, 2)],
        hostile_pairs: vec![(0, 1), (1, 2)],
    };
    let mut app = warbase::testing::test_app();
    warbase::bundle::assemble(&mut app.world_mut(), &bundle);
    assert_eq!(
        app.world().resource::<MatchDiplomacy>().players,
        three_team_diplomacy().players
    );
    assert_eq!(
        app.world().resource::<MatchDiplomacy>().hostile_pairs,
        vec![(TeamId(0), TeamId(1)), (TeamId(1), TeamId(2))]
    );
    let h = app.world().resource::<Hostility>();
    assert!(h.teams_hostile(TeamId(0), TeamId(1)));
    assert!(!h.teams_hostile(TeamId(0), TeamId(2)));
    assert!(h.hostile(Allegiance::Defender, Allegiance::Attacker));
}

/// TeamsDef serde defaults: a section listing players but no hostile pairs
/// gets [(0, 1)]; a wholly absent section (struct default) leaves both vecs
/// empty so `apply_teams` takes the default-diplomacy path.
#[test]
fn teams_def_serde_defaults() {
    let def: TeamsDef = ron::from_str("(players: [(0, 0), (1, 1)])").unwrap();
    assert_eq!(def.players, vec![(0, 0), (1, 1)]);
    assert_eq!(def.hostile_pairs, vec![(0, 1)]);
    assert_eq!(TeamsDef::default().players, Vec::<(u8, u8)>::new());
    assert_eq!(TeamsDef::default().hostile_pairs, Vec::<(u8, u8)>::new());
}
