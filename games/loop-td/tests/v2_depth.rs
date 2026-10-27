//! LT2 (round-2 main course): the six RPG-depth mechanisms on top of the
//! round-1 base + LT-ADOPT seams — equipment on hero towers, the artifact
//! quest aura, challenge waves, treasure digs, tower bonds, and the
//! multiplayer leak pass. Spec: docs/topics/game-design/loop-td-v2-rpg-proposal.md §2.
//! Everything here drives the REAL game composition (GamePlugin on the base
//! test app), same stance as tests/adopt_domain.rs.

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use loop_td::plugins::game::GamePlugin;
use loop_td::systems::gameplay::try_place;
use warbase::prelude::*;
use warbase::testing::test_app;

const LOOP_MAP: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/maps/loop.ron");

/// The real game headless (same recipe as adopt_domain): base test app +
/// GamePlugin; Startup loads the shipped map (grid, cards, item table).
fn game_app() -> App {
    let mut app = test_app();
    app.add_plugins(GamePlugin);
    app.insert_resource(MatchRng::seeded(42));
    app.update();
    app.update();
    app
}

/// Replace the raw keyboard resource (headless has no InputPlugin).
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

/// Place the hero tower (funded; hunter costs 150 > 120 start gold) and let
/// one update run so the attach kits (Selectable / Inventory / EquipSlots)
/// land on the fresh spawn.
fn place_hero(app: &mut App) -> Entity {
    app.world_mut().resource_mut::<Resources>().gold += 200;
    let cell = first_free_cell(app.world().resource::<BuildGrid>());
    let hero = try_place(&mut app.world_mut(), "hunter", cell).expect("place hunter");
    app.update(); // attach_td_kit + attach_equip_slots (Added<UnitInstance>)
    hero
}

// ---------------------------------------------------------------------------
// LT2-1: equipment on hero towers — 3 equip slots, affixes ride StatMods with
// a dedicated marker source per slot (dodges the INV same-source flaw).
// ---------------------------------------------------------------------------

#[test]
fn equipment_injects_replaces_and_falls_back() {
    use loop_td::systems::equip::{equip_item, unequip_slot, EquipSlots};

    let mut app = game_app();
    let hero = place_hero(&mut app);
    assert!(
        app.world().get::<EquipSlots>(hero).is_some(),
        "hero towers carry equip slots"
    );

    // 攻+12% blade -> damage_mult 1.12.
    equip_item(&mut app.world_mut(), hero, 0, "blade12").expect("equip blade12");
    app.update(); // aggregate_stats folds the entry
    let eff = app.world().get::<EffectiveStats>(hero).unwrap();
    assert!((eff.damage_mult - 1.12).abs() < 1e-5, "got {}", eff.damage_mult);

    // Exactly one DamageMult entry from the slot source (no stacking dupes).
    let mods = app.world().get::<StatMods>(hero).unwrap();
    let dmg_entries = mods
        .entries
        .iter()
        .filter(|e| e.kind == StatKind::DamageMult)
        .count();
    assert_eq!(dmg_entries, 1, "one entry per equipped item");

    // Unequip -> falls back to 1.0.
    unequip_slot(&mut app.world_mut(), hero, 0).expect("unequip");
    app.update();
    let eff = app.world().get::<EffectiveStats>(hero).unwrap();
    assert!((eff.damage_mult - 1.0).abs() < 1e-5, "got {}", eff.damage_mult);

    // Different item into the SAME slot -> new value, old gone (撤旧上新).
    equip_item(&mut app.world_mut(), hero, 0, "blade08").expect("equip blade08");
    app.update();
    let eff = app.world().get::<EffectiveStats>(hero).unwrap();
    assert!((eff.damage_mult - 1.08).abs() < 1e-5, "got {}", eff.damage_mult);

    // Same-slot replacement while occupied: blade12 over blade08 -> 1.12.
    equip_item(&mut app.world_mut(), hero, 0, "blade12").expect("replace in place");
    app.update();
    let eff = app.world().get::<EffectiveStats>(hero).unwrap();
    assert!((eff.damage_mult - 1.12).abs() < 1e-5, "got {}", eff.damage_mult);
    // The replaced blade08 went back into the hero's bag (Inventory).
    let inv = app.world().get::<Inventory>(hero).unwrap();
    let bag: Vec<&str> = inv
        .slots
        .iter()
        .filter_map(|s| s.as_ref().map(|st| st.def.as_str()))
        .collect();
    assert!(bag.contains(&"blade08"), "replaced item stowed back: {bag:?}");
}

#[test]
fn equipment_range_affix_and_guards() {
    use loop_td::systems::equip::{equip_item, unequip_slot, EquipSlots, EQUIP_SLOTS};

    let mut app = game_app();
    let hero = place_hero(&mut app);

    // Hunter base range 5.0 (maps/loop.ron); scope adds +1.5 flat.
    equip_item(&mut app.world_mut(), hero, 1, "scope").expect("equip scope");
    app.update();
    let kit = app.world().get::<AttackKit>(hero).unwrap();
    assert!((kit.def.range - 6.5).abs() < 1e-5, "got {}", kit.def.range);
    // Interval affix rides StatMods (quiver -10% attack interval).
    equip_item(&mut app.world_mut(), hero, 2, "quiver").expect("equip quiver");
    app.update();
    let eff = app.world().get::<EffectiveStats>(hero).unwrap();
    assert!((eff.interval_mult - 0.90).abs() < 1e-5, "got {}", eff.interval_mult);

    unequip_slot(&mut app.world_mut(), hero, 1).expect("unequip scope");
    app.update();
    let kit = app.world().get::<AttackKit>(hero).unwrap();
    assert!((kit.def.range - 5.0).abs() < 1e-5, "range restored");

    // Guards: slot index, unknown item, non-equipment rejects, plain towers.
    assert!(equip_item(&mut app.world_mut(), hero, EQUIP_SLOTS, "scope").is_err());
    assert!(equip_item(&mut app.world_mut(), hero, 0, "no-such-item").is_err());
    let cell = first_free_cell(app.world().resource::<BuildGrid>());
    let arrow = try_place(&mut app.world_mut(), "arrow", cell).unwrap();
    app.update();
    assert!(app.world().get::<EquipSlots>(arrow).is_none(), "plain towers carry none");
    assert!(equip_item(&mut app.world_mut(), arrow, 0, "blade12").is_err());
}
