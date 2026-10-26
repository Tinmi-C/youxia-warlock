//! SEL domain acceptance (WC3 alignment plan §4, SEL2/SEL3): selection set,
//! control groups, Tab subgroups, box select, marker sync, panel data.
//! All headless — commands/messages are injected directly; the ui-feature
//! input bridge no-ops under MinimalPlugins (no raw input resources).

use bevy::prelude::*;
use warbase::prelude::*;
use warbase::testing::{spawn_selectable, test_app};

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

/// Spawn a Selectable unit from a registry def (UnitInstance + Hp/MaxHp).
/// `pos` is ground (x, z) — hole-1 plane convention: world Vec3 (x, 0, z).
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

fn send(app: &mut App, msg: impl Message) {
    app.world_mut().write_message(msg);
    app.update();
}

fn selection(app: &App) -> Vec<Entity> {
    app.world().resource::<SelectionState>().entities.clone()
}

fn selection_len(app: &App) -> usize {
    app.world().resource::<SelectionState>().entities.len()
}

fn spawn_n(app: &mut App, n: usize) -> Vec<Entity> {
    (0..n)
        .map(|i| spawn_selectable(app.world_mut(), Vec2::new(i as f32, 0.0), Allegiance::Defender))
        .collect()
}

// -- acceptance assertions (plan §4) --------------------------------------

/// ① Set dedups and sorts: 5 distinct entities in scrambled order with a
/// duplicate -> len 5, index-ascending; duplicates never grow the set.
#[test]
fn set_dedups_and_sorts_by_entity_index() {
    let mut app = test_app();
    let units = spawn_n(&mut app, 5);
    let [a, b, c, d, e] = [units[0], units[1], units[2], units[3], units[4]];

    send(&mut app, SelectCommand::Set(vec![c, a, a, e, b, d]));
    let selected = selection(&app);
    assert_eq!(selected.len(), 5, "duplicate does not grow the set");
    assert!(
        selected.windows(2).all(|w| w[0].index() < w[1].index()),
        "strictly ascending by Entity index: {selected:?}"
    );

    // Add/Toggle duplicates stay inert.
    send(&mut app, SelectCommand::Add(a));
    assert_eq!(selection_len(&app), 5);
    send(&mut app, SelectCommand::Toggle(b));
    assert_eq!(selection_len(&app), 4, "toggle removes a member");
    send(&mut app, SelectCommand::Toggle(b));
    assert_eq!(selection_len(&app), 5, "toggle re-adds it");
}

/// ② Control groups: Bind 5 -> Clear -> Recall restores 5; after one unit
/// despawns, Recall yields 4 and the group holds no dead reference.
#[test]
fn control_groups_remember_and_drop_the_dead() {
    let mut app = test_app();
    let units = spawn_n(&mut app, 5);
    send(&mut app, SelectCommand::Set(units.clone()));
    send(&mut app, GroupCommand::Bind(1));

    send(&mut app, SelectCommand::Clear);
    assert_eq!(selection_len(&app), 0);

    send(&mut app, GroupCommand::Recall(1));
    assert_eq!(selection_len(&app), 5, "recall restores the bound five");

    let dead = units[2];
    app.world_mut().entity_mut(dead).despawn();
    app.update(); // Cleanup sweep clears groups of despawned members.

    send(&mut app, GroupCommand::Recall(1));
    assert_eq!(selection_len(&app), 4, "dead member no longer recalled");
    let groups = app.world().resource::<ControlGroups>();
    assert!(
        !groups.groups[0].contains(&dead),
        "group storage holds no dead reference"
    );
    assert!(groups.groups[0].len() == 4);
}

/// Insert (Shift+digit) unions the current selection into the slot, deduped.
#[test]
fn group_insert_unions_deduped() {
    let mut app = test_app();
    let units = spawn_n(&mut app, 5);
    send(&mut app, SelectCommand::Set(units[..2].to_vec()));
    send(&mut app, GroupCommand::Bind(1));
    send(&mut app, SelectCommand::Set(units[1..4].to_vec()));
    send(&mut app, GroupCommand::Insert(1));
    let group = &app.world().resource::<ControlGroups>().groups[0];
    assert_eq!(group.len(), 4, "union deduped");
    assert!(group.windows(2).all(|w| w[0].index() < w[1].index()));
}

/// ③ Tab subgroups: 3 def-A + 2 def-B -> bucket sequence [A, B]; the cursor
/// walks the ring and is back at the start after 4 steps.
#[test]
fn tab_cycles_def_subgroups_deterministically() {
    let mut app = test_app();
    let reg = registry(&[def("archer", 0.4), def("tower", 0.5)]);
    app.insert_resource(reg.clone());
    let mut units = vec![];
    for _ in 0..3 {
        units.push(spawn_with_def(&mut app, &reg, "archer", Vec2::ZERO));
    }
    for _ in 0..2 {
        units.push(spawn_with_def(&mut app, &reg, "tower", Vec2::ZERO));
    }
    send(&mut app, SelectCommand::Set(units));

    let world = app.world();
    let state = world.resource::<SelectionState>();
    assert_eq!(
        selection_subgroups(world, state),
        vec!["archer".to_string(), "tower".to_string()],
        "buckets sorted by def id"
    );
    assert_eq!(state.subgroup_cursor, 0, "fresh selection resets the cursor");

    let cursor = |app: &App| app.world().resource::<SelectionState>().subgroup_cursor;
    send(&mut app, CycleSubgroups);
    assert_eq!(cursor(&app), 1);
    send(&mut app, CycleSubgroups);
    assert_eq!(cursor(&app), 0, "2 buckets: ring closes after 2");
    send(&mut app, CycleSubgroups);
    assert_eq!(cursor(&app), 1);
    send(&mut app, CycleSubgroups);
    assert_eq!(cursor(&app), 0, "4 steps return to the starting bucket");
}

/// ④ Box select: exactly 3 of 5 bodies enter — the boundary disc at
/// distance == radius counts (edge epsilon 1e-3), radius + 0.01 does not.
#[test]
fn aabb_selects_bodies_touching_the_rect() {
    let mut app = test_app();
    let reg = registry(&[def("squad", 0.5)]);
    app.insert_resource(reg.clone());
    // Rect under test: x in [3, 7], y in [-1, 1].
    let inside_a = spawn_with_def(&mut app, &reg, "squad", Vec2::new(4.0, 0.0));
    let inside_b = spawn_with_def(&mut app, &reg, "squad", Vec2::new(6.0, 0.0));
    let edge = spawn_with_def(&mut app, &reg, "squad", Vec2::new(7.5, 0.0)); // d = r
    let just_out = spawn_with_def(&mut app, &reg, "squad", Vec2::new(7.51, 0.0)); // d = r + 0.01
    let far = spawn_with_def(&mut app, &reg, "squad", Vec2::new(0.0, 0.0));

    send(
        &mut app,
        SelectCommand::Aabb { min: Vec2::new(3.0, -1.0), max: Vec2::new(7.0, 1.0) },
    );
    let selected = selection(&app);
    assert_eq!(selected.len(), 3, "exactly 3 of 5 bodies enter");
    assert!(selected.contains(&inside_a) && selected.contains(&inside_b));
    assert!(
        selected.contains(&edge),
        "disc touching the rect edge exactly (d == r) counts via epsilon 1e-3"
    );
    assert!(!selected.contains(&just_out), "d == r + 0.01 is out");
    assert!(!selected.contains(&far));
}

/// ⑤ The `Selected` marker tracks SelectionState strictly: entering entities
/// gain it, leaving entities lose it, Clear strips everyone.
#[test]
fn selected_marker_stays_in_sync_with_state() {
    let mut app = test_app();
    let units = spawn_n(&mut app, 3);
    let [a, _b, c] = [units[0], units[1], units[2]];

    send(&mut app, SelectCommand::Set(units.clone()));
    for entity in &units {
        assert!(
            app.world().get::<Selected>(*entity).is_some(),
            "member carries the marker"
        );
    }

    send(&mut app, SelectCommand::Set(vec![a, c]));
    assert!(app.world().get::<Selected>(a).is_some());
    assert!(app.world().get::<Selected>(c).is_some());
    assert!(
        app.world().get::<Selected>(units[1]).is_none(),
        "leaving entity loses the marker"
    );

    send(&mut app, SelectCommand::Add(units[1]));
    assert!(app.world().get::<Selected>(units[1]).is_some(), "re-entry regains it");

    send(&mut app, SelectCommand::Clear);
    for entity in &units {
        assert!(app.world().get::<Selected>(*entity).is_none(), "clear strips all");
    }
    // Marker set equals state set, both directions.
    let marked: Vec<Entity> = app
        .world_mut()
        .query_filtered::<Entity, With<Selected>>()
        .iter(app.world())
        .collect();
    assert!(marked.is_empty() && selection_len(&app) == 0);
}

/// SEL3 data layer: panel names follow the selection; a single def-driven
/// selection gets hp + skill placeholders, multi-selection gets no detail.
#[test]
fn panel_data_projects_names_and_single_detail() {
    let mut app = test_app();
    let reg = registry(&[def("archer", 0.4)]);
    app.insert_resource(reg.clone());
    let a = spawn_with_def(&mut app, &reg, "archer", Vec2::ZERO);
    let b = spawn_with_def(&mut app, &reg, "archer", Vec2::ONE);

    send(&mut app, SelectCommand::Set(vec![a]));
    let panel = app.world().resource::<SelectionPanelData>();
    assert_eq!(panel.names, vec!["archer".to_string()]);
    let detail = panel.detail.as_ref().expect("single selection has detail");
    assert_eq!(detail.hp, Some((80.0, 80.0)), "spawned at full hp");
    assert_eq!(detail.skills, vec!["bolt".to_string()]);
    assert_eq!(detail.mana, None);

    send(&mut app, SelectCommand::Set(vec![a, b]));
    let panel = app.world().resource::<SelectionPanelData>();
    assert_eq!(panel.names.len(), 2);
    assert!(panel.detail.is_none(), "multi-selection has no detail readout");
}
