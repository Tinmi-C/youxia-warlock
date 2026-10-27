//! INV1-3 acceptance (ADR-0016 §8): pickups into the first free slot with
//! readable full-inventory rejection, use-effects over the existing
//! SkillCast channel with charge/cooldown gating, carried passives via
//! StatMods (upsert + revoke), symmetric shop gold flows, seeded loot
//! drops, and zero drift for item-less maps.

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use std::collections::HashMap;
use warbase::bundle::{assemble, BuildDomainDef, MapBundle, MapIdentity, WorldDef, MAP_FORMAT_VERSION};
use warbase::prelude::*;
use warbase::testing::test_app;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn item_def(
    id: &str,
    kind: ItemKind,
    charges: Option<u8>,
    cooldown: f32,
    effect: Option<&str>,
    passive: Option<PassiveStat>,
    cost_gold: u32,
) -> ItemDef {
    ItemDef {
        id: id.into(),
        name: format!("{id} name"),
        kind,
        charges,
        cooldown,
        effect: effect.map(str::to_string),
        cost_gold,
        passive,
    }
}

fn carrier_unit_def() -> UnitDef {
    UnitDef {
        hero_stats: None,
        leak: 1,
        id: "carrier".into(),
        attrs: BaseAttributes { hp: 100.0, move_speed: 0.0, body_radius: 0.3 },
        attack: None,
        armor_type: ArmorType::Light,
        cost_gold: 0,
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
    skill_levels: vec![],
    }
}

fn carrier_app(items: Vec<ItemDef>) -> (App, Entity) {
    let mut app = test_app();
    let mut defs = HashMap::new();
    for d in items {
        defs.insert(d.id.clone(), d);
    }
    app.insert_resource(ItemRegistry { defs });
    let units = UnitRegistry {
        defs: [("carrier".to_string(), carrier_unit_def())].into_iter().collect(),
    };
    let carrier =
        spawn_unit(&mut app.world_mut(), &units, "carrier", Vec3::ZERO, Allegiance::Defender, 1.0)
            .unwrap();
    app.insert_resource(units);
    app.world_mut().entity_mut(carrier).insert(Inventory::default());
    let mut res = Resources::default();
    res.gold = 1000;
    app.insert_resource(res);
    app.update(); // warm-up frame (first-frame dt=0 quirk, see spl_domain)
    (app, carrier)
}

fn with_skill(app: &mut App, id: &str) {
    let mut defs = SkillDefs::default();
    defs.defs.insert(
        id.to_string(),
        SkillDef {
            id: id.into(),
            kind: SkillKind::Active,
            targeting: SkillTargeting::None,
            max_level: 1,
            cooldown: 0.0,
            mana_cost: 0.0,
            cast_time: 0.0,
            channel: None,
            area_radius: 0.0, summon: None,
            badge: None,
            hotkey: None,
        },
    );
    app.insert_resource(defs);
}

fn ground(app: &mut App, def: &str, pos: Vec3) -> Entity {
    app.world_mut()
        .spawn((GroundItem { def: def.into(), grace: 0.0 }, Transform::from_translation(pos)))
        .id()
}

fn drain_events(app: &mut App) -> Vec<ItemReject> {
    std::mem::take(&mut app.world_mut().resource_mut::<ItemEvents>().0)
}

fn slot_stack(app: &App, holder: Entity, slot: usize) -> Option<ItemStack> {
    app.world().get::<Inventory>(holder).unwrap().slots[slot].clone()
}

fn stow_directly(app: &mut App, holder: Entity, slot: usize, def_id: &str) {
    let def = app.world().resource::<ItemRegistry>().get(def_id).cloned().unwrap();
    app.world_mut()
        .get_mut::<Inventory>(holder)
        .unwrap()
        .slots[slot] = Some(ItemStack::from_def(&def));
}

fn push_cmd(app: &mut App, cmd: ItemCommand) {
    app.world_mut().write_message(ItemCommands(cmd));
}

fn push_shop(app: &mut App, cmd: ShopCommand) {
    app.world_mut().write_message(ShopCommands(cmd));
}

/// Persistent-cursor collection over `frames` updates (the double-buffer
/// pitfall: a single end-of-run read loses all but the last frame).
fn casts_over_frames(app: &mut App, frames: usize) -> Vec<SkillCast> {
    let mut cursor = MessageCursor::<SkillCast>::default();
    let mut out = Vec::new();
    for _ in 0..frames {
        app.update();
        let msgs = app.world().resource::<Messages<SkillCast>>();
        out.extend(cursor.read(msgs).cloned());
    }
    out
}

fn ground_items(app: &mut App) -> Vec<(Entity, Vec3)> {
    let mut q = app.world_mut().query::<(Entity, &GroundItem, &Transform)>();
    q.iter(app.world()).map(|(e, _, t)| (e, t.translation)).collect()
}

fn gold(app: &App) -> u32 {
    app.world().resource::<Resources>().gold
}

fn damage_mult_entries(app: &App, holder: Entity) -> Vec<f32> {
    app.world()
        .get::<StatMods>(holder)
        .unwrap()
        .entries
        .iter()
        .filter(|e| e.kind == StatKind::DamageMult)
        .map(|e| e.amount)
        .collect()
}

// ---------------------------------------------------------------------------
// INV1: pickup
// ---------------------------------------------------------------------------

/// UT-INV-01: a ground item within body_radius + 0.2 (0.3 + 0.2 = 0.5)
/// auto-stows into the FIRST FREE slot and despawns; an out-of-reach item
/// stays put with no rejects.
#[test]
fn pickup_stows_into_first_free_slot_within_reach() {
    let (mut app, hero) = carrier_app(vec![
        item_def("potion", ItemKind::Consumable, Some(3), 0.0, None, None, 50),
        item_def("sword", ItemKind::Passive, None, 0.0, None, None, 135),
    ]);
    let near = ground(&mut app, "potion", Vec3::new(0.45, 0.0, 0.0));
    let far = ground(&mut app, "sword", Vec3::new(0.8, 0.0, 0.0));
    app.update();

    let s = slot_stack(&app, hero, 0).expect("picked into slot 0");
    assert_eq!(s.def, "potion");
    assert_eq!(s.charges, 3);
    assert!(slot_stack(&app, hero, 1).is_none(), "only one item picked");
    assert!(app.world().get::<GroundItem>(near).is_none(), "stowed item despawned");
    assert!(app.world().get::<GroundItem>(far).is_some(), "out-of-reach item stays");
    assert!(drain_events(&mut app).is_empty(), "no rejects");

    // Slot 0 occupied: the next pickup lands in the first FREE slot.
    let near2 = ground(&mut app, "sword", Vec3::new(0.0, 0.0, 0.4));
    app.update();
    let s = slot_stack(&app, hero, 1).expect("picked into first free slot");
    assert_eq!(s.def, "sword");
    assert!(app.world().get::<GroundItem>(near2).is_none());
}

/// UT-INV-02: all 6 slots full -> exactly one InventoryFull reject, the
/// item stays on the ground.
#[test]
fn full_inventory_rejects_and_leaves_item_on_ground() {
    let (mut app, hero) =
        carrier_app(vec![item_def("potion", ItemKind::Consumable, Some(3), 0.0, None, None, 50)]);
    {
        let mut inv = app.world_mut().get_mut::<Inventory>(hero).unwrap();
        for slot in inv.slots.iter_mut() {
            *slot = Some(ItemStack { def: "potion".into(), charges: 1, cooldown: 0.0 });
        }
    }
    let item = ground(&mut app, "potion", Vec3::new(0.4, 0.0, 0.0));
    app.update();

    assert_eq!(drain_events(&mut app), vec![ItemReject::InventoryFull]);
    assert!(app.world().get::<GroundItem>(item).is_some(), "item stays on the ground");
    // The reject repeats once per frame while the carrier stands on it;
    // nothing else (no charges/no item) leaks into the stream.
    app.update();
    assert_eq!(drain_events(&mut app), vec![ItemReject::InventoryFull]);
}

// ---------------------------------------------------------------------------
// INV2: use / cooldown / passives
// ---------------------------------------------------------------------------

/// UT-INV-03: using a potion whose effect points at a registered skill
/// fires exactly one SkillCast (effect id, level 1) and spends one charge;
/// the 1.0s use cooldown gates an immediate reuse and the final charge
/// empties the slot.
#[test]
fn potion_use_fires_one_cast_spends_charge_and_gates_on_cooldown() {
    let (mut app, hero) = carrier_app(vec![item_def(
        "potion",
        ItemKind::Consumable,
        Some(3),
        1.0,
        Some("heal"),
        None,
        50,
    )]);
    with_skill(&mut app, "heal");
    stow_directly(&mut app, hero, 0, "potion");

    push_cmd(&mut app, ItemCommand::Use { user: hero, slot: 0 });
    let casts = casts_over_frames(&mut app, 3);
    assert_eq!(casts.len(), 1, "exactly one SkillCast");
    assert_eq!(casts[0].skill, "heal");
    assert_eq!(casts[0].caster, hero);
    assert_eq!(casts[0].level, 1, "items cast at level 1 (levelless skills)");
    let s = slot_stack(&app, hero, 0).unwrap();
    assert_eq!(s.charges, 2, "one charge spent");
    // Armed at 1.0 on the use frame, then decremented once per following
    // frame by the Tick timer (the use fired on casts_over_frames' first
    // update; this read happens after three).
    let expected = 1.0 - 2.0 / 60.0;
    assert!(
        (s.cooldown - expected).abs() < 1e-4,
        "use cooldown armed and ticking: {}",
        s.cooldown
    );

    // Inside the 1.0s cooldown window a second use fires nothing.
    push_cmd(&mut app, ItemCommand::Use { user: hero, slot: 0 });
    assert!(casts_over_frames(&mut app, 2).is_empty(), "cooldown blocks reuse");
    assert_eq!(slot_stack(&app, hero, 0).unwrap().charges, 2, "no charge spent");

    // 1.0s later the stack is usable again.
    for _ in 0..60 {
        app.update();
    }
    push_cmd(&mut app, ItemCommand::Use { user: hero, slot: 0 });
    assert_eq!(casts_over_frames(&mut app, 2).len(), 1, "usable after cooldown");
    assert_eq!(slot_stack(&app, hero, 0).unwrap().charges, 1);

    // The final charge empties the slot entirely.
    for _ in 0..60 {
        app.update();
    }
    push_cmd(&mut app, ItemCommand::Use { user: hero, slot: 0 });
    assert_eq!(casts_over_frames(&mut app, 2).len(), 1);
    assert!(slot_stack(&app, hero, 0).is_none(), "exhausted stack leaves the slot");
}

/// UT-INV-04: a passive sword (+0.12 DamageMult) picked up injects exactly
/// one holder-sourced StatMods entry and folds into EffectiveStats (1.12);
/// dropping revokes the entry, and the grace window keeps the dropped
/// sword from returning until it expires (then the standing carrier
/// re-stows it and the upsert stays a single entry).
#[test]
fn passive_sword_grants_folds_and_revokes() {
    let sword = item_def(
        "sword",
        ItemKind::Passive,
        None,
        0.0,
        None,
        Some(PassiveStat { kind: StatKind::DamageMult, amount: 0.12 }),
        135,
    );
    let (mut app, hero) = carrier_app(vec![sword]);
    ground(&mut app, "sword", Vec3::new(0.4, 0.0, 0.0));
    app.update();

    assert!(slot_stack(&app, hero, 0).is_some(), "sword stowed");
    let entries = damage_mult_entries(&app, hero);
    assert_eq!(entries.len(), 1, "exactly one holder-sourced entry");
    assert!((entries[0] - 0.12).abs() < 1e-6);
    app.update(); // let aggregate_stats fold the mod
    assert!(
        (app.world().get::<EffectiveStats>(hero).unwrap().damage_mult - 1.12).abs() < 1e-5,
        "folds through the buff pipeline"
    );

    // Drop: slot empties, entry revoked, item lies at the holder's feet
    // under the grace window.
    push_cmd(&mut app, ItemCommand::Drop { holder: hero, slot: 0 });
    app.update();
    assert!(slot_stack(&app, hero, 0).is_none(), "dropped");
    assert!(damage_mult_entries(&app, hero).is_empty(), "entry revoked with the drop");
    app.update();
    assert!(
        (app.world().get::<EffectiveStats>(hero).unwrap().damage_mult - 1.0).abs() < 1e-5,
        "stat back to neutral"
    );
    let items = ground_items(&mut app);
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].1, Vec3::ZERO, "dropped at the holder's feet");

    // Grace window: half a second later still no re-pick...
    for _ in 0..30 {
        app.update();
    }
    assert!(slot_stack(&app, hero, 0).is_none(), "grace window blocks the instant re-pick");

    // ...and once it expires the standing carrier re-stows (still ONE
    // entry — the upsert is idempotent across grant/revoke cycles).
    for _ in 0..40 {
        app.update();
    }
    assert!(slot_stack(&app, hero, 0).is_some(), "re-stowed after grace");
    assert_eq!(damage_mult_entries(&app, hero).len(), 1, "single entry after re-grant");
}

/// UT-INV-04b: two same-kind passives share one holder-sourced entry —
/// the documented upsert semantics (same (source, kind) replaces, so
/// carried stat sticks of one kind do not stack; see module docs).
#[test]
fn two_same_kind_passives_share_one_entry() {
    let sword = item_def(
        "sword",
        ItemKind::Passive,
        None,
        0.0,
        None,
        Some(PassiveStat { kind: StatKind::DamageMult, amount: 0.12 }),
        135,
    );
    let (mut app, hero) = carrier_app(vec![sword]);
    ground(&mut app, "sword", Vec3::new(0.4, 0.0, 0.0));
    ground(&mut app, "sword", Vec3::new(0.0, 0.0, -0.4));
    app.update();

    assert!(slot_stack(&app, hero, 0).is_some());
    assert!(slot_stack(&app, hero, 1).is_some(), "both swords stowed");
    assert_eq!(damage_mult_entries(&app, hero).len(), 1, "one entry: upsert, no stacking");
}

// ---------------------------------------------------------------------------
// INV3: shop
// ---------------------------------------------------------------------------

/// UT-INV-05: buy charges cost_gold only on success (item stowed); sell
/// refunds exactly half (50 -> 25); a broke buy skips silently and a full
/// buy rejects without charging.
#[test]
fn shop_buy_sell_gold_flows_symmetric() {
    let (mut app, buyer) =
        carrier_app(vec![item_def("potion", ItemKind::Consumable, Some(3), 0.0, None, None, 50)]);
    let shop = app.world_mut().spawn(Shop { stock: vec!["potion".into()] }).id();

    push_shop(&mut app, ShopCommand::Buy { buyer, shop, item: "potion".into() });
    app.update();
    assert_eq!(gold(&app), 950, "buy: 1000 - 50");
    assert_eq!(slot_stack(&app, buyer, 0).unwrap().def, "potion");
    assert!(drain_events(&mut app).is_empty());

    push_shop(&mut app, ShopCommand::Sell { seller: buyer, slot: 0 });
    app.update();
    assert_eq!(gold(&app), 975, "sell: 950 + 25 (50% of 50, pinned)");
    assert!(slot_stack(&app, buyer, 0).is_none(), "sold stack gone");

    // Insufficient funds: silent skip, nothing charged, nothing stowed.
    app.world_mut().resource_mut::<Resources>().gold = 10;
    push_shop(&mut app, ShopCommand::Buy { buyer, shop, item: "potion".into() });
    app.update();
    assert_eq!(gold(&app), 10);
    assert!(slot_stack(&app, buyer, 0).is_none());
    assert!(drain_events(&mut app).is_empty());

    // Full inventory: readable reject, still nothing charged.
    app.world_mut().resource_mut::<Resources>().gold = 1000;
    {
        let mut inv = app.world_mut().get_mut::<Inventory>(buyer).unwrap();
        for slot in inv.slots.iter_mut() {
            *slot = Some(ItemStack { def: "potion".into(), charges: 1, cooldown: 0.0 });
        }
    }
    push_shop(&mut app, ShopCommand::Buy { buyer, shop, item: "potion".into() });
    app.update();
    assert_eq!(gold(&app), 1000, "no charge on a full-inventory buy");
    assert_eq!(drain_events(&mut app), vec![ItemReject::InventoryFull]);
}

/// A shop-bought passive sword grants its stat entry (the shop path shares
/// the pickup's grant semantics) and selling revokes it.
#[test]
fn shop_passive_buy_grants_and_sell_revokes() {
    let sword = item_def(
        "sword",
        ItemKind::Passive,
        None,
        0.0,
        None,
        Some(PassiveStat { kind: StatKind::DamageMult, amount: 0.12 }),
        50,
    );
    let (mut app, buyer) = carrier_app(vec![sword]);
    let shop = app.world_mut().spawn(Shop { stock: vec!["sword".into()] }).id();

    push_shop(&mut app, ShopCommand::Buy { buyer, shop, item: "sword".into() });
    app.update();
    assert_eq!(gold(&app), 950);
    assert!(slot_stack(&app, buyer, 0).is_some());
    assert_eq!(damage_mult_entries(&app, buyer).len(), 1, "bought passive granted");

    push_shop(&mut app, ShopCommand::Sell { seller: buyer, slot: 0 });
    app.update();
    assert_eq!(gold(&app), 975);
    assert!(damage_mult_entries(&app, buyer).is_empty(), "sold passive revoked");
}

// ---------------------------------------------------------------------------
// INV3: loot drops
// ---------------------------------------------------------------------------

/// One seeded death-drop run: chance 1.0 always drops, chance 0.0 never.
fn loot_positions(seed: u64) -> Vec<Vec3> {
    let mut app = test_app();
    app.insert_resource(MatchRng::seeded(seed));
    app.insert_resource(DropTable(
        [(
            "grunt".to_string(),
            vec![("potion".to_string(), 1.0), ("sword".to_string(), 0.0)],
        )]
        .into_iter()
        .collect(),
    ));
    let corpse = app.world_mut().spawn(Transform::from_translation(Vec3::new(5.0, 0.0, 5.0))).id();
    app.update(); // warm-up
    app.world_mut().write_message(UnitDied {
        entity: corpse,
        def: "grunt".into(),
        gold: 0,
        wood: 0,
        xp: 0,
        killer: None,
    });
    app.update();
    ground_items(&mut app).into_iter().map(|(_, pos)| pos).collect()
}

/// UT-INV-06: chance 1.0 always drops and 0.0 never does; the item lands
/// exactly on the 0.3 scatter circle around the corpse; the same seed
/// reproduces the same result across 10 rolls (MatchRng determinism).
#[test]
fn loot_chance_one_always_drops_same_seed_same_result() {
    let first = loot_positions(42);
    assert_eq!(first.len(), 1, "chance 1.0 drops exactly one, chance 0.0 none");
    let d = (first[0] - Vec3::new(5.0, 0.0, 5.0)).length();
    assert!((d - LOOT_SCATTER_RADIUS).abs() < 1e-5, "scattered on the 0.3 circle: {d}");
    for _ in 0..10 {
        assert_eq!(loot_positions(42), first, "same seed -> identical drop positions");
    }
}

// ---------------------------------------------------------------------------
// Bundle wiring + zero drift
// ---------------------------------------------------------------------------

/// Minimal valid bundle (the bx9a sample shape) without an items section.
fn sample_bundle() -> MapBundle {
    let mut units = HashMap::new();
    units.insert("grunt".to_string(), carrier_unit_def());
    MapBundle {
        identity: MapIdentity { id: "inv-sample".into(), version: MAP_FORMAT_VERSION },
        world: WorldDef {
            paths: vec![PathDef::closed_loop(vec![[0.0, 0.0], [4.0, 0.0], [4.0, 4.0], [0.0, 4.0]])],
            build: BuildDomainDef::FreeGrid { cell: 1.0, cols: 16, rows: 16 },
            blocked: vec![],
        },
        units,
        waves: WaveTable {
            waves: vec![WaveEntry {
                spawns: vec![SpawnSpec {
                    unit: "grunt".into(),
                    count: 2,
                    interval: 0.5,
                    path: 0,
                    delay: 0.0,
                    hp_scale: 1.0,
                }],
                boss_timeout: None,
                preview: "2 grunts".into(),
                reward_gold: 0,
                challenge: false,
                }],
            advance: Advance::Auto { intermission: 1.0 },
            early_call_bonus: Some(10),
        },
        rules: MatchRules {
            leak: LeakPolicy::Stockpile { cap: 10 },
            build_window: BuildWindow::Always,
            lap_buff: Some(LapBuffPolicy { speed_pct: 0.05, hp_pct: 0.03 }),
            win: WinCondition::SurviveWaves { waves: 1 },
            sell_refund_pct: 0.7,
        },
        econ_start: EconStart { gold: 120, wood: 5, pop_cap: 8 },
        econ_rules: EconRules::default(),
        attack_armor: AttackArmorTable::default(),
        nav: Default::default(),
        vision: Default::default(),
        items: Default::default(),
        production: Default::default(),
        teams: Default::default(),
        starting_forces: Default::default(),
    harvest: Default::default(),
    camps: Default::default(),
    daynight: Default::default(),
    terrain: Default::default(),
    }
}

/// UT-INV-07: a bundle without an items section assembles to an empty
/// registry and drop table, and the item systems stay inert — deaths drop
/// nothing, no rejects fire (zero drift for every existing map).
#[test]
fn itemless_bundle_is_inert_zero_drift() {
    let mut app = test_app();
    assemble(&mut app.world_mut(), &sample_bundle());
    assert!(app.world().resource::<ItemRegistry>().defs.is_empty());
    assert!(app.world().resource::<DropTable>().0.is_empty());

    let corpse = app.world_mut().spawn(Transform::from_translation(Vec3::ONE)).id();
    app.update();
    app.world_mut().write_message(UnitDied {
        entity: corpse,
        def: "grunt".into(),
        gold: 0,
        wood: 0,
        xp: 0,
        killer: None,
    });
    for _ in 0..10 {
        app.update();
    }
    assert!(ground_items(&mut app).is_empty(), "no loot without a drop table");
    assert!(drain_events(&mut app).is_empty(), "no rejects without items");
}

/// UT-INV-08: apply_items loads the registry and drop table from the
/// bundle's items section.
#[test]
fn apply_items_loads_registry_and_drop_table() {
    let mut app = test_app();
    let mut bundle = sample_bundle();
    bundle.items.items.insert(
        "potion".into(),
        item_def("potion", ItemKind::Consumable, Some(3), 0.0, Some("heal"), None, 50),
    );
    bundle.items.drops.insert("grunt".into(), vec![("potion".into(), 1.0)]);
    assemble(&mut app.world_mut(), &bundle);
    assert!(app.world().resource::<ItemRegistry>().get("potion").is_some());
    assert_eq!(
        app.world().resource::<DropTable>().0["grunt"],
        vec![("potion".to_string(), 1.0)]
    );
}
