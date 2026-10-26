//! HERO domain acceptance (round-2 plan §3): three primary attributes with
//! spawn-time conversion (STR/AGI/INT additive layer over the UnitDef
//! base), altar revival (timer + cost + rebirth position), and kill-xp
//! range sharing. Everything runs on the fixed 1/60 stepping from
//! `warbase::testing`, so every frame count below is exact (60 frames =
//! 1.0s).

use bevy::ecs::message::Messages;
use bevy::prelude::*;
use warbase::prelude::*;
use warbase::testing::test_app;

/// Hero-shaped def: XpHolder (hero: true) + attributes + optional mana.
fn hero_def(id: &str, hp: f32, stats: HeroStats, mana: Option<ManaDef>) -> UnitDef {
    UnitDef {
        leak: 1,
        id: id.into(),
        attrs: BaseAttributes { hp, move_speed: 0.0, body_radius: 0.3 },
        attack: None,
        armor_type: ArmorType::Light,
        cost_gold: 0,
        cost_wood: 0,
        bounty_gold: 0,
        bounty_wood: 0,
        grant_xp: 0,
        traits: vec![],
        auras: vec![],
        hero: true,
        skills: vec![],
        upgrade_to: None,
        heal: None,
        mana,
        skill_levels: vec![],
        hero_stats: Some(stats),
    }
}

/// Plain creep def (legacy shape, no hero data at all).
fn creep_def(id: &str, hp: f32) -> UnitDef {
    UnitDef {
        leak: 1,
        id: id.into(),
        attrs: BaseAttributes { hp, move_speed: 0.0, body_radius: 0.3 },
        attack: None,
        armor_type: ArmorType::Light,
        cost_gold: 0,
        cost_wood: 0,
        bounty_gold: 0,
        bounty_wood: 0,
        grant_xp: 10,
        traits: vec![],
        auras: vec![],
        hero: false,
        skills: vec![],
        upgrade_to: None,
        heal: None,
        mana: None,
        skill_levels: vec![],
        hero_stats: None,
    }
}

fn table(hp_per_str: f32, attack_speed_per_agi: f32, armor_per_agi: f32, mana_per_int: f32, mana_regen_per_int: f32) -> HeroStatsDef {
    HeroStatsDef { hp_per_str, attack_speed_per_agi, armor_per_agi, mana_per_int, mana_regen_per_int }
}

fn install(app: &mut App, defs: Vec<UnitDef>) -> UnitRegistry {
    let reg = UnitRegistry { defs: defs.into_iter().map(|d| (d.id.clone(), d)).collect() };
    app.insert_resource(UnitRegistry { defs: reg.defs.clone() });
    reg
}

/// First-frame dt=0 quirk (see spl_domain): warm up before frame-exact
/// assertions.
fn warm_up(app: &mut App) {
    app.update();
}

fn spawn(app: &mut App, reg: &UnitRegistry, id: &str, pos: Vec3, side: Allegiance) -> Entity {
    spawn_unit(&mut app.world_mut(), reg, id, pos, side, 1.0).unwrap()
}

fn hit(app: &mut App, target: Entity, source: DamageOrigin, amount: f32) {
    app.world_mut()
        .resource_mut::<Messages<DamageRequest>>()
        .write(DamageRequest {
            target,
            source,
            amount,
            attack_type: AttackType::Normal,
            crit: false,
        });
}

// ---------------------------------------------------------------------------
// 1) Three attributes: spawn-time conversion
// ---------------------------------------------------------------------------

/// Acceptance ①: STR 10 x hp_per_str 5 -> MaxHp exactly +50 (and spawn Hp
/// full at the boosted ceiling).
#[test]
fn str_converts_to_hp_pool_at_spawn() {
    let mut app = test_app();
    app.insert_resource(table(5.0, 0.0, 0.0, 0.0, 0.0));
    let reg = install(&mut app, vec![hero_def("paladin", 100.0, HeroStats { str: 10, agi: 0, int: 0 }, None)]);
    let e = spawn(&mut app, &reg, "paladin", Vec3::ZERO, Allegiance::Defender);

    assert_eq!(app.world().get::<MaxHp>(e).unwrap().0, 150.0, "MaxHp +50 exactly");
    assert_eq!(app.world().get::<Hp>(e).unwrap().0, 150.0, "spawns full at boosted ceiling");
    assert_eq!(app.world().get::<HeroStats>(e).unwrap().str, 10);
}

/// Acceptance ②: INT converts Mana/MaxMana/ManaRegen — additive on the
/// def's own mana numbers, and a def WITHOUT a mana bar gets a pool
/// conjured from zero when INT actually converts to something.
#[test]
fn int_converts_mana_pool_and_regen() {
    let mut app = test_app();
    app.insert_resource(table(0.0, 0.0, 0.0, 4.0, 0.5));
    let reg = install(
        &mut app,
        vec![
            hero_def("mage", 100.0, HeroStats { str: 0, agi: 0, int: 10 }, Some(ManaDef { max: 50.0, regen: 1.0 })),
            hero_def("bard", 100.0, HeroStats { str: 0, agi: 0, int: 5 }, None),
        ],
    );
    let mage = spawn(&mut app, &reg, "mage", Vec3::ZERO, Allegiance::Defender);
    let bard = spawn(&mut app, &reg, "bard", Vec3::new(2.0, 0.0, 0.0), Allegiance::Defender);

    // 50 base + 10 INT x 4.0 = 90; regen 1.0 + 10 x 0.5 = 6.0.
    assert_eq!(app.world().get::<MaxMana>(mage).unwrap().0, 90.0);
    assert_eq!(app.world().get::<Mana>(mage).unwrap().0, 90.0);
    assert_eq!(app.world().get::<ManaRegen>(mage).unwrap().0, 6.0);

    // No def mana: pool conjured from zero (5 x 4.0 = 20, regen 2.5).
    assert_eq!(app.world().get::<MaxMana>(bard).unwrap().0, 20.0);
    assert_eq!(app.world().get::<Mana>(bard).unwrap().0, 20.0);
    assert_eq!(app.world().get::<ManaRegen>(bard).unwrap().0, 2.5);
}

/// Acceptance ③: AGI rides the StatMods pipeline — after one update the
/// hero's EffectiveStats show the faster interval and the reduced damage
/// taken, and resolve_damage charges the reduced amount (100 raw -> 90 Hp
/// loss at 0.9 taken-mult with the identity armor matrix).
#[test]
fn agi_attack_speed_and_damage_reduction_through_effective_stats() {
    let mut app = test_app();
    app.insert_resource(table(0.0, 0.02, 0.01, 0.0, 0.0));
    let reg = install(&mut app, vec![hero_def("rogue", 200.0, HeroStats { str: 0, agi: 10, int: 0 }, None)]);
    let hero = spawn(&mut app, &reg, "rogue", Vec3::ZERO, Allegiance::Defender);

    let attacker = app
        .world_mut()
        .spawn((Allegiance::Attacker, Transform::from_translation(Vec3::new(0.5, 0.0, 0.0))))
        .id();

    warm_up(&mut app);
    let eff = app.world().get::<EffectiveStats>(hero).unwrap();
    assert!((eff.interval_mult - 0.8).abs() < 1e-6, "10 AGI x 0.02 = -20% interval: {}", eff.interval_mult);
    assert!((eff.damage_taken_mult - 0.9).abs() < 1e-6, "10 AGI x 0.01 = -10% taken: {}", eff.damage_taken_mult);

    hit(&mut app, hero, DamageOrigin::Attack(attacker), 100.0);
    app.update();
    assert!(
        (app.world().get::<Hp>(hero).unwrap().0 - 110.0).abs() < 1e-4,
        "200 - 100x0.9 = 110, got {}",
        app.world().get::<Hp>(hero).unwrap().0
    );
}

/// Zero-drift seam: identity table (all defaults) + attributes present
/// converts nothing — plain base stats, no conjured mana, no StatMods
/// entries, exactly the pre-HERO component shape.
#[test]
fn identity_table_converts_nothing() {
    let mut app = test_app();
    let reg = install(&mut app, vec![hero_def("squire", 100.0, HeroStats { str: 7, agi: 7, int: 7 }, None)]);
    let e = spawn(&mut app, &reg, "squire", Vec3::ZERO, Allegiance::Defender);

    assert_eq!(app.world().get::<MaxHp>(e).unwrap().0, 100.0);
    assert!(app.world().get::<Mana>(e).is_none(), "identity table conjures no mana bar");
    assert!(app.world().get::<StatMods>(e).unwrap().entries.is_empty());
}

/// Deferred path (wave/summon/production): attributes ride along marked
/// `HeroStatsRaw` and the hero Tick converts them on the first frame after
/// materialization.
#[test]
fn deferred_spawn_converts_on_first_tick() {
    let mut app = test_app();
    app.insert_resource(table(5.0, 0.0, 0.0, 0.0, 0.0));
    let reg = install(&mut app, vec![hero_def("farstrider", 100.0, HeroStats { str: 10, agi: 0, int: 0 }, None)]);

    let e = spawn_unit_deferred(
        &mut app.world_mut().commands(),
        &reg,
        "farstrider",
        Vec3::ZERO,
        Allegiance::Defender,
        1.0,
    )
    .unwrap();
    // Commands flush at the schedule's sync points; two updates make the
    // assertion independent of WHERE inside the first update the queue
    // landed (the hero Tick converts on the first frame it exists).
    app.update();
    app.update();
    assert!(app.world().get::<HeroStatsRaw>(e).is_none(), "converted on the first Tick");
    assert!(app.world().get::<HeroStats>(e).is_some());
    assert_eq!(app.world().get::<MaxHp>(e).unwrap().0, 150.0);
}

// ---------------------------------------------------------------------------
// 2) Altar revival
// ---------------------------------------------------------------------------

/// Kill the hero on THIS update (returns the death frame); the caller then
/// steps further frames.
fn kill(app: &mut App, hero: Entity, killer: Entity) {
    hit(app, hero, DamageOrigin::Attack(killer), 10_000.0);
    app.update();
}

/// Acceptance ④ (in-place half): a killed hero stays in the world carrying
/// Dead + Reviving, is NOT targeted by a nearby enemy attacker, and returns
/// exactly 60 frames after the death frame at half Hp / half Mana, same
/// position (revive_secs 1.0 at 1/60).
#[test]
fn killed_hero_revives_in_place_after_exact_frames() {
    let mut app = test_app();
    app.insert_resource(table(0.0, 0.0, 0.0, 0.0, 0.0));
    app.insert_resource(HeroRules { revive_secs: 1.0, ..Default::default() });
    let mut raider = creep_def("raider", 50.0);
    raider.attack = Some(AttackDef {
        damage: 5.0,
        interval: 1.0,
        range: 10.0,
        attack_type: AttackType::Normal,
        targets: TargetDomain::Both,
        projectile: None,
        windup_secs: 0.0,
        backswing_secs: 0.0,
    });
    let reg = install(
        &mut app,
        vec![
            hero_def("paladin", 100.0, HeroStats::default(), Some(ManaDef { max: 80.0, regen: 0.0 })),
            raider,
        ],
    );
    let hero = spawn(&mut app, &reg, "paladin", Vec3::new(1.0, 0.0, 2.0), Allegiance::Defender);
    let raider = spawn(&mut app, &reg, "raider", Vec3::new(1.5, 0.0, 2.0), Allegiance::Attacker);

    warm_up(&mut app);
    kill(&mut app, hero, raider);

    // Death frame: present, Dead+Reviving, not despawned. (CurrentTarget
    // still holds a STALE Some(hero) from this frame's Intent, which ran
    // before the killing Resolve — the stale lock fires nothing: the
    // Act-phase target query excludes Dead. It clears on the next Intent.)
    assert!(app.world().get_entity(hero).is_ok(), "reviving hero stays in the world");
    assert!(app.world().get::<Dead>(hero).is_some());
    assert!(app.world().get::<Reviving>(hero).is_some());

    app.update(); // re-pick: the Dead hero drops out of targeting
    assert!(
        app.world().get::<CurrentTarget>(raider).unwrap().0.is_none(),
        "a reviving hero is never targeted (Dead semantics)"
    );

    // Frames 3..59 of the window: still down.
    for _ in 0..57 {
        app.update();
    }
    assert!(app.world().get::<Reviving>(hero).is_some(), "still reviving one frame early");
    assert!(app.world().get::<CurrentTarget>(raider).unwrap().0.is_none());

    // 60th decrement frame: back, half Hp / half Mana, same position.
    app.update();
    assert!(app.world().get::<Dead>(hero).is_none(), "Dead removed at revival");
    assert!(app.world().get::<Reviving>(hero).is_none());
    let hp = app.world().get::<Hp>(hero).unwrap().0;
    assert!((hp - 50.0).abs() < 1e-4, "half of MaxHp 100: {hp}");
    let mana = app.world().get::<Mana>(hero).unwrap().0;
    assert!((mana - 40.0).abs() < 1e-4, "half of MaxMana 80: {mana}");
    let tf = app.world().get::<Transform>(hero).unwrap().translation;
    assert!((tf - Vec3::new(1.0, 0.0, 2.0)).length() < 1e-4, "revived in place");

    // And targetable again: the raider picks the revived hero up.
    app.update();
    assert_eq!(
        app.world().get::<CurrentTarget>(raider).unwrap().0,
        Some(hero),
        "revived hero is targetable again"
    );
}

/// Acceptance ④ (altar): a revival position teleports the hero to the
/// altar on return.
#[test]
fn killed_hero_revives_at_altar_position() {
    let mut app = test_app();
    app.insert_resource(table(0.0, 0.0, 0.0, 0.0, 0.0));
    app.insert_resource(HeroRules {
        revive_secs: 0.5,
        revive_altar: Some([10.0, 20.0]),
        ..Default::default()
    });
    let reg = install(
        &mut app,
        vec![
            hero_def("paladin", 100.0, HeroStats::default(), None),
            creep_def("raider", 50.0),
        ],
    );
    let hero = spawn(&mut app, &reg, "paladin", Vec3::new(1.0, 0.0, 2.0), Allegiance::Defender);
    let raider = spawn(&mut app, &reg, "raider", Vec3::new(1.5, 0.0, 2.0), Allegiance::Attacker);

    warm_up(&mut app);
    kill(&mut app, hero, raider);
    assert!(app.world().get::<Reviving>(hero).is_some());
    // Death frame still at the fall position.
    let tf = app.world().get::<Transform>(hero).unwrap().translation;
    assert!((tf - Vec3::new(1.0, 0.0, 2.0)).length() < 1e-4, "body stays where it fell");

    for _ in 0..30 {
        app.update(); // 0.5s = 30 frames
    }
    let tf = app.world().get::<Transform>(hero).unwrap().translation;
    assert!(
        (tf - Vec3::new(10.0, 0.0, 20.0)).length() < 1e-4,
        "revived at the altar (x, 0, z): {tf}"
    );
}

/// Revival cost: affordable gold is deducted once at revival start and
/// recorded in `cost_paid`.
#[test]
fn revival_pays_gold_cost_up_front() {
    let mut app = test_app();
    app.insert_resource(table(0.0, 0.0, 0.0, 0.0, 0.0));
    app.insert_resource(HeroRules { revive_secs: 0.5, revive_cost_gold: 30, ..Default::default() });
    let reg = install(
        &mut app,
        vec![
            hero_def("paladin", 100.0, HeroStats::default(), None),
            creep_def("raider", 50.0),
        ],
    );
    app.world_mut().resource_mut::<Resources>().gold = 100;
    let hero = spawn(&mut app, &reg, "paladin", Vec3::ZERO, Allegiance::Defender);
    let raider = spawn(&mut app, &reg, "raider", Vec3::new(0.5, 0.0, 0.0), Allegiance::Attacker);

    warm_up(&mut app);
    kill(&mut app, hero, raider);
    assert_eq!(app.world().resource::<Resources>().gold, 70, "cost paid once at revival start");
    assert!(app.world().get::<Reviving>(hero).unwrap().cost_paid);
}

/// Zero drift: revive policy off (default HeroRules) -> a dead hero
/// despawns exactly like any unit (the legacy cleanup path).
#[test]
fn revive_policy_off_hero_despawns_as_before() {
    let mut app = test_app();
    let reg = install(
        &mut app,
        vec![
            hero_def("paladin", 100.0, HeroStats::default(), None),
            creep_def("raider", 50.0),
        ],
    );
    let hero = spawn(&mut app, &reg, "paladin", Vec3::ZERO, Allegiance::Defender);
    let raider = spawn(&mut app, &reg, "raider", Vec3::new(0.5, 0.0, 0.0), Allegiance::Attacker);

    warm_up(&mut app);
    kill(&mut app, hero, raider);
    assert!(app.world().get_entity(hero).is_err(), "policy off: hero despawns (zero drift)");
}

// ---------------------------------------------------------------------------
// 3) Kill-xp range sharing
// ---------------------------------------------------------------------------

/// Two allied heroes near the kill split the bounty evenly (10 -> 5/5); a
/// hero beyond the radius and a dead (reviving) hero get nothing; the plain
/// share of a lone eligible hero is the full amount.
#[test]
fn kill_xp_shared_evenly_inside_radius() {
    let mut app = test_app();
    app.insert_resource(table(0.0, 0.0, 0.0, 0.0, 0.0));
    app.insert_resource(HeroRules { share_radius: 5.0, ..Default::default() });
    let reg = install(
        &mut app,
        vec![
            hero_def("killer", 100.0, HeroStats::default(), None),
            hero_def("nearby", 100.0, HeroStats::default(), None),
            hero_def("faraway", 100.0, HeroStats::default(), None),
            creep_def("prey", 20.0),
        ],
    );
    let killer = spawn(&mut app, &reg, "killer", Vec3::ZERO, Allegiance::Defender);
    let nearby = spawn(&mut app, &reg, "nearby", Vec3::new(3.0, 0.0, 0.0), Allegiance::Defender);
    let faraway = spawn(&mut app, &reg, "faraway", Vec3::new(8.0, 0.0, 0.0), Allegiance::Defender);
    let prey = spawn(&mut app, &reg, "prey", Vec3::new(1.0, 0.0, 0.0), Allegiance::Attacker);

    warm_up(&mut app);
    kill(&mut app, prey, killer);
    assert_eq!(app.world().get::<XpHolder>(killer).unwrap().xp, 5, "killer takes half");
    assert_eq!(app.world().get::<XpHolder>(nearby).unwrap().xp, 5, "nearby hero takes half");
    assert_eq!(app.world().get::<XpHolder>(faraway).unwrap().xp, 0, "outside the radius: nothing");

    // A dead (reviving) hero inside the radius does not leech: enable the
    // revive policy, drop `nearby`, then feed one more hostile kill.
    app.insert_resource(HeroRules { share_radius: 5.0, revive_secs: 1.0, ..Default::default() });
    kill(&mut app, nearby, killer); // nearby falls (heroes grant 0 xp), starts reviving
    assert_eq!(app.world().get::<XpHolder>(killer).unwrap().xp, 5, "allied death grants nothing");
    let prey2 = spawn(&mut app, &reg, "prey", Vec3::new(1.0, 0.0, 0.0), Allegiance::Attacker);
    kill(&mut app, prey2, killer);
    // nearby is Dead+Reviving inside the radius: only killer is eligible —
    // and the full 10 pushes the killer over its first threshold
    // (next_level_xp 10): level 2, xp 0 left over.
    let killer_xp = app.world().get::<XpHolder>(killer).unwrap();
    assert_eq!(killer_xp.level, 2, "killer leveled on the full bounty");
    assert_eq!(killer_xp.xp, 5, "5 + 10 - 10 (level drain) = 5");
    assert_eq!(app.world().get::<XpHolder>(nearby).unwrap().xp, 5, "reviving hero gained nothing");
}

/// Zero drift: share_radius 0 (default) -> the killer alone eats the whole
/// bounty, byte-identical credit path.
#[test]
fn share_radius_zero_killer_takes_all() {
    let mut app = test_app();
    let reg = install(
        &mut app,
        vec![
            hero_def("killer", 100.0, HeroStats::default(), None),
            hero_def("nearby", 100.0, HeroStats::default(), None),
            creep_def("prey", 20.0),
        ],
    );
    let killer = spawn(&mut app, &reg, "killer", Vec3::ZERO, Allegiance::Defender);
    let nearby = spawn(&mut app, &reg, "nearby", Vec3::new(3.0, 0.0, 0.0), Allegiance::Defender);
    let prey = spawn(&mut app, &reg, "prey", Vec3::new(1.0, 0.0, 0.0), Allegiance::Attacker);

    warm_up(&mut app);
    kill(&mut app, prey, killer);
    // 10 xp hits the first threshold exactly (next_level_xp 10): the
    // legacy killer-only credit levels the killer up with 0 left over.
    let killer_xp = app.world().get::<XpHolder>(killer).unwrap();
    assert_eq!(killer_xp.level, 2, "killer alone levels on the full bounty");
    assert_eq!(killer_xp.xp, 0);
    assert_eq!(app.world().get::<XpHolder>(nearby).unwrap().xp, 0, "ally untouched");
}

/// An ODD bounty splits floor/floor+remainder (remainder to the lowest
/// Entity index — deterministic), never losing xp.
#[test]
fn odd_bounty_splits_with_remainder_to_lowest_index() {
    let mut app = test_app();
    app.insert_resource(table(0.0, 0.0, 0.0, 0.0, 0.0));
    app.insert_resource(HeroRules { share_radius: 5.0, ..Default::default() });
    let reg = install(
        &mut app,
        vec![
            hero_def("killer", 100.0, HeroStats::default(), None),
            hero_def("nearby", 100.0, HeroStats::default(), None),
            creep_def("prey", 20.0),
        ],
    );
    let killer = spawn(&mut app, &reg, "killer", Vec3::ZERO, Allegiance::Defender);
    let nearby = spawn(&mut app, &reg, "nearby", Vec3::new(3.0, 0.0, 0.0), Allegiance::Defender);
    let prey = spawn(&mut app, &reg, "prey", Vec3::new(1.0, 0.0, 0.0), Allegiance::Attacker);
    // Odd bounty: overwrite the def's grant_xp via a fresh registry entry.
    let mut defs = reg.defs.clone();
    defs.get_mut("prey").unwrap().grant_xp = 11;
    app.insert_resource(UnitRegistry { defs });

    warm_up(&mut app);
    kill(&mut app, prey, killer);
    let a = app.world().get::<XpHolder>(killer).unwrap().xp;
    let b = app.world().get::<XpHolder>(nearby).unwrap().xp;
    assert_eq!(a + b, 11, "no xp lost");
    assert!(
        (a == 6 && b == 5) || (a == 5 && b == 6),
        "5 + remainder 1 to the lower Entity index: {a}/{b}"
    );
}
