//! LT-ADOPT (round-2 plan §4 line 2): loop-td adopts the round-1 base seams
//! it had not consumed yet — mouse selection chain (PickAttempt ->
//! SelectionState/SelectionPanelData, box select, right-click smart command)
//! with the keyboard build cursor kept as a parallel mode, the digit-row
//! modal yield (ModalKeyClaim), and the hero-tower Inventory. Everything
//! here drives the REAL game composition (GamePlugin on the base test app):
//! headless tests inject selection messages / raw ButtonInput exactly like
//! the base input bridges would.

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use loop_td::plugins::game::GamePlugin;
use loop_td::systems::adopt::BUILD_MENU_CLAIM;
use loop_td::systems::gameplay::try_place;
use warbase::prelude::*;
use warbase::testing::test_app;
use warbase::ui::{CommandAction, CommandCardAction};

/// The real game headless: base test app + GamePlugin (Startup loads the
/// shipped map: grid, build cursor, build card, Build-mode selection). Two
/// settle frames so the PreUpdate modal-claim sync has run after Startup.
fn game_app() -> App {
    let mut app = test_app();
    app.add_plugins(GamePlugin);
    app.update();
    app.update();
    app
}

/// Replace the raw keyboard resource (headless has no InputPlugin to reset
/// `just_pressed`, so each key phase installs a fresh button set).
fn press_keys(app: &mut App, codes: &[KeyCode]) {
    let mut keys = ButtonInput::<KeyCode>::default();
    for code in codes {
        keys.press(*code);
    }
    app.insert_resource(keys);
}

/// First free grid cell (row-major).
fn first_free_cell(grid: &BuildGrid) -> Cell {
    (0..grid.cols)
        .flat_map(|x| (0..grid.rows).map(move |y| Cell(x, y)))
        .find(|c| grid.check(*c).is_ok())
        .expect("a free cell")
}

/// Three horizontally adjacent free cells (box-select fixture).
fn free_row_of_three(grid: &BuildGrid) -> [Cell; 3] {
    (0..grid.cols - 2)
        .flat_map(|x| (0..grid.rows).map(move |y| Cell(x, y)))
        .find(|c| (0..3).all(|i| grid.check(Cell(c.0 + i, c.1)).is_ok()))
        .map(|c| [c, Cell(c.0 + 1, c.1), Cell(c.0 + 2, c.1)])
        .expect("three adjacent free cells")
}

// ---------------------------------------------------------------------------
// LT-ADOPT 1a: digit-row modal yield — open build menu claims the digits,
// the base keyboard bridge stays silent, the menu still receives the key.
// ---------------------------------------------------------------------------

#[test]
fn modal_digits_yield_to_the_build_menu() {
    let mut app = game_app();
    assert_eq!(
        app.world().resource::<ModalKeyClaim>().0,
        Some(BUILD_MENU_CLAIM),
        "the open build menu claims the digit row"
    );

    press_keys(&mut app, &[KeyCode::Digit1]);
    let mut groups = MessageCursor::<GroupCommand>::default();
    let mut actions = MessageCursor::<CommandCardAction>::default();
    app.update();

    let group_count = {
        let msgs = app.world().resource::<Messages<GroupCommand>>();
        groups.read(msgs).count()
    };
    assert_eq!(
        group_count, 0,
        "modal period: the base keyboard bridge emits no GroupCommand"
    );

    let fired: Vec<CommandCardAction> = {
        let msgs = app.world().resource::<Messages<CommandCardAction>>();
        actions.read(msgs).cloned().collect()
    };
    assert_eq!(fired.len(), 1, "the build menu still receives the digit");
    assert!(
        matches!(&fired[0].action, CommandAction::Build(id) if id == "arrow"),
        "Digit1 is the first sorted buildable (arrow), got {:?}",
        fired[0].action
    );

    // The action really places a tower through the game dispatch (one extra
    // frame in case dispatch ran before command_input in the same update).
    press_keys(&mut app, &[]); // clear just_pressed (no InputPlugin headless)
    app.update();
    assert_eq!(app.world().resource::<BuildGrid>().used(), 1, "card action placed a tower");
}

// ---------------------------------------------------------------------------
// LT-ADOPT 1b: closing the menu releases the claim — plain digits bind and
// recall control groups again through the base bridge (B key is the path).
// ---------------------------------------------------------------------------

#[test]
fn closing_the_build_menu_restores_group_recall() {
    let mut app = game_app();

    // B toggles the shipped selection mode Build -> None (build_input).
    press_keys(&mut app, &[KeyCode::KeyB]);
    app.update();
    press_keys(&mut app, &[]); // clear; next PreUpdate releases the claim
    app.update();
    assert_eq!(
        app.world().resource::<ModalKeyClaim>().0,
        None,
        "claim released on close"
    );

    // Select a tower and bind group 1 through the base bridge (Ctrl+1).
    let cell = first_free_cell(app.world().resource::<BuildGrid>());
    let tower = try_place(&mut app.world_mut(), "arrow", cell).expect("place arrow");
    app.update(); // attach_td_kit lands Selectable
    app.world_mut().write_message(SelectCommand::Set(vec![tower]));
    app.update();
    press_keys(&mut app, &[KeyCode::ControlLeft, KeyCode::Digit1]);
    app.update();
    assert_eq!(
        app.world().resource::<ControlGroups>().groups[0],
        vec![tower],
        "Ctrl+1 binds the group once the modal released the digits"
    );
    press_keys(&mut app, &[]);
    app.update();

    // Clear the selection, then recall via plain 1 through the bridge.
    app.world_mut().write_message(SelectCommand::Clear);
    app.update();
    assert!(app.world().resource::<SelectionState>().entities.is_empty());
    press_keys(&mut app, &[KeyCode::Digit1]);
    app.update();
    assert_eq!(
        app.world().resource::<SelectionState>().entities,
        vec![tower],
        "plain 1 recalls the group after the menu closed"
    );
}

// ---------------------------------------------------------------------------
// LT-ADOPT 2a: mouse chain — a left-click PickAttempt on a tower selects it
// and refreshes the base panel data (keyboard cursor stays parallel).
// ---------------------------------------------------------------------------

#[test]
fn left_click_selects_a_tower_and_fills_the_panel() {
    let mut app = game_app();
    let grid = app.world().resource::<BuildGrid>().clone();
    let cell = first_free_cell(&grid);
    let tower = try_place(&mut app.world_mut(), "arrow", cell).expect("place arrow");
    app.update(); // attach_td_kit -> Selectable
    assert!(app.world().get::<Selectable>(tower).is_some(), "tower joined the pick chain");

    app.world_mut().write_message(PickAttempt {
        world: grid.cell_center(cell),
        button: PickButton::Left,
        click_count: 1,
        ctrl: false,
        shift: false,
    });
    app.update(); // resolve_picks -> apply_point_selects emits Set ...
    app.update(); // ... consumed here + fill_panel_data

    assert_eq!(
        app.world().resource::<SelectionState>().entities,
        vec![tower],
        "left click selected the tower"
    );
    assert!(app.world().get::<Selected>(tower).is_some(), "Selected marker synced");
    let panel = app.world().resource::<SelectionPanelData>();
    assert_eq!(panel.names, vec!["arrow".to_string()], "panel data updated");
    let detail = panel.detail.as_ref().expect("single-selection detail");
    assert_eq!(detail.hp, Some((80.0, 80.0)), "arrow hp readout on the panel");
}

// ---------------------------------------------------------------------------
// LT-ADOPT 2b: box select — one Aabb SelectCommand covering three towers
// selects exactly them (the drag path the pointer bridge emits).
// ---------------------------------------------------------------------------

#[test]
fn box_select_grabs_every_tower_in_the_rect() {
    let mut app = game_app();
    app.world_mut().resource_mut::<Resources>().gold += 100; // 3 x 50g > 120 start
    let grid = app.world().resource::<BuildGrid>().clone();
    let cells = free_row_of_three(&grid);
    for cell in cells {
        try_place(&mut app.world_mut(), "arrow", cell).expect("place arrow");
    }
    app.update(); // attach_td_kit

    let a = grid.cell_center(cells[0]);
    let b = grid.cell_center(cells[2]);
    app.world_mut().write_message(SelectCommand::Aabb {
        min: Vec2::new(a.x - 0.6, a.y - 0.6),
        max: Vec2::new(b.x + 0.6, b.y + 0.6),
    });
    app.update();

    assert_eq!(
        app.world().resource::<SelectionState>().entities.len(),
        3,
        "box select grabbed all three towers"
    );
}

// ---------------------------------------------------------------------------
// LT-ADOPT 3: right-click chain — PickResolved -> smart_resolve produces
// exactly one UnitCommand (open ground -> Move); TD has no consumer for
// tower moves, and the frames after keep running (harmless by construction).
// ---------------------------------------------------------------------------

#[test]
fn right_click_resolves_to_a_unit_command_without_consumers() {
    let mut app = game_app();
    let grid = app.world().resource::<BuildGrid>().clone();
    let cell = first_free_cell(&grid);
    let tower = try_place(&mut app.world_mut(), "arrow", cell).expect("place arrow");
    app.update(); // Selectable
    app.world_mut().write_message(SelectCommand::Set(vec![tower]));
    app.update();

    // Right-click open ground far from any tower disc.
    let ground = grid.cell_center(Cell(2, 2));
    app.world_mut().write_message(PickAttempt {
        world: ground,
        button: PickButton::Right,
        click_count: 1,
        ctrl: false,
        shift: false,
    });
    let mut cursor = MessageCursor::<UnitCommand>::default();
    app.update();

    let msgs = app.world().resource::<Messages<UnitCommand>>();
    let commands: Vec<&UnitCommand> = cursor.read(msgs).collect();
    assert_eq!(commands.len(), 1, "exactly one smart command per right click");
    assert_eq!(commands[0].subjects, vec![tower], "command subjects = the selection");
    match &commands[0].order {
        Order::Move { to } => {
            assert_eq!(*to, Vec3::new(ground.x, 0.0, ground.y), "Move onto the y=0 plane");
        }
        other => panic!("open ground right click should Move, got {other:?}"),
    }

    // No consumer panics: towers have no MoveAgent, orders drop silently.
    for _ in 0..3 {
        app.update();
    }
}

// ---------------------------------------------------------------------------
// LT-ADOPT 4: hero towers (hero: true defs) carry the base 6-slot Inventory
// (empty) — the LT2 equipment-on-towers runway; plain towers carry none.
// ---------------------------------------------------------------------------

#[test]
fn hero_towers_carry_an_empty_six_slot_inventory() {
    let mut app = game_app();
    // Hunter costs 150 > the map's 120 start gold; fund the test placement.
    app.world_mut().resource_mut::<Resources>().gold += 100;

    let grid = app.world().resource::<BuildGrid>().clone();
    let hero_cell = first_free_cell(&grid);
    let hunter = try_place(&mut app.world_mut(), "hunter", hero_cell).expect("place hunter");
    // Next free cell AFTER the hunter's placement is committed to the grid.
    let plain_cell = first_free_cell(app.world().resource::<BuildGrid>());
    let arrow = try_place(&mut app.world_mut(), "arrow", plain_cell).expect("place arrow");
    app.update(); // attach_td_kit

    assert!(
        app.world().get::<XpHolder>(hunter).is_some(),
        "hunter is the hero def (def.hero marker)"
    );
    let inv = app
        .world()
        .get::<Inventory>(hunter)
        .expect("hero tower carries an inventory");
    assert_eq!(inv.slots.len(), INVENTORY_SLOTS, "six slots");
    assert!(inv.slots.iter().all(|s| s.is_none()), "all slots start empty");
    assert!(
        app.world().get::<Inventory>(arrow).is_none(),
        "plain towers carry no inventory"
    );
}

// ---------------------------------------------------------------------------
// LT-ADOPT guard: creeps (Attacker side) never join the selection chain —
// picks and box selects stay tower-only in TD.
// ---------------------------------------------------------------------------

#[test]
fn creeps_never_become_selectable() {
    let mut app = game_app();
    app.world_mut().write_message(StartWave);
    for _ in 0..10 {
        app.update();
    }
    let world = app.world_mut();

    let mut attackers = 0;
    {
        let mut q = world.query::<&Allegiance>();
        for side in q.iter(world) {
            if *side == Allegiance::Attacker {
                attackers += 1;
            }
        }
    }
    assert!(attackers >= 1, "wave 1 spawned creeps");

    let mut selectable_creeps = 0;
    {
        let mut q = world
            .query_filtered::<&Allegiance, (With<UnitInstance>, With<Selectable>)>();
        for side in q.iter(world) {
            if *side == Allegiance::Attacker {
                selectable_creeps += 1;
            }
        }
    }
    assert_eq!(selectable_creeps, 0, "creeps stay out of the pick chain");
}
