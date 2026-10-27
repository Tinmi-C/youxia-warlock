//! HIGHLAND acceptance (round-2 plan §4 line 1): authored high ground
//! occludes the VisionGrid itself (physical line of sight, WC3 cliff
//! semantics) and grants a high->low attack bonus at the damage
//! computation point; D5 stays untouched (tower auto-targeting ignores
//! fog AND occlusion — range is the only input).
//!
//! Same geometry trick as fog23_domain.rs: the 16x16/1.0 FreeGrid over
//! the 0..15 path loop lands the origin on (-0.5, -0.5), so every cell
//! (i, j) CENTER sits on the integer world point (i, j) and all
//! distances below are exact.

use bevy::prelude::*;
use warbase::bundle::{assemble, BuildDomainDef, MapBundle, MapIdentity, WorldDef, MAP_FORMAT_VERSION};
use warbase::prelude::*;
use warbase::testing::{count_over_frames, test_app};

/// Minimal valid bundle; waves spawn nothing inside any test window
/// (delay 600s; the Auto intermission is just as long).
fn sample_bundle() -> MapBundle {
    let mut units = std::collections::HashMap::new();
    units.insert("grunt".to_string(), grunt_def("grunt", 100.0));
    MapBundle {
        identity: MapIdentity { id: "highsave-sample".into(), version: MAP_FORMAT_VERSION },
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
                    delay: 600.0, // keep the director quiet in these tests
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

/// Walker grunt (no attack). `move_speed` > 0 so path followers advance.
fn grunt_def(id: &str, hp: f32) -> UnitDef {
    UnitDef {
        hero_stats: None,
        leak: 1,
        id: id.into(),
        attrs: BaseAttributes { hp, move_speed: 3.0, body_radius: 0.3 },
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

/// Tower def: fires every frame (interval 0) unless `interval` is given;
/// range 8 outruns every vision radius used below (the D5 construct).
fn tower_def(id: &str, interval: f32) -> UnitDef {
    UnitDef {
        hero_stats: None,
        leak: 0,
        id: id.into(),
        attrs: BaseAttributes { hp: 100.0, move_speed: 0.0, body_radius: 0.5 },
        attack: Some(AttackDef {
            damage: 10.0,
            interval,
            range: 8.0,
            attack_type: AttackType::Normal,
            targets: TargetDomain::Ground,
            projectile: None,
            windup_secs: 0.0,
            backswing_secs: 0.0,
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

/// Bare vision source (no markers => home-team scenery; fog23 pattern).
fn spawn_bare_source(app: &mut App, x: f32, z: f32, radius: f32) -> Entity {
    app.world_mut()
        .spawn((VisionSource { radius }, Transform::from_translation(Vec3::new(x, 0.0, z))))
        .id()
}

// ---------------------------------------------------------------------------
// HIGHLAND: occlusion
// ---------------------------------------------------------------------------

/// UT-HS-01: a LOW viewer cannot light high ground — the destination high
/// cell itself blocks (cliff face), and cells behind the ridge stay dark —
/// while a viewer ON the high ground lights everything around it below and
/// beside it. Occlusion directionality, pinned on the VisionGrid lighting
/// itself (physical LOS, not a player-fog rule).
#[test]
fn highland_occlusion_is_directional() {
    let mut bundle = sample_bundle();
    bundle.terrain = TerrainDef { high_cells: vec![[4, 0]], attack_bonus: 0.0 };
    let mut app = test_app();
    assemble(&mut app.world_mut(), &bundle);
    let _low = spawn_bare_source(&mut app, 0.0, 0.0, 6.0); // low viewer, r=6

    app.update();
    let grid = app.world().resource::<VisionGrid>().clone();
    assert!(grid.visible.contains(&(3, 0)), "plain low cell in radius: lit");
    assert!(
        !grid.visible.contains(&(4, 0)),
        "low viewer cannot light a high cell at distance 4 (target itself blocks)"
    );
    assert!(
        !grid.visible.contains(&(5, 0)),
        "cells BEHIND the high cell stay dark (ridge shadow)"
    );
    assert!(
        !grid.visible.contains(&(6, 0)),
        "the shadow runs the whole ray: (6,0)'s line crosses (4,0)"
    );
    assert!(
        !grid.explored.contains(&(4, 0)),
        "unlit cells leave no explored memory"
    );

    // The direction flips: a viewer standing ON the high ground lights its
    // own cell and looks DOWN without any occlusion (same-height + below).
    let _high = spawn_bare_source(&mut app, 4.0, 0.0, 6.0);
    app.update();
    let grid = app.world().resource::<VisionGrid>().clone();
    assert!(grid.visible.contains(&(4, 0)), "high viewer lights its own cell");
    assert!(grid.visible.contains(&(5, 0)), "high sees low (one ridge cell down)");
    assert!(grid.visible.contains(&(6, 0)), "high sees far below, no occlusion");
    assert!(grid.explored.contains(&(4, 0)), "now lit, now remembered");
}

/// UT-HS-02: the high-ground attack bonus is exact at the damage
/// computation point — high hits low at exactly x1.25 (bonus 0.25), while
/// low hits high, same-height duels, and FLAT maps (empty TerrainDef) all
/// pay plain damage (zero drift for every existing map).
#[test]
fn high_ground_attack_bonus_is_exact() {
    /// Run tower-vs-grunt for warm-up + 3 counted frames (4 hits, fog23
    /// counting convention) and return the grunt's remaining hp.
    fn shootout(terrain: TerrainDef, tower: [f32; 2], grunt: [f32; 2]) -> f32 {
        let mut bundle = sample_bundle();
        bundle.terrain = terrain;
        let mut app = test_app();
        assemble(&mut app.world_mut(), &bundle);
        let reg = install_registry(
            &mut app,
            vec![tower_def("tower", 0.0), grunt_def("grunt", 500.0)],
        );
        let _tower = spawn_at(&mut app, &reg, "tower", tower[0], tower[1], Allegiance::Defender);
        let grunt = spawn_at(&mut app, &reg, "grunt", grunt[0], grunt[1], Allegiance::Attacker);
        app.update(); // warm-up frame (fires once)
        let _ = count_over_frames::<DamageDealt>(&mut app, 3);
        app.world().get::<Hp>(grunt).unwrap().0
    }

    let cliff = TerrainDef { high_cells: vec![[0, 0]], attack_bonus: 0.25 };
    assert_eq!(
        shootout(cliff.clone(), [0.0, 0.0], [2.0, 0.0]),
        450.0,
        "high hits low: exactly x1.25 (4 hits x 12.5)"
    );
    assert_eq!(
        shootout(cliff, [2.0, 0.0], [0.0, 0.0]),
        460.0,
        "low hits high: no bonus (one-way)"
    );
    assert_eq!(
        shootout(TerrainDef { high_cells: vec![[0, 0], [2, 0]], attack_bonus: 0.25 }, [0.0, 0.0], [2.0, 0.0]),
        460.0,
        "same height duel: no bonus"
    );
    assert_eq!(
        shootout(TerrainDef::default(), [0.0, 0.0], [2.0, 0.0]),
        460.0,
        "flat map (empty TerrainDef): zero drift, plain damage"
    );
}

/// UT-HS-03 (D5, explicit for HIGHLAND): occlusion is PHYSICS — it changes
/// what the grid lights (a low tower cannot SEE a creep standing on the
/// cliff: dark cell, InFog mark) — but tower AUTO-TARGETING never consults
/// it: the same tower acquires and shoots the dark high-ground creep on
/// range alone, DamageDealt flowing exactly as on flat ground.
#[test]
fn d5_targeting_ignores_highland_occlusion() {
    let mut bundle = sample_bundle();
    bundle.terrain = TerrainDef { high_cells: vec![[4, 0]], attack_bonus: 0.25 };
    let mut app = test_app();
    assemble(&mut app.world_mut(), &bundle);
    let reg = install_registry(
        &mut app,
        vec![tower_def("tower", 0.0), grunt_def("grunt", 500.0)],
    );
    let tower = spawn_at(&mut app, &reg, "tower", 0.0, 0.0, Allegiance::Defender);
    app.world_mut().entity_mut(tower).insert(VisionSource { radius: 6.0 });
    let creep = spawn_at(&mut app, &reg, "grunt", 4.0, 0.0, Allegiance::Attacker);

    app.update(); // warm-up (fires once)

    // Physical LOS: the creep's cell is dark and the creep is fog-marked.
    let grid = app.world().resource::<VisionGrid>().clone();
    assert!(
        !grid.visible.contains(&(4, 0)),
        "low tower cannot light the high cell the creep stands on"
    );
    assert!(app.world().get::<InFog>(creep).is_some(), "dark cell => InFog mark");

    // D5: combat is untouched by occlusion — range is the only input.
    let dealt = count_over_frames::<DamageDealt>(&mut app, 3);
    assert_eq!(dealt, 4, "tower fires into the occluded dark (D5)");
    assert_eq!(
        app.world().get::<CurrentTarget>(tower).unwrap().0,
        Some(creep),
        "targeting never consults terrain occlusion"
    );
    assert_eq!(
        app.world().get::<Hp>(creep).unwrap().0,
        460.0,
        "low attacker => no bonus even against a dark high target (4 x 10)"
    );
}
