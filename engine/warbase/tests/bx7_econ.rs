//! BX7 acceptance: conversion rate, population cost curve, rejection
//! semantics, kill bounty flow.

use bevy::prelude::*;
use warbase::prelude::*;
use warbase::testing::test_app;

fn econ_write(app: &mut App, cmd: EconCommand) {
    app.world_mut()
        .resource_mut::<bevy::ecs::message::Messages<EconCommand>>()
        .write(cmd);
    app.update();
}

/// UT-ECO-01: wood converts at the shipped rate.
#[test]
fn convert_wood_uses_rules_rate() {
    let mut app = test_app();
    let mut res = Resources::default();
    res.wood = 3;
    app.insert_resource(res).insert_resource(EconRules { wood_to_gold: 25, ..Default::default() });
    econ_write(&mut app, EconCommand::ConvertWood { wood: 3 });
    let r = app.world().resource::<Resources>();
    assert_eq!((r.gold, r.wood), (75, 0));
}

/// UT-ECO-02: population purchases follow the growing cost curve.
#[test]
fn population_cost_grows_per_purchase() {
    let mut app = test_app();
    let mut res = Resources::default();
    res.wood = 100;
    app.insert_resource(res).insert_resource(EconRules { pop_cost_base: 2, pop_cost_growth: 1.5, pop_gain: 2, ..Default::default() });

    econ_write(&mut app, EconCommand::BuyPopulation);
    econ_write(&mut app, EconCommand::BuyPopulation);

    let r = app.world().resource::<Resources>();
    let pop = app.world().resource::<Population>();
    // costs: 2, then round(2*1.5)=3
    assert_eq!(r.wood, 100 - 2 - 3);
    assert_eq!(pop.cap, 4);
    assert_eq!(app.world().resource::<PopPurchases>().0, 2);
}

/// UT-ECO-03: insufficient funds reject without mutating state.
#[test]
fn insufficient_spend_rejects_and_keeps_balance() {
    let mut app = test_app();
    let mut res = Resources::default();
    res.gold = 50;
    app.insert_resource(res).insert_resource(EconRules::default());
    econ_write(&mut app, EconCommand::Spend { gold: 100, wood: 0 });

    let mut cursor = bevy::ecs::message::MessageCursor::<EconRejected>::default();
    let msgs = app.world().resource::<bevy::ecs::message::Messages<EconRejected>>();
    assert_eq!(cursor.read(msgs).count(), 1);
    assert_eq!(app.world().resource::<Resources>().gold, 50);
}

/// UT-ECO-04: kill bounties flow into Resources (one EconChanged per grant).
#[test]
fn kill_bounty_flows_into_resources() {
    let mut app = test_app();
    app.insert_resource(EconRules::default());
    app.world_mut()
        .resource_mut::<bevy::ecs::message::Messages<UnitDied>>()
        .write(UnitDied { entity: Entity::PLACEHOLDER, def: "grunt".into(), gold: 7, wood: 1, xp: 2, killer: None });
    app.update();
    let r = app.world().resource::<Resources>();
    assert_eq!((r.gold, r.wood), (7, 1));
}

/// PopUse clamps at cap and rejects over-cap use.
#[test]
fn pop_use_clamps_at_cap() {
    let mut app = test_app();
    let mut pop = Population::default();
    pop.cap = 2;
    app.insert_resource(pop).insert_resource(EconRules::default());
    econ_write(&mut app, EconCommand::PopUse { delta: 5 });
    assert_eq!(app.world().resource::<Population>().used, 0, "over-cap use rejected without partial apply");

    let mut cursor = bevy::ecs::message::MessageCursor::<EconRejected>::default();
    let msgs = app.world().resource::<bevy::ecs::message::Messages<EconRejected>>();
    assert_eq!(cursor.read(msgs).count(), 1, "over-cap use rejected");

    econ_write(&mut app, EconCommand::PopUse { delta: 2 });
    assert_eq!(app.world().resource::<Population>().used, 2);
    econ_write(&mut app, EconCommand::PopUse { delta: -1 });
    assert_eq!(app.world().resource::<Population>().used, 1);
}
