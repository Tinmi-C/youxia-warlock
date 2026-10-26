//! SEL4 acceptance (alignment plan §4): pick resolution (ordering, body
//! radius boundary, build-grid cell), smart right-click commands, and
//! command hotkeys (H/S direct, A targeting + Esc cancel, P two-click
//! patrol). All headless — picks and hotkeys are injected as messages and
//! `UnitCommand`s are observed at the message boundary (MOV consumption is
//! Phase 3).

use bevy::app::App;
use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use warbase::prelude::*;
use warbase::testing::test_app;

// -- in-file helpers (testing.rs is off-limits for this increment) --------

fn def(id: &str, body_radius: f32) -> UnitDef {
    UnitDef {
        hero_stats: None,
        leak: 1,
        id: id.to_string(),
        attrs: BaseAttributes { hp: 80.0, move_speed: 0.0, body_radius },
        attack: None,
        armor_type: ArmorType::Fortified,
        cost_gold: 0,
        cost_wood: 0,
        bounty_gold: 0,
        bounty_wood: 0,
        grant_xp: 0,
        traits: vec![],
        auras: vec![],
        hero: false,
        skills: vec!["bolt".to_string()],
        upgrade_to: None,
        heal: None,
        mana: None,
    skill_levels: vec![],
    }
}

fn registry(defs: &[UnitDef]) -> UnitRegistry {
    UnitRegistry { defs: defs.iter().cloned().map(|d| (d.id.clone(), d)).collect() }
}

/// Spawn a Selectable Defender unit from a registry def. `pos` is ground
/// (x, z) — hole-1 plane convention: world Vec3 (x, 0, z).
fn spawn_with_def(app: &mut App, reg: &UnitRegistry, id: &str, pos: Vec2) -> Entity {
    let entity = spawn_unit(
        &mut app.world_mut(),
        reg,
        id,
        Vec3::new(pos.x, 0.0, pos.y),
        Allegiance::Defender,
        1.0,
    )
    .expect("def present");
    app.world_mut().entity_mut(entity).insert(Selectable);
    entity
}

fn pick(world: Vec2, button: PickButton) -> PickAttempt {
    PickAttempt { world, button, click_count: 1, ctrl: false, shift: false }
}

fn send(app: &mut App, msg: impl Message) {
    app.world_mut().write_message(msg);
    app.update();
}

/// Drain new `PickResolved`s through a persistent cursor (double-buffer
/// safe: returns exactly what was written since the previous drain).
fn drain_picks(app: &mut App, cursor: &mut MessageCursor<PickResolved>) -> Vec<PickResolved> {
    let messages = app.world().resource::<Messages<PickResolved>>();
    cursor.read(messages).cloned().collect()
}

/// Drain new `UnitCommand`s as (order, subjects) pairs (UnitCommand is not
/// Clone; Order and the subjects vec are).
fn drain_commands(
    app: &mut App,
    cursor: &mut MessageCursor<UnitCommand>,
) -> Vec<(Order, Vec<Entity>)> {
    let messages = app.world().resource::<Messages<UnitCommand>>();
    cursor.read(messages).map(|c| (c.order.clone(), c.subjects.clone())).collect()
}

// -- acceptance assertions (plan §4, SEL4) --------------------------------

/// ① Pick ordering: hits come back nearest-first; equal distances break
/// ties by Entity index (the lower index wins). Gridless app -> cell None.
#[test]
fn picks_resolve_nearest_first_with_index_tiebreak() {
    let mut app = test_app();
    let reg = registry(&[def("unit", 0.6)]);
    app.insert_resource(reg.clone());
    // Spawn far FIRST: far owns the smaller index, so only distance can put
    // near ahead of it.
    let far = spawn_with_def(&mut app, &reg, "unit", Vec2::new(0.0, 0.5));
    let near = spawn_with_def(&mut app, &reg, "unit", Vec2::new(0.0, 0.3));
    assert!(far.index() < near.index(), "precondition: far spawned first");

    let mut picks = MessageCursor::<PickResolved>::default();
    send(&mut app, pick(Vec2::ZERO, PickButton::Left));
    let resolved = drain_picks(&mut app, &mut picks);
    assert_eq!(resolved.len(), 1);
    assert_eq!(resolved[0].entities, vec![near, far], "nearest first");
    assert_eq!(resolved[0].world, Vec2::ZERO);
    assert_eq!(resolved[0].cell, None, "no BuildGrid resource -> no cell");

    // Tie: both 0.3 from the pick -> smaller Entity index first; the far
    // body stays last despite owning the smallest index overall.
    let tie = spawn_with_def(&mut app, &reg, "unit", Vec2::new(0.3, 0.0));
    send(&mut app, pick(Vec2::ZERO, PickButton::Left));
    let resolved = drain_picks(&mut app, &mut picks);
    assert_eq!(
        resolved[0].entities,
        vec![near, tie, far],
        "distance ascending, equal distance -> smaller Entity index first"
    );
}

/// ② body_radius boundary: a disc containing the pick exactly (d == r)
/// counts via the pinned epsilon (1e-3, same rule as box select); d == r +
/// 0.01 does not.
#[test]
fn pick_boundary_hits_at_radius_and_rejects_plus_001() {
    let mut app = test_app();
    let reg = registry(&[def("unit", 0.5)]);
    app.insert_resource(reg.clone());
    let exact = spawn_with_def(&mut app, &reg, "unit", Vec2::new(0.5, 0.0)); // d == r
    let just_out = spawn_with_def(&mut app, &reg, "unit", Vec2::new(0.51, 0.0)); // d == r + 0.01

    let mut picks = MessageCursor::<PickResolved>::default();
    send(&mut app, pick(Vec2::ZERO, PickButton::Left));
    let resolved = drain_picks(&mut app, &mut picks);
    assert_eq!(resolved[0].entities, vec![exact], "d == r in, d == r + 0.01 out");
    assert!(!resolved[0].entities.contains(&just_out));
}

/// resolve_picks reports the build-grid cell only when a FreeGrid covers
/// the pick point; gridless and FixedSlots apps resolve None (slots are
/// authored points, not a lattice).
#[test]
fn pick_reports_build_grid_cell_when_covered() {
    let mut app = test_app();
    let reg = registry(&[def("unit", 0.1)]);
    app.insert_resource(reg.clone());
    let _ = spawn_with_def(&mut app, &reg, "unit", Vec2::new(0.1, 0.1));

    let mut picks = MessageCursor::<PickResolved>::default();
    send(&mut app, pick(Vec2::new(1.5, 2.5), PickButton::Left));
    assert_eq!(drain_picks(&mut app, &mut picks)[0].cell, None, "gridless");

    app.insert_resource(BuildGrid::free_grid(1.0, 4, 4));
    send(&mut app, pick(Vec2::new(1.5, 2.5), PickButton::Left));
    assert_eq!(drain_picks(&mut app, &mut picks)[0].cell, Some((1, 2)), "covered");

    send(&mut app, pick(Vec2::new(-0.5, 2.5), PickButton::Left));
    assert_eq!(drain_picks(&mut app, &mut picks)[0].cell, None, "off the grid");

    app.insert_resource(BuildGrid::fixed_slots(vec![Vec2::new(1.5, 2.5)]));
    send(&mut app, pick(Vec2::new(1.5, 2.5), PickButton::Left));
    assert_eq!(drain_picks(&mut app, &mut picks)[0].cell, None, "fixed slots: no lattice");
}

/// ③ Smart right-click on a hostile body: exactly one UnitCommand{
/// AttackTarget }, subjects = the whole selection.
#[test]
fn right_click_hostile_issues_one_attack_command() {
    let mut app = test_app();
    let reg = registry(&[def("archer", 0.4)]);
    app.insert_resource(reg.clone());
    let squad = vec![
        spawn_with_def(&mut app, &reg, "archer", Vec2::new(-3.0, 0.0)),
        spawn_with_def(&mut app, &reg, "archer", Vec2::new(-3.0, 1.0)),
    ];
    send(&mut app, SelectCommand::Set(squad.clone()));

    // Attacker-side selectable body under the cursor (def-based spawn would
    // pin it Defender; spawn the marker set by hand). Ground (x, z) = (5, 0).
    let enemy = app
        .world_mut()
        .spawn((
            Selectable,
            Allegiance::Attacker,
            Transform::from_translation(Vec3::new(5.0, 0.0, 0.0)),
        ))
        .id();

    let mut commands = MessageCursor::<UnitCommand>::default();
    send(&mut app, pick(Vec2::new(5.0, 0.0), PickButton::Right));
    let issued = drain_commands(&mut app, &mut commands);
    assert_eq!(issued.len(), 1, "one right click = one command for the group");
    assert_eq!(issued[0].0, Order::AttackTarget { target: enemy });
    assert_eq!(issued[0].1, squad, "subjects carry the whole selection");
}

/// ④ Smart right-click on open ground: one Move command, to = the pick
/// point on the y=0 plane (ground (x, z) -> Vec3 (x, 0, z), hole-1); an
/// empty selection issues nothing.
#[test]
fn right_click_ground_issues_move_to_pick_point() {
    let mut app = test_app();
    let reg = registry(&[def("archer", 0.4)]);
    app.insert_resource(reg.clone());
    let unit = spawn_with_def(&mut app, &reg, "archer", Vec2::ZERO);
    send(&mut app, SelectCommand::Set(vec![unit]));

    let mut commands = MessageCursor::<UnitCommand>::default();
    send(&mut app, pick(Vec2::new(3.5, -2.0), PickButton::Right));
    assert_eq!(
        drain_commands(&mut app, &mut commands),
        vec![(Order::Move { to: Vec3::new(3.5, 0.0, -2.0) }, vec![unit])],
        "open ground -> move to the pick point"
    );

    send(&mut app, SelectCommand::Clear);
    send(&mut app, pick(Vec2::new(3.5, -2.0), PickButton::Right));
    assert!(
        drain_commands(&mut app, &mut commands).is_empty(),
        "empty selection -> no command"
    );
}

/// ⑤ H/S fire immediately for the current selection (no targeting round).
#[test]
fn hold_and_stop_hotkeys_fire_directly() {
    let mut app = test_app();
    let reg = registry(&[def("squad", 0.3)]);
    app.insert_resource(reg.clone());
    let units = vec![
        spawn_with_def(&mut app, &reg, "squad", Vec2::ZERO),
        spawn_with_def(&mut app, &reg, "squad", Vec2::ONE),
    ];
    send(&mut app, SelectCommand::Set(units.clone()));

    let mut commands = MessageCursor::<UnitCommand>::default();
    send(&mut app, CommandHotkey::Hold);
    send(&mut app, CommandHotkey::Stop);
    assert_eq!(
        drain_commands(&mut app, &mut commands),
        vec![(Order::HoldPosition, units.clone()), (Order::Stop, units)],
        "one command each, subjects = selection"
    );
}

/// ⑥ A arms AttackMove targeting: the next left click fires at that point
/// and returns the cursor to idle; Esc cancels — the click after fires
/// nothing and the selection survives Esc untouched.
#[test]
fn attack_move_targeting_and_esc_cancel() {
    let mut app = test_app();
    let reg = registry(&[def("squad", 0.3)]);
    app.insert_resource(reg.clone());
    let unit = spawn_with_def(&mut app, &reg, "squad", Vec2::ZERO);
    send(&mut app, SelectCommand::Set(vec![unit]));

    let pending = |app: &App| *app.world().resource::<PendingTargetOrder>();
    let mut commands = MessageCursor::<UnitCommand>::default();

    send(&mut app, CommandHotkey::AttackMove);
    assert_eq!(pending(&app), PendingTargetOrder::AttackMove, "A arms targeting");

    send(&mut app, pick(Vec2::new(2.0, 3.0), PickButton::Left));
    assert_eq!(
        drain_commands(&mut app, &mut commands),
        vec![(Order::AttackMove { to: Vec3::new(2.0, 0.0, 3.0) }, vec![unit])],
        "armed click targets the order"
    );
    assert_eq!(pending(&app), PendingTargetOrder::Idle, "cursor back to idle");

    send(&mut app, CommandHotkey::AttackMove);
    send(&mut app, CommandHotkey::Cancel);
    assert_eq!(pending(&app), PendingTargetOrder::Idle, "Esc disarms");
    assert_eq!(
        app.world().resource::<SelectionState>().entities,
        vec![unit],
        "Esc clears the pending state only, never the selection"
    );
    send(&mut app, pick(Vec2::new(2.0, 3.0), PickButton::Left));
    assert!(
        drain_commands(&mut app, &mut commands).is_empty(),
        "click after cancel fires nothing"
    );
    // The post-cancel click hit open ground far from the unit: point select
    // (hole-2) now clears the selection — applied one frame after the pick
    // (the select applier already ran when the pick resolved). The
    // targeting machine above is what stayed dead, which is the point.
    app.update();
    assert_eq!(
        app.world().resource::<SelectionState>().entities,
        Vec::<Entity>::new(),
        "ground click after cancel deselects (point select)"
    );
}

/// ⑦ Patrol is a two-click machine: the first click parks point a (no
/// command yet), the second fires Patrol{a, b}; Esc between clicks cancels
/// and the parked point dies with the pending state.
#[test]
fn patrol_needs_two_points_and_esc_cancels() {
    let mut app = test_app();
    let reg = registry(&[def("squad", 0.3)]);
    app.insert_resource(reg.clone());
    let unit = spawn_with_def(&mut app, &reg, "squad", Vec2::ZERO);
    send(&mut app, SelectCommand::Set(vec![unit]));

    let pending = |app: &App| *app.world().resource::<PendingTargetOrder>();
    let mut commands = MessageCursor::<UnitCommand>::default();

    send(&mut app, CommandHotkey::Patrol);
    assert_eq!(pending(&app), PendingTargetOrder::PatrolFirst, "P arms first point");

    send(&mut app, pick(Vec2::new(1.0, 1.0), PickButton::Left));
    assert!(
        drain_commands(&mut app, &mut commands).is_empty(),
        "one point is not a patrol yet"
    );
    assert_eq!(
        pending(&app),
        PendingTargetOrder::PatrolSecond { a: Vec3::new(1.0, 0.0, 1.0) },
        "first point parked"
    );

    send(&mut app, pick(Vec2::new(2.0, 0.0), PickButton::Left));
    assert_eq!(
        drain_commands(&mut app, &mut commands),
        vec![(
            Order::Patrol {
                a: Vec3::new(1.0, 0.0, 1.0),
                b: Vec3::new(2.0, 0.0, 0.0),
                toward_b: true,
            },
            vec![unit],
        )],
        "second point completes the patrol"
    );
    assert_eq!(pending(&app), PendingTargetOrder::Idle);

    // Esc between the two clicks: nothing fires afterwards.
    send(&mut app, CommandHotkey::Patrol);
    send(&mut app, pick(Vec2::new(9.0, 9.0), PickButton::Left));
    send(&mut app, CommandHotkey::Cancel);
    send(&mut app, pick(Vec2::new(8.0, 8.0), PickButton::Left));
    assert!(
        drain_commands(&mut app, &mut commands).is_empty(),
        "cancelled mid-machine fires nothing"
    );
    assert_eq!(pending(&app), PendingTargetOrder::Idle);
}
