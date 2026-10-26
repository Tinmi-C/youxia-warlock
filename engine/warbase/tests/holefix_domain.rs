//! Hole-fix acceptance (channel C->A upshot, ADR-0016 §11): the five
//! abstraction holes rts-lab documented while running the RL slice on the
//! untouched base, now fixed IN the base. Each test pins one acceptance
//! clause of the upshot spec:
//! ① hole-1: `OrthoTopDownPicker` hits the y=0 ground plane through a
//!    straight-down orthographic camera (the old z=0 math returned None
//!    for every pixel of that shape);
//! ② hole-2: left-click point select — hit -> SelectionState contains it,
//!    open ground -> Clear;
//! ③ hole-3/5: pathless bundles validate (empty waves + empty paths),
//!    build_grid centers on (0, 0), and path maps keep their exact
//!    origin + blocked set;
//! ④ hole-3: starting_forces spawn with Owner + derived Allegiance;
//! ⑤ hole-4: same-frame single-subject Move commands to one point scatter.

use bevy::camera::{RenderTargetInfo, Viewport};
use bevy::prelude::*;
use warbase::bundle::{
    assemble, build_grid, validate, BuildDomainDef, MapBundle, MapIdentity, StartingForce,
    WorldDef, MAP_FORMAT_VERSION,
};
use warbase::input::prelude::*;
use warbase::prelude::*;
use warbase::testing::test_app;

// -- fixtures (bx9a/sel patterns, rebuilt in-file) ---------------------------

fn def(id: &str, body_radius: f32) -> UnitDef {
    UnitDef {
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
        skills: vec![],
        upgrade_to: None,
        heal: None,
        mana: None,
    skill_levels: vec![],
    }
}

fn registry(defs: &[UnitDef]) -> UnitRegistry {
    UnitRegistry { defs: defs.iter().cloned().map(|d| (d.id.clone(), d)).collect() }
}

/// Bundle base for the bundle clauses: two defs, a 16x12 FreeGrid, empty
/// paths/waves, LastTeamStanding (no TD semantics needed).
fn pathless_bundle() -> MapBundle {
    let units = registry(&[def("peasant", 0.3), def("grunt", 0.35)]).defs;
    MapBundle {
        identity: MapIdentity { id: "holefix".into(), version: MAP_FORMAT_VERSION },
        world: WorldDef {
            paths: vec![],
            build: BuildDomainDef::FreeGrid { cell: 1.0, cols: 16, rows: 12 },
            blocked: vec![],
        },
        units,
        waves: WaveTable {
            waves: vec![],
            advance: Advance::Auto { intermission: 12.0 },
            early_call_bonus: None,
        },
        rules: MatchRules {
            leak: LeakPolicy::Stockpile { cap: 50 },
            win: WinCondition::LastTeamStanding,
            ..Default::default()
        },
        econ_start: EconStart { gold: 150, wood: 0, pop_cap: 12 },
        econ_rules: EconRules::default(),
        attack_armor: AttackArmorTable::default(),
        nav: Default::default(),
        vision: Default::default(),
        items: Default::default(),
        production: Default::default(),
        teams: Default::default(),
        starting_forces: Default::default(),
    }
}

// -- ① hole-1: ortho picker on the y=0 plane ---------------------------------

/// A headless app whose only camera is a straight-down orthographic view of
/// the ground (rts-lab's shape: above the origin, looking at it, up = -Z),
/// with a manually computed projection (no render backend needed — the same
/// fixture trick warden's pick_upcast tests use).
fn top_down_app() -> App {
    let mut app = test_app();
    let size = UVec2::new(1280, 720);
    let projection = Projection::Orthographic(OrthographicProjection {
        scaling_mode: bevy::camera::ScalingMode::Fixed { width: 20.0, height: 10.0 },
        ..OrthographicProjection::default_3d()
    });
    let tf = Transform::from_xyz(0.0, 40.0, 0.0).looking_at(Vec3::ZERO, Vec3::NEG_Z);
    let mut updated = projection.clone();
    updated.update(1280.0, 720.0);
    let camera = app
        .world_mut()
        .spawn((
            Camera {
                viewport: Some(Viewport { physical_size: size, ..default() }),
                ..default()
            },
            projection,
            tf,
            GlobalTransform::from(tf),
        ))
        .id();
    let mut cam = app.world_mut().get_mut::<Camera>(camera).unwrap();
    cam.computed.target_info =
        Some(RenderTargetInfo { physical_size: size, scale_factor: 1.0 });
    cam.computed.clip_from_view = updated.get_clip_from_view();
    app
}

/// UT-HOLE-01 (acceptance ①): the default picker resolves screen pixels to
/// ground (x, z) through the straight-down ortho camera — the screen center
/// lands on the origin the camera looks at, known ground points round-trip
/// through project -> pick, and off-viewport pixels map to None. (Pre-fix
/// the z=0 intersection divided by direction.z ~ 0 and returned None for
/// EVERY pixel of this camera shape.)
#[test]
fn ortho_picker_resolves_y0_plane_from_top_down_camera() {
    let mut app = top_down_app();
    let picker = OrthoTopDownPicker;
    let world = app.world_mut();

    // Screen center = the point the camera looks straight down at = origin
    // (within f32 projection epsilon; the exact-zero demand is not the
    // point — SOME hit at the center is: pre-fix this was None).
    let center = picker
        .screen_to_world(world, Vec2::new(640.0, 360.0))
        .expect("straight-down camera center picks the origin");
    assert!(center.length() < 1e-4, "center picked {center:?}, not the origin");

    // Round-trip: known ground points project to screen, and the picker maps
    // those screen points back to the same ground (x, z).
    let (cam, gt) = {
        let mut q = world.query::<(&Camera, &GlobalTransform)>();
        let (cam, gt) = q.single(world).expect("one camera");
        (cam.clone(), *gt)
    };
    for ground in [Vec3::new(0.0, 0.0, 0.0), Vec3::new(3.7, 0.0, -2.4), Vec3::new(-6.0, 0.0, 3.1)]
    {
        let screen = cam.world_to_viewport(&gt, ground).expect("ground point projects");
        let picked = picker
            .screen_to_world(world, screen)
            .expect("on-viewport screen point picks");
        assert!(
            (picked - Vec2::new(ground.x, ground.z)).length() < 1e-3,
            "screen {screen:?} of ground {ground:?} picked {picked:?}"
        );
    }

    // Off-viewport pixels never pick (viewport_to_world extrapolates; the
    // picker must bounds-check itself).
    assert_eq!(picker.screen_to_world(world, Vec2::new(-5.0, 100.0)), None);
    assert_eq!(picker.screen_to_world(world, Vec2::new(1300.0, 360.0)), None);
}

// -- ② hole-2: left-click point select ----------------------------------------

/// UT-HOLE-02 (acceptance ②): a left PickAttempt on a body puts it in
/// SelectionState; a left PickAttempt on open ground clears; shift+ground
/// keeps the selection. Point select lands one frame after the pick (the
/// select applier already ran this frame — see apply_point_selects).
#[test]
fn left_click_point_selects_and_ground_clears() {
    let mut app = test_app();
    let reg = registry(&[def("squad", 0.5)]);
    app.insert_resource(reg.clone());
    let unit = spawn_unit(
        &mut app.world_mut(),
        &reg,
        "squad",
        Vec3::new(2.0, 0.0, 3.0),
        Allegiance::Defender,
        1.0,
    )
    .unwrap();
    app.world_mut().entity_mut(unit).insert(Selectable);

    let left = |world: Vec2, shift: bool| PickAttempt {
        world,
        button: PickButton::Left,
        click_count: 1,
        ctrl: false,
        shift,
    };
    let settle = |app: &mut App| {
        // One update resolves the pick (writes SelectCommand), the next one
        // applies it (documented one-frame latency).
        app.update();
        app.update();
    };

    // Hit: the unit lands in the selection (and only it).
    app.world_mut().write_message(left(Vec2::new(2.0, 3.0), false));
    settle(&mut app);
    assert_eq!(app.world().resource::<SelectionState>().entities, vec![unit]);

    // Open ground: cleared.
    app.world_mut().write_message(left(Vec2::new(-8.0, -8.0), false));
    settle(&mut app);
    assert!(app.world().resource::<SelectionState>().entities.is_empty());

    // Shift+ground keeps the selection (shift never clears).
    app.world_mut().write_message(SelectCommand::Set(vec![unit]));
    app.update();
    app.world_mut().write_message(left(Vec2::new(-8.0, -8.0), true));
    settle(&mut app);
    assert_eq!(app.world().resource::<SelectionState>().entities, vec![unit]);
}

// -- ③ hole-3/5: pathless bundles + unchanged path maps ------------------------

/// UT-HOLE-03a (acceptance ③): a pathless bundle (empty waves + empty paths)
/// validates — RTS/skirmish maps are legal map content now. Waves that still
/// reference a path keep being rejected (the TD contract is untouched).
#[test]
fn pathless_bundle_validates_and_wave_refs_still_rejected() {
    let mut bundle = pathless_bundle();
    assert!(validate(&bundle).is_ok(), "empty paths + empty waves is a legal map");

    // A spawn referencing path 0 of an empty path list stays invalid.
    bundle.waves.waves.push(WaveEntry {
        spawns: vec![SpawnSpec {
            unit: "peasant".into(),
            count: 1,
            interval: 1.0,
            path: 0,
            delay: 0.0,
            hp_scale: 1.0,
        }],
        boss_timeout: None,
        preview: "1 peasant".into(),
        reward_gold: 0,
    });
    assert!(validate(&bundle).is_err(), "wave spawn with no paths is still rejected");
}

/// UT-HOLE-03b (acceptance ③): a pathless FreeGrid centers on (0, 0); a
/// path map's build grid is value-for-value what it always was (origin from
/// the path bbox, border ring blocked — the fog-sample geometry, pinned).
#[test]
fn build_grid_centers_pathless_on_origin_and_keeps_path_maps() {
    // Pathless: origin = -(cols*cell/2, rows*cell/2), nothing blocked.
    let grid = build_grid(&pathless_bundle().world);
    assert_eq!(grid.origin, Vec2::new(-8.0, -6.0));
    assert_eq!((grid.cols, grid.rows), (16, 12));
    assert!(grid.blocked.is_empty());

    // Path map (fog-sample shape): origin (-0.5, -0.5) and the exact 16x16
    // border ring of blocked cells — the values the pre-fix code produced.
    let mut bundle = pathless_bundle();
    bundle.world.paths = vec![PathDef::closed_loop(vec![
        [0.0, 0.0],
        [15.0, 0.0],
        [15.0, 15.0],
        [0.0, 15.0],
    ])];
    bundle.world.build = BuildDomainDef::FreeGrid { cell: 1.0, cols: 16, rows: 16 };
    let grid = build_grid(&bundle.world);
    assert_eq!(grid.origin, Vec2::new(-0.5, -0.5), "path bbox centering unchanged");
    let mut expected = std::collections::HashSet::new();
    for c in 0..16 {
        expected.insert(Cell(c, 0));
        expected.insert(Cell(c, 15));
        expected.insert(Cell(0, c));
        expected.insert(Cell(15, c));
    }
    let actual: std::collections::HashSet<_> = grid.blocked.iter().copied().collect();
    assert_eq!(actual, expected, "path-blocked cells value-for-value unchanged");
}

// -- ④ hole-3: starting_forces -------------------------------------------------

/// UT-HOLE-04 (acceptance ④): 2 peasants (owner 0) + 1 grunt (owner 1) in
/// the section -> exactly 3 units on the field after assemble, with the
/// right Owner markers, MP1-derived Allegiance, and ground-plane positions.
/// A bundle without the section spawns nothing (old maps, zero drift).
#[test]
fn starting_forces_spawn_owned_units_with_derived_allegiance() {
    let mut bundle = pathless_bundle();
    bundle.teams = TeamsDef {
        players: vec![(0, 0), (1, 1)],
        hostile_pairs: vec![(0, 1)],
    };
    bundle.starting_forces = vec![
        StartingForce { unit: "peasant".into(), count: 2, pos: [-3.5, 1.5], owner: Some(0) },
        StartingForce { unit: "grunt".into(), count: 1, pos: [9.5, -6.0], owner: Some(1) },
    ];

    let mut app = test_app();
    assemble(&mut app.world_mut(), &bundle);

    let mut q = app.world_mut().query::<(&UnitInstance, &Transform)>();
    let mut field: Vec<(String, Vec3)> =
        q.iter(app.world()).map(|(i, t)| (i.def.clone(), t.translation)).collect();
    field.sort_by(|a, b| a.0.cmp(&b.0));
    assert_eq!(field.len(), 3, "exactly the three authored units are on the field");
    assert_eq!(
        field[0],
        ("grunt".to_string(), Vec3::new(9.5, 0.0, -6.0)),
        "grunt at its authored ground (x, 0, z)"
    );
    assert_eq!(
        field[1],
        ("peasant".to_string(), Vec3::new(-3.5, 0.0, 1.5)),
        "peasants both spawn (sorted, one spot shared)"
    );
    assert_eq!(field[2], ("peasant".to_string(), Vec3::new(-3.5, 0.0, 1.5)));

    // Owner markers + MP1-derived sides: P0 -> team 0 -> Defender;
    // P1 -> team 1 (hostile to 0) -> Attacker.
    let mut owned = app.world_mut().query::<(&UnitInstance, &Owner, &Allegiance)>();
    let mut sides: Vec<(String, u8, Allegiance)> = owned
        .iter(app.world())
        .map(|(i, o, a)| (i.def.clone(), o.0 .0, *a))
        .collect();
    sides.sort_by(|a, b| (a.1, &a.0).cmp(&(b.1, &b.0)));
    assert_eq!(
        sides,
        vec![
            ("peasant".to_string(), 0, Allegiance::Defender),
            ("peasant".to_string(), 0, Allegiance::Defender),
            ("grunt".to_string(), 1, Allegiance::Attacker),
        ],
        "Owner verbatim from the section, Allegiance derived from diplomacy"
    );

    // Old maps (no section) assemble with zero starting spawns.
    let mut plain = test_app();
    assemble(&mut plain.world_mut(), &pathless_bundle());
    let count = plain
        .world_mut()
        .query::<&UnitInstance>()
        .iter(plain.world())
        .count();
    assert_eq!(count, 0, "no starting_forces section -> no spawns (zero drift)");
}
// -- ⑤ hole-4: same-frame single-subject scatter --------------------------------

fn spawn_agent(app: &mut App, pos: Vec3) -> Entity {
    app.world_mut()
        .spawn((
            MoveAgent {
                speed: 1.0,
                arrival_radius: 0.05,
                waypoints: Vec::new(),
                velocity: Vec3::ZERO,
                body_radius: 0.3,
                order: None,
            },
            AgentOrderState::Idle,
            Transform::from_translation(pos),
        ))
        .id()
}

/// UT-HOLE-05 (acceptance ⑤): four INDEPENDENT single-subject Move commands
/// to the same point, all written in one frame, scatter through MOV5 slots —
/// every pair of first waypoints is >= 0.9 * (0.3 + 0.3) apart, and the
/// smallest-index subject keeps the exact command point (slot 0).
#[test]
fn same_frame_single_subject_moves_scatter() {
    let mut app = test_app();
    let mut units: Vec<Entity> = (0..4).map(|i| spawn_agent(&mut app, Vec3::new(i as f32 * 5.0, 0.0, 0.0))).collect();
    units.sort_by_key(|e| e.index_u32());
    app.update(); // warm-up frame (ManualDuration dt = 0 on the first)

    let target = Vec3::new(10.0, 0.0, 5.0);
    for &u in &units {
        app.world_mut().write_message(UnitCommand {
            order: Order::Move { to: target },
            subjects: vec![u],
        });
    }
    app.update(); // one frame consumes all four commands

    let firsts: Vec<Vec3> = units
        .iter()
        .map(|&e| app.world().get::<MoveAgent>(e).unwrap().waypoints[0])
        .collect();
    assert_eq!(firsts[0], target, "slot 0 (smallest index) IS the command point");
    for i in 0..4 {
        for j in (i + 1)..4 {
            let d = firsts[i].distance(firsts[j]);
            assert!(d >= 0.9 * (0.3 + 0.3), "waypoints ({i},{j}) only {d} apart");
        }
    }
}
