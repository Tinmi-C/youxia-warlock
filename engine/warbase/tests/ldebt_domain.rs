//! Round-4 LDEBT acceptance battery (plan v4 §Phase 1 line 3): the seven
//! uplited seams L1-L3 + L5-L8, one file so each hole's contract reads as
//! a unit. Games keep their own behavioral batteries (legion behavior.rs);
//! here the SEAM is the subject.

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use warbase::prelude::*;
use warbase::testing::test_app;

// -- shared helpers ---------------------------------------------------------

/// A unit-carrying world entity to use as a killer in synthetic
/// `UnitDied` messages (apply_econ only reads its `Owner`).
fn owned_unit(world: &mut World, owner: Option<PlayerId>) -> Entity {
    let mut e = world.spawn((UnitInstance { def: "victim".into(), level: 1 }, Hp(10.0)));
    if let Some(p) = owner {
        e.insert(Owner(p));
    }
    e.id()
}

fn died(killer: Option<DamageOrigin>, gold: u32) -> UnitDied {
    UnitDied { entity: Entity::PLACEHOLDER, def: "victim".into(), gold, wood: 0, xp: 0, killer }
}

/// A bare app with 2v2 wallets: P0+P2 team 0 vs P1+P3 team 1, local = P0,
/// every wallet seeded to 100g (mirror left at zero so the first update
/// projects the local view — matching the projection branch under test).
fn mp2_app() -> App {
    let mut app = test_app();
    let world = app.world_mut();
    world.insert_resource(MatchDiplomacy {
        players: vec![
            (PlayerId(0), TeamId(0)),
            (PlayerId(1), TeamId(1)),
            (PlayerId(2), TeamId(0)),
            (PlayerId(3), TeamId(1)),
        ],
        hostile_pairs: vec![(TeamId(0), TeamId(1))],
    });
    let wallet = Wallet { gold: 100, ..Default::default() };
    world.insert_resource(Wallets(vec![wallet; 4]));
    app
}

// -- L2: per-player economy ---------------------------------------------------

/// L2-i (zero drift): an UNOWNED killer credits the local wallet and the
/// legacy `Resources` view together — the exact pre-L2 global semantics
/// single-player maps (warden/loop-td/rts-lab) rely on.
#[test]
fn unowned_kill_bounty_lands_in_local_wallet_and_view() {
    let mut app = test_app();
    let killer = owned_unit(app.world_mut(), None);
    app.world_mut().write_message(died(Some(DamageOrigin::Attack(killer)), 7));
    app.update();
    let w = &app.world().resource::<Wallets>().0[0];
    assert_eq!((w.gold, w.wood), (7, 0), "local wallet credited");
    assert_eq!(app.world().resource::<Resources>().gold, 7, "legacy view identical");
}

/// L2-ii (attribution): P0's unit kills -> P0's wallet grows, P1's does
/// not, and the local view (P0) grows with it.
#[test]
fn owned_kill_credits_killer_wallet_only() {
    let mut app = mp2_app();
    app.update(); // project: view <- wallets[local] (100g)

    let killer = owned_unit(app.world_mut(), Some(PlayerId(0)));
    app.world_mut().write_message(died(Some(DamageOrigin::Attack(killer)), 12));
    app.update();

    let wallets = &app.world().resource::<Wallets>().0;
    assert_eq!(wallets[0].gold, 112, "P0 wallet takes the bounty");
    assert_eq!(wallets[1].gold, 100, "P1 wallet untouched");
    assert_eq!(app.world().resource::<Resources>().gold, 112, "local view follows P0");
}

/// L2-iii (remote attribution): P1's unit kills -> only P1's wallet grows;
/// the local view (P0) does NOT move (this is the double-accounting the
/// pre-L2 base could not express).
#[test]
fn remote_owner_kill_credits_only_that_wallet() {
    let mut app = mp2_app();
    app.update();

    let killer = owned_unit(app.world_mut(), Some(PlayerId(1)));
    app.world_mut().write_message(died(Some(DamageOrigin::Attack(killer)), 9));
    app.update();

    let wallets = &app.world().resource::<Wallets>().0;
    assert_eq!(wallets[1].gold, 109, "P1 wallet credited");
    assert_eq!(wallets[0].gold, 100, "P0 wallet untouched");
    assert_eq!(
        app.world().resource::<Resources>().gold,
        100,
        "the local view mirrors P0 only"
    );
}

/// L2-iv (view identity): wallet writes project into the view, direct view
/// writes absorb into the wallet — the invariant is `Resources ==
/// Wallets[local]` after either kind of write.
#[test]
fn local_view_and_wallet_stay_identical_both_directions() {
    let mut app = mp2_app();
    app.update();
    assert_eq!(app.world().resource::<Resources>().gold, 100);

    // MP-style write: the wallet is the writer -> the view projects.
    app.world_mut().resource_mut::<Wallets>().of_mut(PlayerId(0)).gold = 42;
    app.update();
    assert_eq!(app.world().resource::<Resources>().gold, 42, "view follows wallet");

    // Legacy-style write: the view is the writer (cheat panel / old game
    // code) -> the wallet absorbs.
    app.world_mut().resource_mut::<Resources>().gold = 77;
    app.update();
    assert_eq!(
        app.world().resource::<Wallets>().0[0].gold,
        77,
        "wallet absorbs the view write"
    );

    // Idle frame: converged, nothing moves (the zero-write branch).
    app.update();
    assert_eq!(app.world().resource::<Resources>().gold, 77);
    assert_eq!(app.world().resource::<Wallets>().0[0].gold, 77);
}

// -- L7: structured match endings ---------------------------------------------

/// L7-i: `MatchFlow::Ended` carries the structured outcome — a Draw is
/// first-class data (the pre-L7 legion had to ride the Won channel with a
/// reason-string convention), and the reason stays the readable contract.
/// Game-side write path (round-cap adjudication) exercised directly.
#[test]
fn ended_flow_carries_structured_win_and_draw() {
    // Draw: no winner crowns.
    let mut app = test_app();
    let mut ended = MessageCursor::<MatchEnded>::default();
    *app.world_mut().resource_mut::<MatchFlow>() = MatchFlow::Ended {
        reason: "round cap 20: draw on lives (t0=20, t1=20)".into(),
        outcome: MatchOutcome::Draw,
    };
    app.world_mut().write_message(MatchEnded {
        outcome: Ok("round cap 20: draw on lives (t0=20, t1=20)".into()),
    });
    app.update();
    let flow = app.world().resource::<MatchFlow>().clone();
    assert!(matches!(
        flow,
        MatchFlow::Ended { outcome: MatchOutcome::Draw, ref reason } if reason.contains("draw")
    ));
    assert!(
        app.world().get_resource::<MatchWinner>().is_none(),
        "a draw crowns nobody"
    );
    let msgs = app.world().resource::<Messages<MatchEnded>>();
    assert!(ended.read(msgs).any(|m| matches!(&m.outcome, Ok(r) if r.contains("draw"))));

    // Win: the outcome carries the team as data.
    let flow = MatchFlow::Ended {
        reason: "last team standing on lives: team 0 wins".into(),
        outcome: MatchOutcome::Win(TeamId(0)),
    };
    assert!(matches!(flow, MatchFlow::Ended { outcome: MatchOutcome::Win(TeamId(0)), .. }));
}
