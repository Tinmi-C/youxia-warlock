//! PRD1 acceptance: building train queues — pay on enqueue, frame-exact
//! completion spawn, 100% cancel refund, queue cap, population-gated pause.

use bevy::prelude::*;
use std::collections::HashMap;
use warbase::prelude::*;
use warbase::testing::{send, test_app};

fn unit_def(id: &str, cost_gold: u32) -> UnitDef {
    UnitDef {
        leak: 1,
        id: id.into(),
        attrs: BaseAttributes { hp: 10.0, move_speed: 1.0, body_radius: 0.3 },
        attack: None,
        armor_type: ArmorType::Light,
        cost_gold,
        cost_wood: 0,
        bounty_gold: 0,
        bounty_wood: 0,
        grant_xp: 0,
        traits: vec![],
        auras: vec![],
        hero: false,
        skills: vec![],
        upgrade_to: None,
        heal: None,
        mana: None,
    }
}

/// App with one barracks builder (queue auto-attached after the spawn
/// frame), gold 1000, population 0/10, train table barracks -> footman
/// (50g, 5s = 300 frames at the fixed 1/60 step).
fn prd_app() -> (App, Entity) {
    let mut app = test_app();
    let mut defs = HashMap::new();
    defs.insert("barracks".to_string(), unit_def("barracks", 0));
    defs.insert("footman".to_string(), unit_def("footman", 50));
    app.insert_resource(UnitRegistry { defs });
    let mut res = Resources::default();
    res.gold = 1000;
    app.insert_resource(res);
    let mut pop = Population::default();
    pop.cap = 10;
    app.insert_resource(pop);
    let mut trains = HashMap::new();
    trains.insert("barracks".to_string(), vec!["footman".to_string()]);
    app.insert_resource(TrainTable(trains));
    let mut times = HashMap::new();
    times.insert("footman".to_string(), 5.0);
    app.insert_resource(TrainTimes(times));

    let builder = app
        .world_mut()
        .spawn((
            UnitInstance { def: "barracks".into(), level: 1 },
            Transform::from_translation(Vec3::new(10.0, 0.0, 4.0)),
        ))
        .id();
    app.update(); // Added<UnitInstance> observer attaches the queue
    (app, builder)
}

fn trained_units(app: &mut App, def: &str) -> Vec<Entity> {
    let mut q = app.world_mut().query::<(Entity, &UnitInstance)>();
    q.iter(app.world()).filter(|(_, i)| i.def == def).map(|(e, _)| e).collect()
}

fn queue_len(app: &App, builder: Entity) -> usize {
    app.world().get::<ProductionQueue>(builder).unwrap().entries.len()
}

/// UT-PRD-01: enqueue pays immediately and the entry lands in the queue;
/// non-trainable targets are skipped silently.
#[test]
fn enqueue_pays_immediately() {
    let (mut app, builder) = prd_app();
    assert!(
        app.world().get::<ProductionQueue>(builder).is_some(),
        "queue attached on the spawn frame"
    );
    send(&mut app, TrainCommand { builder, unit: "footman".into() });
    assert_eq!(app.world().resource::<Resources>().gold, 1000 - 50);
    assert_eq!(queue_len(&app, builder), 1);

    // Not trainable by this building -> skipped without charging.
    send(&mut app, TrainCommand { builder, unit: "barracks".into() });
    assert_eq!(app.world().resource::<Resources>().gold, 950);
    assert_eq!(queue_len(&app, builder), 1);

    // Zero drift: unit ids absent from the table never grow a queue.
    let stray = app.world_mut().spawn(UnitInstance { def: "footman".into(), level: 1 }).id();
    app.update();
    assert!(app.world().get::<ProductionQueue>(stray).is_none());
}

/// UT-PRD-02: a 5s train spawns on exactly frame 300 (frame 299: nothing),
/// takes one population, empties the queue, Defender side, beside the
/// builder.
#[test]
fn five_second_train_completes_on_frame_300() {
    let (mut app, builder) = prd_app();
    send(&mut app, TrainCommand { builder, unit: "footman".into() }); // frame 1
    for _ in 0..298 {
        app.update(); // frames 2..=299
    }
    assert_eq!(trained_units(&mut app, "footman").len(), 0, "frame 299: not spawned yet");
    app.update(); // frame 300
    let spawned = trained_units(&mut app, "footman");
    assert_eq!(spawned.len(), 1, "frame 300: unit on-board");
    assert_eq!(app.world().resource::<Population>().used, 1);
    assert_eq!(queue_len(&app, builder), 0);
    let e = spawned[0];
    assert!(matches!(app.world().get::<Allegiance>(e), Some(Allegiance::Defender)));
    assert_eq!(
        app.world().get::<Transform>(e).unwrap().translation,
        Vec3::new(10.0 + SPAWN_OFFSET, 0.0, 4.0)
    );
}

/// UT-PRD-03: cancel at 2s (120 frames) -> 100% refund, no spawn ever,
/// queue empty.
#[test]
fn cancel_after_2s_refunds_and_never_spawns() {
    let (mut app, builder) = prd_app();
    send(&mut app, TrainCommand { builder, unit: "footman".into() });
    for _ in 0..119 {
        app.update(); // 120 frames of training in total
    }
    let progressed = app.world().get::<ProductionQueue>(builder).unwrap().entries.front().unwrap().remaining;
    assert!(
        progressed > 2.0 && progressed < 5.0,
        "trained ~2s (remaining {progressed})"
    );
    send(&mut app, CancelTrain { builder, index: 0 });
    assert_eq!(app.world().resource::<Resources>().gold, 1000, "100% refund");
    assert_eq!(queue_len(&app, builder), 0);
    for _ in 0..300 {
        app.update(); // well past the original 5s window
    }
    assert!(trained_units(&mut app, "footman").is_empty(), "cancelled entry never spawns");
}

/// UT-PRD-04: the 6th same-frame command past cap 5 is skipped — queue
/// stays at 5, only five charges land.
#[test]
fn sixth_train_past_cap_is_skipped() {
    let (mut app, builder) = prd_app();
    {
        let mut msgs =
            app.world_mut().resource_mut::<bevy::ecs::message::Messages<TrainCommand>>();
        for _ in 0..6 {
            msgs.write(TrainCommand { builder, unit: "footman".into() });
        }
    }
    app.update();
    assert_eq!(queue_len(&app, builder), DEFAULT_QUEUE_CAP as usize);
    assert_eq!(app.world().resource::<Resources>().gold, 1000 - 5 * 50);
}

/// UT-PRD-05: full population pauses training (frozen remaining, readable
/// paused flag); freeing a slot resumes and completes the train.
#[test]
fn full_population_pauses_until_freed() {
    let (mut app, builder) = prd_app();
    app.world_mut().resource_mut::<Population>().used = 10; // cap 10 -> full
    send(&mut app, TrainCommand { builder, unit: "footman".into() });
    // Paid on enqueue; paused set on the same frame, remaining untouched.
    assert_eq!(app.world().resource::<Resources>().gold, 950);
    let q = app.world().get::<ProductionQueue>(builder).unwrap();
    assert!(q.paused);
    assert_eq!(q.entries.front().unwrap().remaining, 5.0);

    for _ in 0..60 {
        app.update(); // one second while full
    }
    let q = app.world().get::<ProductionQueue>(builder).unwrap();
    assert!(q.paused, "still paused while population is full");
    assert_eq!(q.entries.front().unwrap().remaining, 5.0, "remaining frozen");
    assert!(trained_units(&mut app, "footman").is_empty());

    // Free one slot: training resumes and completes (5s from zero).
    app.world_mut().resource_mut::<Population>().used = 9;
    app.update(); // resume frame: paused clears, first subtraction
    let q = app.world().get::<ProductionQueue>(builder).unwrap();
    assert!(!q.paused);
    for _ in 0..299 {
        app.update();
    }
    assert_eq!(trained_units(&mut app, "footman").len(), 1, "resumed train completes");
    assert_eq!(
        app.world().resource::<Population>().used,
        10,
        "population re-filled by the graduate"
    );
    assert_eq!(queue_len(&app, builder), 0);
}
