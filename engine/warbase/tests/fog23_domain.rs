//! FOG2/FOG3 acceptance (ADR-0016 §9): InFog marking of units, shared vision
//! per observer team (`VisionPerspective`), the D5 combat/fog separation
//! (towers fire into fog), the manual-cast point visibility gate, and the
//! minimap data layer — plus the zero-drift contract for maps without fog.
//!
//! Same geometry trick as fog_domain.rs (rebuilt in-file, no cross-file
//! helpers): the sample map's FreeGrid origin lands on (-0.5, -0.5), so
//! every cell (i, j) CENTER sits on the integer world point (i, j) and all
//! distances below are exact.

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use warbase::bundle::{
    assemble, BuildDomainDef, MapBundle, MapIdentity, WorldDef, MAP_FORMAT_VERSION,
};
use warbase::prelude::*;
use warbase::testing::{count_over_frames, test_app, STEP};

/// Minimal valid bundle (bx9a pattern): 16x16/1.0 FreeGrid => integer cell
/// centers (see module doc). Wave spawn delayed out of every test.
fn sample_bundle() -> MapBundle {
    let mut units = std::collections::HashMap::new();
    units.insert(
        "grunt".to_string(),
        UnitDef {
            leak: 1,
            id: "grunt".into(),
            attrs: BaseAttributes { hp: 50.0, move_speed: 0.0, body_radius: 0.3 },
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
        },
    );
    MapBundle {
        identity: MapIdentity { id: "fog23-sample".into(), version: MAP_FORMAT_VERSION },
        world: WorldDef {
            paths: vec![PathDef::closed_loop(vec![
                [0.0, 0.0],
                [15.0, 0.0],
                [15.0, 15.0],
                [0.0, 15.0],
            ])],
            build: BuildDomainDef::FreeGrid { cell: 1.0, cols: 16, rows: 16 },
            blocked: vec![],
        },
        units,
        waves: WaveTable {
            waves: vec![WaveEntry {
                spawns: vec![SpawnSpec {
                    unit: "grunt".into(),
                    count: 1,
                    interval: 1.0,
                    path: 0,
                    delay: 600.0, // keep the director out of these short tests
                    hp_scale: 1.0,
                }],
                boss_timeout: None,
                preview: "1 grunt".into(),
                reward_gold: 0,
            }],
            advance: Advance::Auto { intermission: 600.0 },
            early_call_bonus: None,
        },
        rules: MatchRules {
            leak: LeakPolicy::Stockpile { cap: 10 },
            win: WinCondition::SurviveWaves { waves: 1 },
            ..Default::default()
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

/// Unit def with explicit hp / hero flag (grunt shape: no attack, no mana).
fn grunt_def(id: &str, hp: f32, hero: bool) -> UnitDef {
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
        hero,
        skills: vec![],
        upgrade_to: None,
        heal: None,
        mana: None,
    skill_levels: vec![],
    }
}

/// Tower def: short vision is inserted by the test; the attack reaches far
/// past it (the D5 construct — range must outrun the tower's own sight).
fn tower_def(id: &str) -> UnitDef {
    UnitDef {
        leak: 0,
        id: id.into(),
        attrs: BaseAttributes { hp: 100.0, move_speed: 0.0, body_radius: 0.5 },
        attack: Some(AttackDef {
            damage: 10.0,
            interval: 0.0, // fires every frame: exact hit counts below
            range: 8.0,
            attack_type: AttackType::Normal,
            targets: TargetDomain::Ground,
            projectile: None,
            windup_secs: 0.0, backswing_secs: 0.0,
        }),
        armor_type: ArmorType::Fortified,
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

/// Register defs and hand the same registry back for direct spawns.
fn install_registry(app: &mut App, defs: Vec<UnitDef>) -> UnitRegistry {
    let reg = UnitRegistry { defs: defs.into_iter().map(|d| (d.id.clone(), d)).collect() };
    app.insert_resource(UnitRegistry { defs: reg.defs.clone() });
    reg
}

fn spawn_at(app: &mut App, reg: &UnitRegistry, id: &str, x: f32, z: f32, side: Allegiance) -> Entity {
    spawn_unit(&mut app.world_mut(), reg, id, Vec3::new(x, 0.0, z), side, 1.0).unwrap()
}

/// Bare vision source (no Owner, no Allegiance => home-team scenery, the
/// FOG1-compatible carrier; see vision::source_team).
fn spawn_bare_source(app: &mut App, x: f32, z: f32, radius: f32) -> Entity {
    app.world_mut()
        .spawn((VisionSource { radius }, Transform::from_translation(Vec3::new(x, 0.0, z))))
        .id()
}

/// UT-FOG2-01: a unit on a visible cell carries no InFog (a stale mark is
/// REMOVED); the source marching away darkens the cell and the mark appears.
#[test]
fn infog_tracks_the_visible_cell() {
    let mut app = test_app();
    assemble(&mut app.world_mut(), &sample_bundle());
    let reg = install_registry(&mut app, vec![grunt_def("grunt", 100.0, false)]);
    let src = spawn_bare_source(&mut app, 0.0, 0.0, 5.0);
    let creep = spawn_at(&mut app, &reg, "grunt", 3.0, 0.0, Allegiance::Attacker);
    // Stale mark: the visible branch must wipe it, not just skip inserting.
    app.world_mut().entity_mut(creep).insert(InFog);

    app.update();
    assert!(
        app.world().get::<InFog>(creep).is_none(),
        "cell (3,0) center at d=3 <= 5: visible, stale mark removed"
    );

    // March the source away: cell (3,0) goes dark (d ~ 15 > 5) while the
    // grid stays lit around the source's new home.
    *app.world_mut()
        .entity_mut(src)
        .get_mut::<Transform>()
        .unwrap() = Transform::from_translation(Vec3::new(12.0, 0.0, 12.0));
    app.update();
    assert!(
        app.world().get::<InFog>(creep).is_some(),
        "source moved away: the mark appears"
    );
}

/// UT-FOG2-02: a perspective-team tower (VisionSource carrier) never marks
/// InFog — its own source keeps its own cell lit — while dark-map units do.
#[test]
fn vision_source_tower_never_marked_in_fog() {
    let mut app = test_app();
    assemble(&mut app.world_mut(), &sample_bundle());
    let reg = install_registry(
        &mut app,
        vec![grunt_def("tower", 100.0, false), grunt_def("grunt", 100.0, false)],
    );
    let tower = spawn_at(&mut app, &reg, "tower", 0.0, 0.0, Allegiance::Defender);
    app.world_mut().entity_mut(tower).insert(VisionSource { radius: 5.0 });
    let near = spawn_at(&mut app, &reg, "grunt", 3.0, 0.0, Allegiance::Attacker);
    let far = spawn_at(&mut app, &reg, "grunt", 12.0, 12.0, Allegiance::Attacker);

    for _ in 0..3 {
        app.update();
        assert!(app.world().get::<InFog>(tower).is_none(), "tower lights its own cell");
        assert!(app.world().get::<InFog>(near).is_none(), "d=3 <= 5: lit");
    }
    assert!(app.world().get::<InFog>(far).is_some(), "d ~ 17 > 5: dark");
}

/// UT-FOG2-03 (the D5 rule, explicit): a tower whose attack range outruns
/// its own vision acquires and shoots a creep standing in a DARK cell —
/// combat never consults the player's fog; DamageDealt flows as usual.
#[test]
fn d5_tower_fires_at_fog_hidden_creep() {
    let mut app = test_app();
    assemble(&mut app.world_mut(), &sample_bundle());
    let reg = install_registry(
        &mut app,
        vec![tower_def("tower"), grunt_def("grunt", 500.0, false)],
    );
    let tower = spawn_at(&mut app, &reg, "tower", 0.0, 0.0, Allegiance::Defender);
    // Vision shorter than the attack range (5 world units of dark room at
    // range 8): the creep at d=6 is in range but out of sight.
    app.world_mut().entity_mut(tower).insert(VisionSource { radius: 1.5 });
    let creep = spawn_at(&mut app, &reg, "grunt", 6.0, 0.0, Allegiance::Attacker);

    app.update(); // warm-up (first-frame dt=0 quirk) — fires once already

    let dealt = count_over_frames::<DamageDealt>(&mut app, 3);
    // 4 = the warm-up hit (a fresh MessageCursor still sees the previous
    // frame's messages in the 2-frame buffer) + one hit per counted frame.
    assert_eq!(dealt, 4, "fog is irrelevant to tower fire (D5)");
    assert_eq!(
        app.world().get::<CurrentTarget>(tower).unwrap().0,
        Some(creep),
        "targeting never consults fog"
    );
    assert!(
        app.world().get::<InFog>(creep).is_some(),
        "cell (6,0) at d=6 > tower r=1.5: dark for the observer"
    );
    assert_eq!(
        app.world().get::<Hp>(creep).unwrap().0,
        460.0,
        "warm frame + 3 counted frames of 10 damage (Light x Normal = 1.0)"
    );
}

/// UT-FOG2-04: shared vision per observer seat — enemy-team sources (an
/// unowned Attacker carrier mapping to the environment team, and an owned
/// enemy whose Owner beats a stale Allegiance marker) light nothing for
/// team 0; flipping the seat relights around them and darkens the home
/// source. The flip also pins the perspective riding in the grid
/// fingerprint (no source moved, yet the grid recomputed).
#[test]
fn perspective_filters_enemy_team_sources() {
    let mut app = test_app();
    assemble(&mut app.world_mut(), &sample_bundle());
    let dip = MatchDiplomacy {
        players: vec![(PlayerId(0), TeamId(0)), (PlayerId(1), TeamId(1))],
        hostile_pairs: vec![(TeamId(0), TeamId(1))],
    };
    app.insert_resource(dip.clone());
    app.insert_resource(Hostility::from(&dip));

    let _home = spawn_bare_source(&mut app, 0.0, 0.0, 5.0); // no markers => team 0
    app.world_mut().spawn((
        VisionSource { radius: 5.0 },
        Transform::from_translation(Vec3::new(8.0, 0.0, 0.0)),
        Allegiance::Attacker, // unowned attacker => environment team (1 here)
    ));
    app.world_mut().spawn((
        VisionSource { radius: 5.0 },
        Transform::from_translation(Vec3::new(8.0, 0.0, 8.0)),
        Owner(PlayerId(1)), // owner's team (1) wins over the stale marker below
        Allegiance::Defender,
    ));

    app.update();
    let grid = app.world().resource::<VisionGrid>().clone();
    assert!(grid.visible.contains(&(0, 0)), "home-team source lights its cell");
    assert!(
        !grid.visible.contains(&(8, 0)),
        "environment-team source must not light the team-0 grid"
    );
    assert!(
        !grid.visible.contains(&(8, 8)),
        "owned enemy source must not either"
    );

    // Flip the observer seat.
    app.insert_resource(VisionPerspective(TeamId(1)));
    app.update();
    let grid = app.world().resource::<VisionGrid>().clone();
    assert!(grid.visible.contains(&(8, 0)), "enemy seat: attacker source lights up");
    assert!(grid.visible.contains(&(8, 8)), "enemy seat: owned team-1 source lights up");
    assert!(
        !grid.visible.contains(&(0, 0)),
        "enemy seat: home-team source goes dark"
    );
}

/// UT-FOG3-05: the minimap model carries exactly the visible units as
/// blips, in Entity order, with side/hero flags and the grid-derived map
/// bounds; dark units leave no blip.
#[test]
fn minimap_blips_mirror_visible_units_in_entity_order() {
    let mut app = test_app();
    assemble(&mut app.world_mut(), &sample_bundle());
    let reg = install_registry(
        &mut app,
        vec![grunt_def("grunt", 100.0, false), grunt_def("hero", 100.0, true)],
    );
    spawn_bare_source(&mut app, 0.0, 0.0, 5.0);
    let _u1 = spawn_at(&mut app, &reg, "grunt", 1.0, 0.0, Allegiance::Defender);
    let _u2 = spawn_at(&mut app, &reg, "grunt", 2.0, 0.0, Allegiance::Attacker);
    let _u3 = spawn_at(&mut app, &reg, "grunt", 12.0, 12.0, Allegiance::Attacker); // dark
    let _u4 = spawn_at(&mut app, &reg, "hero", 1.0, 1.0, Allegiance::Defender);

    app.update();
    let model = app.world().resource::<MinimapModel>();
    assert_eq!(model.blips.len(), 3, "three visible units, the dark one projects nothing");
    assert_eq!(
        model.blips[0],
        MinimapBlip { pos: Vec2::new(1.0, 0.0), side: Allegiance::Defender, is_hero: false }
    );
    assert_eq!(
        model.blips[1],
        MinimapBlip { pos: Vec2::new(2.0, 0.0), side: Allegiance::Attacker, is_hero: false },
        "visible enemies blip too (side flag tells the tint)"
    );
    assert_eq!(
        model.blips[2],
        MinimapBlip { pos: Vec2::new(1.0, 1.0), side: Allegiance::Defender, is_hero: true }
    );
    assert_eq!(
        model.map_bounds,
        Rect { min: Vec2::new(-0.5, -0.5), max: Vec2::new(15.5, 15.5) },
        "bounds = the vision grid's world extent"
    );
}

/// UT-FOG2/3-06 (zero drift): maps without fog in play never see a mark, a
/// blip, or a refused cast — both flavors: no grid geometry at all (warden's
/// FixedSlots) and a valid grid with nothing lit (loop-td before any
/// VisionSource carrier). Stale marks are wiped.
#[test]
fn maps_without_fog_stay_zero_drift() {
    // Flavor A: FixedSlots — no build extent, degenerate grid.
    let mut bundle = sample_bundle();
    bundle.world.build =
        BuildDomainDef::FixedSlots { slots: vec![[0.0, 0.0], [4.0, 0.0], [8.0, 0.0]] };
    let mut app = test_app();
    assemble(&mut app.world_mut(), &bundle);
    let reg = install_registry(&mut app, vec![grunt_def("grunt", 100.0, false)]);
    let _ = spawn_bare_source(&mut app, 4.0, 0.0, 5.0); // lights nothing: no cells
    let u1 = spawn_at(&mut app, &reg, "grunt", 0.0, 0.0, Allegiance::Attacker);
    let u2 = spawn_at(&mut app, &reg, "grunt", 4.0, 0.0, Allegiance::Defender);
    app.world_mut().entity_mut(u1).insert(InFog); // stale mark must be wiped
    for _ in 0..3 {
        app.update();
    }
    assert!(app.world().get::<InFog>(u1).is_none(), "degenerate grid: no fog ever");
    assert!(app.world().get::<InFog>(u2).is_none());
    assert!(app.world().resource::<MinimapModel>().blips.is_empty());
    let grid = app.world().resource::<VisionGrid>().clone();
    assert!(
        point_visible(&grid, Vec2::new(99.0, -99.0)),
        "degenerate grid: the cast gate passes everything through"
    );

    // Flavor B: valid grid geometry but zero lit cells (loop-td today).
    let mut app = test_app();
    assemble(&mut app.world_mut(), &sample_bundle());
    let u3 = spawn_at(&mut app, &reg, "grunt", 0.0, 0.0, Allegiance::Defender);
    app.world_mut().entity_mut(u3).insert(InFog); // stale mark must be wiped
    app.update();
    assert!(
        app.world().get::<InFog>(u3).is_none(),
        "unlit grid: fog inactive, stale mark wiped"
    );
    let model = app.world().resource::<MinimapModel>();
    assert!(model.blips.is_empty());
    assert_eq!(
        model.map_bounds,
        Rect { min: Vec2::new(-0.5, -0.5), max: Vec2::new(15.5, 15.5) },
        "bounds reflect map geometry even with fog inactive"
    );
}

/// UT-FOG2-07: the manual-cast visibility gate (D5 interaction layer). A
/// Point-targeted cast completing over a DARK cell is refused — no
/// SkillCast, no cooldown burn, Casting dropped — while a visible point
/// fires with the ground point passed through. Points only exist via
/// `Casting.point` (game aiming layers); the base's own request path ships
/// none, so this test injects the aiming layer's output directly.
#[test]
fn point_cast_into_dark_cell_is_refused() {
    let mut app = test_app();
    assemble(&mut app.world_mut(), &sample_bundle());
    let reg = install_registry(&mut app, vec![grunt_def("caster", 100.0, false)]);
    let mut defs = SkillDefs::default();
    defs.defs.insert(
        "blink".into(),
        SkillDef {
            id: "blink".into(),
            kind: SkillKind::Active,
            targeting: SkillTargeting::Point,
            max_level: 3,
            cooldown: 5.0,
            mana_cost: 0.0,
            cast_time: 1.0,
            channel: None,
            area_radius: 0.0, summon: None,
            badge: None,
            hotkey: None,
        },
    );
    app.insert_resource(defs);
    spawn_bare_source(&mut app, 0.0, 0.0, 2.0); // fog active near the origin
    let caster = spawn_at(&mut app, &reg, "caster", 0.0, 0.0, Allegiance::Defender);
    app.world_mut().entity_mut(caster).insert(SkillCaster {
        slots: vec![SkillSlot {
            skill: "blink".into(),
            level: 1,
            cooldown: 0.0,
            autocast_on: false,
        }],
    });
    app.update(); // warm-up frame (first-frame dt=0 quirk)

    // Direct probe pins: (1,0) lit at d=1 <= 2, (10,0) dark at d=10 > 2.
    let grid = app.world().resource::<VisionGrid>().clone();
    assert!(point_visible(&grid, Vec2::new(1.0, 0.0)));
    assert!(!point_visible(&grid, Vec2::new(10.0, 0.0)));

    // Dark point: refused at completion.
    app.world_mut().entity_mut(caster).insert(Casting {
        skill: "blink".into(),
        elapsed: STEP as f32 * 59.0, // one frame short of the 1.0s total
        total: 1.0,
        target: None,
        point: Some(Vec3::new(10.0, 0.0, 0.0)),
    });
    let mut cursor = MessageCursor::<SkillCast>::default();
    app.update();
    {
        let msgs = app.world().resource::<Messages<SkillCast>>();
        assert_eq!(cursor.read(msgs).count(), 0, "dark point: refused, no SkillCast");
    }
    assert!(app.world().get::<Casting>(caster).is_none(), "refused cast drops its Casting");
    assert_eq!(
        app.world().get::<SkillCaster>(caster).unwrap().slots[0].cooldown,
        0.0,
        "refused cast burns no cooldown"
    );

    // Visible point: fires with the point passed through untouched.
    app.world_mut().entity_mut(caster).insert(Casting {
        skill: "blink".into(),
        elapsed: STEP as f32 * 59.0,
        total: 1.0,
        target: None,
        point: Some(Vec3::new(1.0, 0.0, 0.0)),
    });
    app.update();
    {
        let msgs = app.world().resource::<Messages<SkillCast>>();
        let fired: Vec<_> = cursor.read(msgs).collect();
        assert_eq!(fired.len(), 1, "visible point: exactly one SkillCast");
        assert_eq!(fired[0].point, Some(Vec3::new(1.0, 0.0, 0.0)));
    }
    assert_eq!(
        app.world().get::<SkillCaster>(caster).unwrap().slots[0].cooldown,
        5.0,
        "accepted cast starts its cooldown"
    );
}
