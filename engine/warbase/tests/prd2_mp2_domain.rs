//! PRD2 + PRD3 + MP2 acceptance: player research (pay on start, stat grants
//! through the ResearchSource marker, build/skill unlocks, 100% cancel
//! refund, busy-column rejection, exactly one ResearchComplete sfx), rally
//! points (UnitCommand decoupling from movement), and LastTeamStanding team
//! elimination with structured winner.

use bevy::ecs::message::{MessageCursor, Messages};
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

/// Fighter def (AttackKit carrier) for LastTeamStanding tests.
fn armed_def(id: &str) -> UnitDef {
    let mut d = unit_def(id, 0);
    d.attack = Some(AttackDef {
        damage: 5.0,
        interval: 1.0,
        range: 2.0,
        attack_type: AttackType::Normal,
        targets: TargetDomain::Ground,
        projectile: None,
    });
    d
}

fn research_def(id: &str, cost_gold: u32, time: f32, grants: Vec<ResearchGrant>) -> ResearchDef {
    ResearchDef { id: id.into(), cost_gold, time, grants }
}

fn push_research(app: &mut App, cmd: ResearchCommand) {
    app.world_mut().resource_mut::<ResearchCommands>().0.push_back(cmd);
    app.update();
}

fn push_rally(app: &mut App, cmd: RallyCommand) {
    app.world_mut().resource_mut::<RallyCommands>().0.push_back(cmd);
    app.update();
}

/// Advance `frames` updates, returning how many `ResearchComplete` sfx
/// requests were written (per-frame cursor drain — messages are
/// double-buffered).
fn run_counting_research_sfx(app: &mut App, frames: usize) -> usize {
    let mut cursor = MessageCursor::<SfxRequest>::default();
    let mut seen = 0;
    for _ in 0..frames {
        app.update();
        let msgs = app.world().resource::<Messages<SfxRequest>>();
        seen += cursor.read(msgs).filter(|r| r.kind == SfxKind::ResearchComplete).count();
    }
    seen
}

/// Advance `frames` updates, collecting every UnitCommand's (order,
/// subjects) pair.
fn run_collecting_unit_commands(app: &mut App, frames: usize) -> Vec<(Order, Vec<Entity>)> {
    let mut cursor = MessageCursor::<UnitCommand>::default();
    let mut out = vec![];
    for _ in 0..frames {
        app.update();
        let msgs = app.world().resource::<Messages<UnitCommand>>();
        out.extend(cursor.read(msgs).map(|c| (c.order.clone(), c.subjects.clone())));
    }
    out
}

/// App with a tech tree: atk-up (100g, 2s = 120 frames) granting footman
/// +50% damage, build "castle" and skill "bash"; def-up (50g, 1s) for the
/// busy-column test. One field footman already on board.
fn research_app() -> (App, Entity) {
    let mut app = test_app();
    let mut defs = HashMap::new();
    defs.insert("barracks".to_string(), unit_def("barracks", 0));
    defs.insert("footman".to_string(), unit_def("footman", 0));
    app.insert_resource(UnitRegistry { defs });
    let mut res = Resources::default();
    res.gold = 1000;
    app.insert_resource(res);
    let table = ResearchTable(HashMap::from([
        (
            "atk-up".to_string(),
            research_def(
                "atk-up",
                100,
                2.0,
                vec![
                    ResearchGrant::Stat { unit: "footman".into(), kind: StatKind::DamageMult, amount: 0.5 },
                    ResearchGrant::UnlockBuild("castle".into()),
                    ResearchGrant::UnlockSkill("bash".into()),
                ],
            ),
        ),
        (
            "def-up".to_string(),
            research_def("def-up", 50, 1.0, vec![]),
        ),
    ]));
    app.insert_resource(table);

    let registry = app.world().resource::<UnitRegistry>().clone();
    let field_unit = spawn_unit(
        &mut app.world_mut(),
        &registry,
        "footman",
        Vec3::ZERO,
        Allegiance::Defender,
        1.0,
    )
    .unwrap();
    app.update(); // aggregate: neutral EffectiveStats on the field unit
    let eff = app.world().get::<EffectiveStats>(field_unit).unwrap();
    assert!((eff.damage_mult - 1.0).abs() < 1e-6, "baseline damage_mult 1.0");
    (app, field_unit)
}

/// UT-PRD2-01: start pays 100 immediately; completion on frame 120 applies
/// the stat grant (TypeModifiers hit + same-frame EffectiveStats on an
/// existing unit + on a freshly spawned same-id unit, source == the
/// ResearchSource marker), records both unlocks, and emits EXACTLY ONE
/// ResearchComplete sfx.
#[test]
fn research_completes_with_grants_unlocks_and_one_sfx() {
    let (mut app, field_unit) = research_app();
    push_research(&mut app, ResearchCommand::Start { id: "atk-up".into() });
    assert_eq!(app.world().resource::<Resources>().gold, 900, "pays on start");
    assert_eq!(app.world().resource::<ResearchState>().entries.len(), 1);

    // Push consumed frame 1; a 2s research at 1/60 lands on frame 120.
    // f32 accumulation of 120 sequential subtractions may cross zero one
    // frame late (deterministic per build) — pin the tolerance to 1 frame.
    let mut cursor = MessageCursor::<SfxRequest>::default();
    let (mut seen, mut completion_frame) = (0, 0);
    for frame in 2..=130 {
        app.update();
        let msgs = app.world().resource::<Messages<SfxRequest>>();
        seen += cursor.read(msgs).filter(|r| r.kind == SfxKind::ResearchComplete).count();
        if seen == 1 && completion_frame == 0 {
            completion_frame = frame;
        }
    }
    assert_eq!(seen, 1, "exactly one ResearchComplete");
    assert!(
        (120..=121).contains(&completion_frame),
        "2s research completes on frame 120 (+/- 1 f32 frame), got {completion_frame}"
    );
    assert!(!app.world().resource::<TypeModifiers>().0.is_empty(), "grants applied by then");

    // Stat grant landed in the type table and folds into the field unit.
    let table = app.world().resource::<TypeModifiers>();
    assert_eq!(
        table.0.get(&("footman".to_string(), StatKind::DamageMult)),
        Some(&0.5),
        "TypeModifiers hit"
    );
    let eff = app.world().get::<EffectiveStats>(field_unit).unwrap();
    assert!((eff.damage_mult - 1.5).abs() < 1e-6, "field unit damage_mult 1.5, got {}", eff.damage_mult);
    // Pinned design: the ModEntry is sourced from the global marker.
    let marker = app.world().resource::<ResearchSource>().0;
    let mods = app.world().get::<StatMods>(field_unit).unwrap();
    assert!(mods.entries.iter().any(|e| e.source == marker
        && e.kind == StatKind::DamageMult
        && (e.amount - 0.5).abs() < 1e-6));

    // A freshly spawned same-id unit inherits the completed research.
    let registry = app.world().resource::<UnitRegistry>().clone();
    let rookie = spawn_unit(&mut app.world_mut(), &registry, "footman", Vec3::new(3.0, 0.0, 0.0), Allegiance::Defender, 1.0).unwrap();
    app.update();
    let eff = app.world().get::<EffectiveStats>(rookie).unwrap();
    assert!((eff.damage_mult - 1.5).abs() < 1e-6, "rookie inherits the grant");

    // Unlocks recorded and queryable; unrelated ids stay unrevealed.
    let unlocks = app.world().resource::<Unlocks>();
    assert!(is_build_unlocked(unlocks, "castle"));
    assert!(is_skill_unlocked(unlocks, "bash"));
    assert!(!is_build_unlocked(unlocks, "tower"));
    assert!(!is_skill_unlocked(unlocks, "storm"));

    // No further completions while idle.
    let seen = run_counting_research_sfx(&mut app, 60);
    assert_eq!(seen, 0, "still exactly one over the whole run");
    assert_eq!(app.world().resource::<ResearchState>().entries.len(), 0);
}

/// UT-PRD2-02: cancel mid-progress refunds 100%, clears the column, and
/// nothing ever completes (no grants, no sfx).
#[test]
fn research_cancel_refunds_100pct_and_never_completes() {
    let (mut app, _) = research_app();
    push_research(&mut app, ResearchCommand::Start { id: "atk-up".into() });
    let _ = run_counting_research_sfx(&mut app, 60); // ~1s of progress
    push_research(&mut app, ResearchCommand::Cancel);
    assert_eq!(app.world().resource::<Resources>().gold, 1000, "100% refund");
    assert_eq!(app.world().resource::<ResearchState>().entries.len(), 0);

    let seen = run_counting_research_sfx(&mut app, 300);
    assert_eq!(seen, 0, "cancelled research never completes");
    assert!(app.world().resource::<TypeModifiers>().0.is_empty());
    assert!(!is_build_unlocked(app.world().resource::<Unlocks>(), "castle"));
}

/// UT-PRD2-03: a second start while the single column is busy is rejected
/// without charging; an empty tech tree rejects starts silently (zero
/// drift for maps without research).
#[test]
fn second_start_rejected_while_busy_without_charge() {
    let (mut app, _) = research_app();
    push_research(&mut app, ResearchCommand::Start { id: "atk-up".into() });
    assert_eq!(app.world().resource::<Resources>().gold, 900);
    push_research(&mut app, ResearchCommand::Start { id: "def-up".into() });
    assert_eq!(app.world().resource::<Resources>().gold, 900, "busy column: no charge");
    assert_eq!(app.world().resource::<ResearchState>().entries.len(), 1);

    // Unknown / empty-table ids never charge either.
    push_research(&mut app, ResearchCommand::Start { id: "nope".into() });
    assert_eq!(app.world().resource::<Resources>().gold, 900);
    assert_eq!(app.world().resource::<ResearchState>().entries.len(), 1);
}

/// PRD3 app: the PRD1 layout (barracks trains footman 50g/5s, gold 1000,
/// pop 0/10) with the queue attached.
fn rally_app() -> (App, Entity) {
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
    app.update(); // queue attaches
    (app, builder)
}

fn footmen(app: &mut App) -> Vec<Entity> {
    let mut q = app.world_mut().query::<(Entity, &UnitInstance)>();
    q.iter(app.world()).filter(|(_, i)| i.def == "footman").map(|(e, _)| e).collect()
}

/// UT-PRD3-01: a builder with RallyPoint(Some(Move)) emits EXACTLY ONE
/// UnitCommand{order: Move, subjects: [graduate]} on the completion frame.
#[test]
fn rally_point_orders_graduate_via_unit_command() {
    let (mut app, builder) = rally_app();
    let to = Vec3::new(20.0, 0.0, 20.0);
    push_rally(&mut app, RallyCommand::Set { builder, order: Order::Move { to } });
    assert_eq!(
        app.world().get::<RallyPoint>(builder).unwrap().0,
        Some(Order::Move { to }),
        "rally readable on the builder"
    );

    send(&mut app, TrainCommand { builder, unit: "footman".into() }); // frame 1
    let cmds = run_collecting_unit_commands(&mut app, 298); // frames 2..=299
    assert!(cmds.is_empty(), "no command before completion");
    let cmds = run_collecting_unit_commands(&mut app, 1); // frame 300
    let spawned = footmen(&mut app);
    assert_eq!(spawned.len(), 1, "graduate on board");
    assert_eq!(cmds.len(), 1, "exactly one UnitCommand");
    assert_eq!(cmds[0].0, Order::Move { to }, "carries the rally order");
    assert_eq!(cmds[0].1, vec![spawned[0]], "subject is the graduate");
}

/// UT-PRD3-02: without a rally point (never set, or set then cleared) the
/// graduate holds — zero UnitCommands over the whole run.
#[test]
fn no_rally_means_no_command_and_hold() {
    // (a) never set.
    let (mut app, builder) = rally_app();
    send(&mut app, TrainCommand { builder, unit: "footman".into() });
    let cmds = run_collecting_unit_commands(&mut app, 300);
    assert_eq!(cmds.len(), 0, "no rally: no UnitCommand");
    assert_eq!(footmen(&mut app).len(), 1);

    // (b) set then cleared -> RallyPoint(None) reads back, still no order.
    let to = Vec3::new(5.0, 0.0, 5.0);
    push_rally(&mut app, RallyCommand::Set { builder, order: Order::Move { to } });
    push_rally(&mut app, RallyCommand::Clear { builder });
    assert_eq!(app.world().get::<RallyPoint>(builder).unwrap().0, None);
    send(&mut app, TrainCommand { builder, unit: "footman".into() });
    let cmds = run_collecting_unit_commands(&mut app, 300);
    assert_eq!(cmds.len(), 0, "cleared rally: no UnitCommand");
}

/// UT-MP2-01: LastTeamStanding — two hostile teams with live fighters keep
/// the match running; eliminating team A ends the match with team B the
/// structured winner.
#[test]
fn last_team_standing_running_then_b_wins() {
    let mut app = test_app();
    app.insert_resource(MatchRules { win: WinCondition::LastTeamStanding, ..Default::default() })
        .insert_resource(MatchDiplomacy {
            players: vec![(PlayerId(0), TeamId(0)), (PlayerId(1), TeamId(1))],
            hostile_pairs: vec![(TeamId(0), TeamId(1))],
        });
    let mut defs = HashMap::new();
    defs.insert("grunt".to_string(), armed_def("grunt"));
    app.insert_resource(UnitRegistry { defs });
    let registry = app.world().resource::<UnitRegistry>().clone();

    let a = spawn_unit(&mut app.world_mut(), &registry, "grunt", Vec3::ZERO, Allegiance::Defender, 1.0)
        .unwrap();
    app.world_mut().entity_mut(a).insert(Owner(PlayerId(0)));
    let b = spawn_unit(&mut app.world_mut(), &registry, "grunt", Vec3::new(5.0, 0.0, 0.0), Allegiance::Attacker, 1.0)
        .unwrap();
    app.world_mut().entity_mut(b).insert(Owner(PlayerId(1)));
    app.update();

    assert_eq!(*app.world().resource::<MatchFlow>(), MatchFlow::Running, "two hostile teams: running");
    assert!(app.world().get_resource::<MatchWinner>().is_none());

    // Team A eliminated (combat pipeline would despawn; direct despawn is
    // the same post-cleanup_dead state).
    app.world_mut().despawn(a);
    app.update();

    assert_eq!(
        *app.world().resource::<MatchFlow>(),
        MatchFlow::Won("last team standing: team 1 wins".into()),
        "team B (id 1) wins when A is eliminated"
    );
    let mut cursor = MessageCursor::<MatchEnded>::default();
    let msgs = app.world().resource::<Messages<MatchEnded>>();
    let last = cursor.read(msgs).last().cloned();
    assert_eq!(last.map(|m| m.outcome), Some(Ok("last team standing: team 1 wins".into())));
    assert_eq!(app.world().get_resource::<MatchWinner>(), Some(&MatchWinner(TeamId(1))));

    // Transition is once-only: further frames change nothing.
    app.update();
    app.update();
    assert_eq!(*app.world().resource::<MatchFlow>(), MatchFlow::Won("last team standing: team 1 wins".into()));
}

/// UT-MP2-02: zero drift — default rules (Endless) never take the team
/// elimination branch, even when one side's fighters all die; and maps
/// without a tech tree/rally keep the PRD2/3 pipeline inert.
#[test]
fn default_rules_and_bare_maps_stay_inert() {
    let mut app = test_app();
    let mut defs = HashMap::new();
    defs.insert("grunt".to_string(), armed_def("grunt"));
    app.insert_resource(UnitRegistry { defs });
    let registry = app.world().resource::<UnitRegistry>().clone();
    let d = spawn_unit(&mut app.world_mut(), &registry, "grunt", Vec3::ZERO, Allegiance::Defender, 1.0).unwrap();
    let a = spawn_unit(&mut app.world_mut(), &registry, "grunt", Vec3::new(5.0, 0.0, 0.0), Allegiance::Attacker, 1.0).unwrap();
    for _ in 0..10 {
        app.update();
    }
    assert_eq!(*app.world().resource::<MatchFlow>(), MatchFlow::Running);

    app.world_mut().despawn(a);
    for _ in 0..10 {
        app.update();
    }
    assert_eq!(
        *app.world().resource::<MatchFlow>(),
        MatchFlow::Running,
        "Endless never takes the LastTeamStanding branch"
    );

    // Bare map: empty research table rejects starts without charging, and
    // the empty TypeModifiers fast path touches nothing.
    let gold_before = app.world().resource::<Resources>().gold;
    push_research(&mut app, ResearchCommand::Start { id: "atk-up".into() });
    assert_eq!(app.world().resource::<Resources>().gold, gold_before);
    assert!(app.world().resource::<TypeModifiers>().0.is_empty());
    let eff = app.world().get::<EffectiveStats>(d).unwrap();
    assert!((eff.damage_mult - 1.0).abs() < 1e-6, "no research, no drift");
}
