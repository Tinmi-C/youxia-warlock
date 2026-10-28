//! SKVOC domain acceptance (round-2 plan §3): area (AOE) targeting,
//! stun/silence primitives (with cast interrupt), the summon primitive,
//! and attack windup/backswing. Every frame count runs on the fixed 1/60
//! stepping from `warbase::testing`, with one warmup frame consumed first
//! (the harness clock's first-frame dt=0 quirk, ts0_determinism), so all
//! numbers below are exact.

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use warbase::prelude::*;
use warbase::testing::{count_over_frames, test_app};

/// 1/60 in f32 — the exact per-frame delta the assertions count in.
const DT: f32 = 1.0 / 60.0;

// --------------------------------------------------------------------------
// fixtures
// --------------------------------------------------------------------------

fn udef(id: &str, hp: f32) -> UnitDef {
    UnitDef {
        hero_stats: None,
        pop_cost: 1,
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
        hero: false,
        skills: vec![],
        upgrade_to: None,
        heal: None,
        mana: None,
        skill_levels: vec![],
    }
}

fn atk(damage: f32, interval: f32, range: f32, windup: f32, backswing: f32) -> AttackDef {
    AttackDef {
        damage,
        interval,
        range,
        attack_type: AttackType::Normal,
        targets: TargetDomain::Both,
        projectile: None,
        windup_secs: windup,
        backswing_secs: backswing,
    }
}

fn registry_of(defs: Vec<UnitDef>) -> UnitRegistry {
    UnitRegistry { defs: defs.into_iter().map(|d| (d.id.clone(), d)).collect() }
}

fn warm_up(app: &mut App) {
    app.update();
}

fn run(app: &mut App, frames: usize) {
    for _ in 0..frames {
        app.update();
    }
}

/// Persistent-cursor DamageDealt tally. `count_over_frames` creates a
/// fresh cursor per call, and Messages retain two frames of history, so a
/// hit on a slice boundary gets re-counted by the next slice's fresh
/// cursor. One cursor across the whole test counts every hit exactly once.
struct DealtTally {
    cursor: MessageCursor<DamageDealt>,
}

impl DealtTally {
    fn new() -> Self {
        Self { cursor: MessageCursor::default() }
    }

    /// Advance `frames` updates, returning the DamageDealt messages written
    /// during them (each counted exactly once over this tally's lifetime).
    fn over(&mut self, app: &mut App, frames: usize) -> usize {
        let mut total = 0;
        for _ in 0..frames {
            app.update();
            let msgs = app.world().resource::<Messages<DamageDealt>>();
            total += self.cursor.read(msgs).count();
        }
        total
    }
}

fn cast(app: &mut App, caster: Entity, slot: usize) {
    app.world_mut()
        .resource_mut::<Messages<CastSkill>>()
        .write(CastSkill { caster, slot, target: None });
}

fn apply_status(app: &mut App, target: Entity, status: StatusKind, duration: f32) {
    app.world_mut()
        .resource_mut::<Messages<ApplyStatus>>()
        .write(ApplyStatus { target, status, duration });
}

/// A shooter (Defender, origin) with a customizable attack kit plus a
/// living Attacker victim in range.
fn duel_app(
    interval: f32,
    windup: f32,
    backswing: f32,
    victim_hp: f32,
) -> (App, Entity, Entity) {
    let mut app = test_app();
    let mut shooter = udef("shooter", 100.0);
    shooter.attack = Some(atk(50.0, interval, 100.0, windup, backswing));
    let victim = udef("victim", victim_hp);
    let reg = registry_of(vec![shooter, victim]);
    app.insert_resource(UnitRegistry { defs: reg.defs.clone() });
    let s = spawn_unit(&mut app.world_mut(), &reg, "shooter", Vec3::ZERO, Allegiance::Defender, 1.0)
        .unwrap();
    let v = spawn_unit(
        &mut app.world_mut(),
        &reg,
        "victim",
        Vec3::new(3.0, 0.0, 0.0),
        Allegiance::Attacker,
        1.0,
    )
    .unwrap();
    // Block firing during the warmup frame (dt=0, ts0 quirk) so the first
    // real shot/windup starts on the first counted update; then rearm.
    app.world_mut().entity_mut(s).insert(AttackCooldown(interval));
    warm_up(&mut app);
    app.world_mut().entity_mut(s).insert(AttackCooldown(0.0));
    (app, s, v)
}

// --------------------------------------------------------------------------
// (1) Area targeting
// --------------------------------------------------------------------------

/// Acceptance 1: `area_targets` picks exactly the living enemies of `from`
/// inside the radius (inclusive boundary), allies and out-of-circle units
/// excluded, dead/zero-hp excluded, in Entity-index order.
#[test]
fn area_pick_exactly_three_enemies_in_index_order() {
    let mut app = test_app();
    let world = app.world_mut();
    // Vocabulary pin: an Area skill def carries the radius (compile + data).
    let mut defs = SkillDefs::default();
    defs.defs.insert(
        "blizzard".into(),
        SkillDef {
            id: "blizzard".into(),
            kind: SkillKind::Active,
            targeting: SkillTargeting::Area,
            max_level: 1,
            cooldown: 6.0,
            mana_cost: 0.0,
            cast_time: 0.5,
            channel: None,
            area_radius: 5.0,
            summon: None,
            badge: None,
            hotkey: None,
        },
    );
    world.insert_resource(defs);

    let _ally_in = world
        .spawn((Allegiance::Defender, Transform::from_translation(Vec3::new(1.0, 0.0, 0.0)), Hp(50.0)))
        .id();
    let e2 = world
        .spawn((Allegiance::Attacker, Transform::from_translation(Vec3::new(2.0, 0.0, 0.0)), Hp(50.0)))
        .id();
    let _enemy_out = world
        .spawn((Allegiance::Attacker, Transform::from_translation(Vec3::new(10.0, 0.0, 0.0)), Hp(50.0)))
        .id();
    let e4 = world
        .spawn((Allegiance::Attacker, Transform::from_translation(Vec3::new(3.0, 0.0, 0.0)), Hp(50.0)))
        .id();
    let _dead_in = world
        .spawn((
            Allegiance::Attacker,
            Transform::from_translation(Vec3::new(4.0, 0.0, 0.0)),
            Hp(50.0),
            Dead,
        ))
        .id();
    let _zero_hp_in = world
        .spawn((Allegiance::Attacker, Transform::from_translation(Vec3::new(4.5, 0.0, 0.0)), Hp(0.0)))
        .id();
    let _ally_out = world
        .spawn((Allegiance::Defender, Transform::from_translation(Vec3::new(9.0, 0.0, 0.0)), Hp(50.0)))
        .id();
    // Exactly on the circle boundary (distance 5.0): included (<=).
    let e_boundary = world
        .spawn((Allegiance::Attacker, Transform::from_translation(Vec3::new(5.0, 0.0, 0.0)), Hp(50.0)))
        .id();

    let picked = area_targets(app.world_mut(), Vec3::ZERO, 5.0, Allegiance::Defender);
    assert_eq!(picked, vec![e2, e4, e_boundary], "exactly the three enemies, index order");

    // An attacker-centered query sees the defender instead.
    let picked = area_targets(app.world_mut(), Vec3::ZERO, 5.0, Allegiance::Attacker);
    assert_eq!(picked, vec![_ally_in], "hostility is from-perspective");
}

// --------------------------------------------------------------------------
// (2)/(3)/(4) Stun and silence
// --------------------------------------------------------------------------

/// Acceptance 2: stun 2.0s (120 frames) on a fighting, walking unit —
/// zero DamageDealt and zero displacement for exactly those 120 frames,
/// then both resume.
#[test]
fn stun_freezes_attacks_and_movement_then_recovers() {
    let mut app = test_app();
    let mut tower = udef("tower", 100.0);
    tower.attack = Some(atk(5.0, 0.5, 100.0, 0.0, 0.0));
    let grunt = udef("grunt", 9999.0);
    let reg = registry_of(vec![tower, grunt]);
    app.insert_resource(UnitRegistry { defs: reg.defs.clone() });
    let t = spawn_unit(&mut app.world_mut(), &reg, "tower", Vec3::ZERO, Allegiance::Defender, 1.0)
        .unwrap();
    let _g = spawn_unit(
        &mut app.world_mut(),
        &reg,
        "grunt",
        Vec3::new(2.0, 0.0, 0.0),
        Allegiance::Attacker,
        1.0,
    )
    .unwrap();
    app.world_mut().entity_mut(t).insert(MoveAgent {
        speed: 5.0,
        arrival_radius: 0.1,
        waypoints: vec![Vec3::new(20.0, 0.0, 0.0)],
        velocity: Vec3::ZERO,
        body_radius: 0.3,
        order: None,
    }).insert(AgentOrderState::Idle);
    // Block the warmup frame's dt=0 shot; rearm after.
    app.world_mut().entity_mut(t).insert(AttackCooldown(0.5));
    warm_up(&mut app);
    app.world_mut().entity_mut(t).insert(AttackCooldown(0.0));

    let mut tally = DealtTally::new();
    apply_status(&mut app, t, StatusKind::Stun, 2.0);
    // Updates 1..120 post-application: fully frozen (the removal lands on
    // update 121's Tick).
    assert_eq!(tally.over(&mut app, 120), 0, "no hits during the 2s stun");
    assert_eq!(
        app.world().get::<Transform>(t).unwrap().translation,
        Vec3::ZERO,
        "zero displacement during the stun"
    );
    assert!(
        app.world().get::<Stunned>(t).is_some(),
        "still stunned through frame 120 (expiry on 121)"
    );

    // Recovery on the very next frame: damage and movement both resume.
    assert_eq!(tally.over(&mut app, 1), 1, "attacks resume the frame the stun drops");
    assert!(
        app.world().get::<Transform>(t).unwrap().translation.x > 0.0,
        "movement resumes along the preserved route"
    );
}

/// Acceptance 3: a stun at 0.5s into a 1.0s cast interrupts it — no
/// SkillCast ever, no mana refund, slot cooldown restarted at full from
/// the interrupt frame (InterruptCast semantics, reached via ApplyStatus).
#[test]
fn stun_interrupts_cast_cooldown_from_interrupt_frame() {
    let mut app = test_app();
    let mut u = udef("mage", 100.0);
    u.mana = Some(ManaDef { max: 100.0, regen: 0.0 });
    let def = SkillDef {
        id: "pyro".into(),
        kind: SkillKind::Active,
        targeting: SkillTargeting::None,
        max_level: 3,
        cooldown: 5.0,
        mana_cost: 30.0,
        cast_time: 1.0,
        channel: None,
        area_radius: 0.0,
        summon: None,
        badge: None,
        hotkey: None,
    };
    let reg = registry_of(vec![u]);
    app.insert_resource(UnitRegistry { defs: reg.defs.clone() });
    let mut defs = SkillDefs::default();
    defs.defs.insert("pyro".into(), def);
    app.insert_resource(defs);
    let e = spawn_unit(&mut app.world_mut(), &reg, "mage", Vec3::ZERO, Allegiance::Defender, 1.0)
        .unwrap();
    app.world_mut().entity_mut(e).insert(SkillCaster {
        slots: vec![SkillSlot { skill: "pyro".into(), level: 1, cooldown: 0.0, autocast_on: false }],
    });
    warm_up(&mut app);

    cast(&mut app, e, 0);
    assert_eq!(count_over_frames::<SkillCast>(&mut app, 30), 0, "0.5s: mid-cast");
    assert!(app.world().get::<Casting>(e).is_some());

    apply_status(&mut app, e, StatusKind::Stun, 2.0);
    app.update(); // the interrupt frame
    assert!(app.world().get::<Casting>(e).is_none(), "stun cancelled the cast");
    let cd = app.world().get::<SkillCaster>(e).unwrap().slots[0].cooldown;
    assert!((cd - 5.0).abs() < 1e-5, "cooldown restarted at full from the interrupt frame: {cd}");

    assert_eq!(count_over_frames::<SkillCast>(&mut app, 5), 0, "interrupted cast never fires");
    let mana = app.world().get::<Mana>(e).unwrap().0;
    assert!((mana - 70.0).abs() < 1e-5, "no mana refund: {mana}");
}

/// Acceptance 4: silence refuses new casts with no side effects (no
/// SkillCast, no cooldown burn, mana untouched) until it expires.
#[test]
fn silence_refuses_casts_until_expiry() {
    let mut app = test_app();
    let mut u = udef("mage", 100.0);
    u.mana = Some(ManaDef { max: 100.0, regen: 0.0 });
    let reg = registry_of(vec![u]);
    app.insert_resource(UnitRegistry { defs: reg.defs.clone() });
    let mut defs = SkillDefs::default();
    defs.defs.insert(
        "bolt".into(),
        SkillDef {
            id: "bolt".into(),
            kind: SkillKind::Active,
            targeting: SkillTargeting::None,
            max_level: 3,
            cooldown: 2.0,
            mana_cost: 30.0,
            cast_time: 0.0,
            channel: None,
            area_radius: 0.0,
            summon: None,
            badge: None,
            hotkey: None,
        },
    );
    app.insert_resource(defs);
    let e = spawn_unit(&mut app.world_mut(), &reg, "mage", Vec3::ZERO, Allegiance::Defender, 1.0)
        .unwrap();
    app.world_mut().entity_mut(e).insert(SkillCaster {
        slots: vec![SkillSlot { skill: "bolt".into(), level: 1, cooldown: 0.0, autocast_on: false }],
    });
    warm_up(&mut app);

    apply_status(&mut app, e, StatusKind::Silence, 1.0);
    app.update(); // silence applied
    cast(&mut app, e, 0);
    assert_eq!(count_over_frames::<SkillCast>(&mut app, 3), 0, "silenced cast refused");
    let cd = app.world().get::<SkillCaster>(e).unwrap().slots[0].cooldown;
    assert_eq!(cd, 0.0, "refused cast burns no cooldown");
    let mana = app.world().get::<Mana>(e).unwrap().0;
    assert!((mana - 100.0).abs() < 1e-5, "mana untouched: {mana}");

    // 1.0s silence expires (~60 frames) — the same cast then goes through.
    run(&mut app, 61);
    assert!(app.world().get::<Silenced>(e).is_none(), "silence expired");
    cast(&mut app, e, 0);
    assert_eq!(count_over_frames::<SkillCast>(&mut app, 2), 1, "casts again after expiry");
    let mana = app.world().get::<Mana>(e).unwrap().0;
    assert!((mana - 70.0).abs() < 1e-5, "post-expiry cast spends mana normally: {mana}");
}

// --------------------------------------------------------------------------
// (5) Summons
// --------------------------------------------------------------------------

/// Acceptance 5: a summon skill (count 2, lifetime 5.0s) spawns exactly 2
/// units of the summoned def on the caster's side and despawns them on the
/// 300th frame after the cast.
#[test]
fn summon_two_wolves_expire_after_300_frames() {
    let mut app = test_app();
    let wolf = udef("wolf", 30.0);
    let summoner = udef("summoner", 100.0);
    let reg = registry_of(vec![wolf, summoner]);
    app.insert_resource(UnitRegistry { defs: reg.defs.clone() });
    let mut defs = SkillDefs::default();
    defs.defs.insert(
        "wolfs".into(),
        SkillDef {
            id: "wolfs".into(),
            kind: SkillKind::Active,
            targeting: SkillTargeting::None,
            max_level: 1,
            cooldown: 1.0,
            mana_cost: 0.0,
            cast_time: 0.0,
            channel: None,
            area_radius: 0.0,
            summon: Some(SummonSpec { unit: "wolf".into(), count: 2, lifetime_secs: 5.0 }),
            badge: None,
            hotkey: None,
        },
    );
    app.insert_resource(defs);
    let e = spawn_unit(
        &mut app.world_mut(),
        &reg,
        "summoner",
        Vec3::ZERO,
        Allegiance::Defender,
        1.0,
    )
    .unwrap();
    app.world_mut().entity_mut(e).insert(SkillCaster {
        slots: vec![SkillSlot { skill: "wolfs".into(), level: 1, cooldown: 0.0, autocast_on: false }],
    });
    warm_up(&mut app);

    let count_wolves = |app: &mut App| -> usize {
        let mut q = app
            .world_mut()
            .query_filtered::<&UnitInstance, With<Summoned>>();
        q.iter(app.world())
            .filter(|i| i.def == "wolf")
            .count()
    };

    cast(&mut app, e, 0);
    app.update(); // the cast frame: SkillCast + 2 deferred spawns
    assert_eq!(count_wolves(&mut app), 2, "exactly two summoned entities");

    // Both on the caster's side, both leased.
    {
        let mut q = app
            .world_mut()
            .query_filtered::<(&UnitInstance, &Allegiance), With<Summoned>>();
        let sides: Vec<Allegiance> = q
            .iter(app.world())
            .filter(|(i, _)| i.def == "wolf")
            .map(|(_, s)| *s)
            .collect();
        assert_eq!(sides.len(), 2);
        assert!(sides.iter().all(|s| *s == Allegiance::Defender), "summons share the caster's side");
    }

    // Alive through 300 updates after the cast frame...
    run(&mut app, 299);
    assert_eq!(count_wolves(&mut app), 2, "alive through frame 300");

    // ...and despawned on the next one (5.0s lease, ticked in Cleanup's
    // despawn step).
    run(&mut app, 1);
    assert_eq!(count_wolves(&mut app), 0, "gone on frame 301 (5.0s = 300 frames)");
}

// --------------------------------------------------------------------------
// (6) Attack windup
// --------------------------------------------------------------------------

/// Acceptance 6a: windup 0.3s (18 frames). The victim dying on frame 17 —
/// one frame before the damage point — cancels the committed swing: no
/// damage ever, windup component removed.
#[test]
fn windup_hit_cancels_when_target_dies_on_frame_17() {
    let (mut app, s, v) = duel_app(10.0, 0.3, 0.0, 10000.0);
    let mut tally = DealtTally::new();
    assert_eq!(tally.over(&mut app, 17), 0, "windup mid-flight (damage would land on 18)");
    app.world_mut().get_mut::<Hp>(v).unwrap().0 = 0.0; // dies on frame 17
    run(&mut app, 1);
    assert!(
        app.world().get::<AttackWindup>(s).is_none(),
        "the committed swing is cancelled at the damage-point check"
    );
    assert_eq!(tally.over(&mut app, 10), 0, "no damage ever");
}

/// Acceptance 6b: same kit, living target — the hit lands exactly on
/// frame 18 (windup 0.3s; the start frame counts as the first).
#[test]
fn windup_damage_lands_on_frame_18() {
    let (mut app, s, _v) = duel_app(10.0, 0.3, 0.0, 10000.0);
    let mut tally = DealtTally::new();
    assert_eq!(tally.over(&mut app, 17), 0, "nothing before the damage point");
    assert!(app.world().get::<AttackWindup>(s).is_some(), "swing still committed");
    assert_eq!(tally.over(&mut app, 1), 1, "the hit lands on frame 18");
    assert!(app.world().get::<AttackWindup>(s).is_none(), "windup consumed");
    assert_eq!(tally.over(&mut app, 5), 0, "interval 10s: no follow-up");
}

/// Spec (stun x windup cross): a stun mid-windup cancels the committed
/// swing — no damage while stunned.
#[test]
fn stun_cancels_inflight_windup() {
    let (mut app, s, _v) = duel_app(10.0, 0.3, 0.0, 10000.0);
    run(&mut app, 5);
    assert!(app.world().get::<AttackWindup>(s).is_some());
    let mut tally = DealtTally::new();
    apply_status(&mut app, s, StatusKind::Stun, 1.0);
    run(&mut app, 1);
    assert!(app.world().get::<AttackWindup>(s).is_none(), "stun eats the swing");
    assert_eq!(tally.over(&mut app, 60), 0, "no damage during the stun window");
}

// --------------------------------------------------------------------------
// (7) Backswing cancel
// --------------------------------------------------------------------------

/// Acceptance 7: windup 0.3 + backswing 0.5 + interval 0.4. The first hit
/// settles at frame 18; a fresh Move order at frame 25 (mid-backswing)
/// releases the lock immediately, so the second hit lands at frame 59 —
/// earlier than the natural-lock control (frame 65) — while the unit
/// actually walks the order.
#[test]
fn backswing_cancelled_by_move_order_second_hit_early() {
    let (mut app, s, _v) = duel_app(0.4, 0.3, 0.5, 100000.0);
    app.world_mut().entity_mut(s).insert(MoveAgent {
        speed: 5.0,
        arrival_radius: 0.1,
        waypoints: vec![],
        velocity: Vec3::ZERO,
        body_radius: 0.3,
        order: None,
    }).insert(AgentOrderState::Idle);

    // Frames 1..24: first hit at the damage point (18), backswing running.
    let mut tally = DealtTally::new();
    assert_eq!(tally.over(&mut app, 24), 1, "first hit settled before the cancel order");
    assert!(app.world().get::<AttackLock>(s).is_some(), "backswing lock held");

    // Fresh order mid-backswing: lock released (the micro-retreat).
    app.world_mut()
        .resource_mut::<Messages<UnitCommand>>()
        .write(UnitCommand { order: Order::Move { to: Vec3::new(8.0, 0.0, 0.0) }, subjects: vec![s] });
    run(&mut app, 1);
    assert!(app.world().get::<AttackLock>(s).is_none(), "order cancels the backswing lock");

    // Cooldown (0.4s from frame 18) readies at 42; second windup 42..59.
    assert_eq!(tally.over(&mut app, 35), 1, "second hit lands at frame 59 (early unlock)");
    assert!(
        app.world().get::<Transform>(s).unwrap().translation.x > 0.0,
        "the unit walked the order (cancel is about moving)"
    );
}

/// Control for acceptance 7: without the cancel order the natural lock
/// defers the second hit past frame 60 (it lands on frame 65).
#[test]
fn backswing_natural_lock_defers_second_hit() {
    let (mut app, _s, _v) = duel_app(0.4, 0.3, 0.5, 100000.0);
    let mut tally = DealtTally::new();
    assert_eq!(tally.over(&mut app, 60), 1, "natural lock holds the second hit past frame 60");
    assert_eq!(tally.over(&mut app, 6), 1, "second hit lands on frame 65");
}

// --------------------------------------------------------------------------
// (8) Zero drift with defaults
// --------------------------------------------------------------------------

/// Acceptance 8: windup 0 / backswing 0 (every pre-SKVOC def value) is
/// byte-for-byte the legacy fire behavior — the hit lands the same frame
/// the cooldown readies, cadence follows `interval` exactly, and no attack
/// phase component ever exists.
#[test]
fn default_zero_windup_backswing_is_legacy_behavior() {
    let (mut app, s, _v) = duel_app(0.3, 0.0, 0.0, 100000.0);
    let mut tally = DealtTally::new();
    assert_eq!(tally.over(&mut app, 1), 1, "fires the same frame (no windup delay)");
    assert!(app.world().get::<AttackWindup>(s).is_none());
    assert!(app.world().get::<AttackLock>(s).is_none());
    // 0.3s interval: hits at frames 1, 19, 37, 55, 73, 91 — six per 100.
    assert_eq!(tally.over(&mut app, 99), 5, "exact legacy cadence");
    assert!(app.world().get::<AttackWindup>(s).is_none());
    assert!(app.world().get::<AttackLock>(s).is_none());
}

/// Sanity on the frame arithmetic the exact counts above rely on: DT is
/// the fixed step (1/60), and 0.3s is exactly 18 frames of it.
#[test]
fn frame_arithmetic() {
    assert!((DT - (1.0f32 / 60.0)).abs() < 1e-7);
    assert!((0.3f32 / DT - 18.0).abs() < 1e-3);
}
