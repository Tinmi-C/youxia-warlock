//! Headless balance readout tool (team 「设计目标锚点框架」methodology).
//!
//! This is NOT a pass/fail test of balance yet — it is the *observation
//! channel*: given a sane auto-build policy, it simulates a real headless
//! run and reports per-wave clear time / leaks / gold gain, plus the ending
//! base HP, gold, and win/lose. The AI (or human) reads these numbers against
//! the §14/§15 target ranges and converges the starting values. Run with:
//!
//! `cargo test --test balance -- --nocapture`
//!
//! Assertions here only guard the *harness*; balance-range assertions are
//! added once the design owner pins the anchor targets (BAL2).

use std::time::Duration;

use bevy::camera::{RenderTargetInfo, Viewport};
use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use bevy::{state::app::StatesPlugin, time::TimeUpdateStrategy};

use warden::components::{Commander, Enemy, SkillCaster, StatMods, Tower, TowerKind, TowerSlot};
use warden::plugins::{
    fx::FxPlugin,
    acquisition::AcquisitionPlugin, economy::EconomyPlugin, enemies::EnemiesPlugin,
    game::GamePlugin, map::MapPlugin, meta::MetaPlugin, projectiles::ProjectilesPlugin,
    skills::SkillsPlugin, towers::TowersPlugin, ui::UiPlugin, waves::WavesPlugin,
};
use warden::resources::{
    BaseHp, Economy, FusionDefs, MetaSavePath, RunRng, SkillAiming, TowerDefs, WaveChoice,
    WavePhase, WaveState,
};
use warden::states::GameState;
use warden::systems::archer_skills::{
    archer_skill_caster, SKILL_FIRE_ARROWS, SKILL_MULTISHOT,
};
use warden::systems::damage::{DamageDealt, DamageOrigin};
use warden::systems::fusion_skills::SKILL_DEADEYE;
use warden::systems::skills::{CastSkill, SKILL_METEOR_RAIN, SKILL_WAR_FRENZY};

fn test_app() -> App {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, StatesPlugin))
        .init_state::<GameState>()
        .add_plugins((
            FxPlugin,
            GamePlugin,
            MapPlugin,
            TowersPlugin,
            EnemiesPlugin,
            WavesPlugin,
            EconomyPlugin,
            AcquisitionPlugin,
            MetaPlugin,
            ProjectilesPlugin,
            SkillsPlugin,
            UiPlugin,
        ))
        .insert_resource(MetaSavePath(
            std::env::temp_dir().join("warden_meta_balance_test.txt"),
        ))
        .insert_resource(RunRng::seeded(1234))
        // Headless: no winit / asset plugins, so create the resources the
        // systems under test need (input, mesh/material asset stores).
        .init_resource::<ButtonInput<KeyCode>>()
        .init_resource::<ButtonInput<bevy::input::mouse::MouseButton>>()
        .init_resource::<Assets<Mesh>>()
        .init_resource::<Assets<StandardMaterial>>()
        // ART11: `start_enemy_walk` takes Res<Assets<Gltf>> +
        // ResMut<Assets<AnimationGraph>> (normally provided by the gltf /
        // animation plugins, absent headless). init_resource only —
        // init_asset would demand an AssetServer, which MinimalPlugins
        // does not provide.
        .init_resource::<Assets<bevy::gltf::Gltf>>()
        .init_resource::<Assets<AnimationGraph>>()
        .init_resource::<Assets<AnimationClip>>()
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(
            1.0 / 60.0,
        )));
    app
}

/// One cleared wave's readout.
#[derive(Clone, Copy)]
struct WaveSample {
    index: u32,
    /// Game seconds from wave start to resolve (killed + leaked).
    clear_secs: f32,
    /// Base HP lost (enemies that leaked) this wave.
    leaks: u32,
    /// Gold gained during the wave (kill gold + wave reward).
    gold_gain: u32,
}

/// Slot world positions (from the map subsystem).
fn slot_positions(app: &mut App) -> Vec<Vec3> {
    let world = app.world_mut();
    let mut q = world.query_filtered::<&Transform, With<TowerSlot>>();
    q.iter(world).map(|t| t.translation).collect()
}

/// Place a base tower at a slot, paying its cost. Marks the slot used by bumping
/// `occupied`. Returns true if the tower was placed.
fn place_one(app: &mut App, tower_type: usize, occupied: &mut usize) -> bool {
    let defs = app.world().resource::<TowerDefs>().list.clone();
    let def = &defs[tower_type];
    let slots = slot_positions(app);
    if *occupied >= slots.len() {
        return false;
    }
    let gold = app.world().resource::<Economy>().gold;
    if gold < def.cost {
        return false;
    }
    *app.world_mut().resource_mut::<Economy>() = Economy { gold: gold - def.cost, wood: 0 };
    let pos = slots[*occupied];
    let tower = Tower {
        tower_index: tower_type,
        damage: def.damage,
        attack_speed: def.attack_speed,
        range: def.range,
        attack_type: def.attack_type,
        cooldown: 0.0,
        target: None,
        kind: TowerKind::Base(tower_type),
        aoe_radius: 0.0,
        slow_factor: 0.0,
        slow_duration: 0.0,
        invested: 0,
    };
    let tf = Transform::from_xyz(pos.x, 0.6, pos.z);
    // SK9: the harness mirrors the real gateway — every tower gets a StatMods
    // bag and the archer ships with its kit (the SK11 contribution readout
    // needs real skill damage).
    if tower_type == 0 {
        app.world_mut()
            .spawn((tower, StatMods::default(), archer_skill_caster(), tf));
    } else {
        app.world_mut().spawn((tower, StatMods::default(), tf));
    }
    *occupied += 1;
    true
}

/// A sane greedy auto-builder: spend gold at intermissions on a mixed loadout
/// (archer/mage/cannon/shield cycling) as long as slots and gold allow.
fn invest_build(app: &mut App, occupied: &mut usize) {
    let defs = app.world().resource::<TowerDefs>().list.clone();
    let order = [0usize, 2, 3, 1]; // archer, mage, cannon, shield
    let mut i = 0;
    loop {
        let cheapest = defs.iter().map(|d| d.cost).min().unwrap();
        let gold = app.world().resource::<Economy>().gold;
        if gold < cheapest || *occupied >= slot_positions(app).len() {
            break;
        }
        let t = order[i % order.len()];
        i += 1;
        if !place_one(app, t, occupied) {
            continue; // try the next type
        }
    }
}

/// Resolve a pending three-choose-one by pressing 1 (picks option 0), then a
/// single update so the choice system applies it.
fn auto_choose(app: &mut App) {
    if app.world().resource::<WaveChoice>().pending {
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Digit1);
        app.update();
    }
}

/// SK11: damage by source — the skill-contribution readout (BAL anchor
/// 「塔技能占比」and the commander ≤20% review both read these columns).
#[derive(Default)]
struct DamageLedger {
    tower: f32,
    tower_skill: f32,
    commander: f32,
}

/// SK11: arm the harness archers like a player would — fire-arrow autocast
/// ON (set once after building; the trigger recasts on cooldown).
fn arm_archer_skills(app: &mut App) {
    let mut q = app.world_mut().query::<&mut SkillCaster>();
    for mut caster in q.iter_mut(app.world_mut()) {
        for slot in caster.slots.iter_mut() {
            if slot.skill == SKILL_FIRE_ARROWS {
                slot.autocast_on = true;
            }
        }
    }
}

/// BAL2: every ready NO-targeting skill on the board fires — multishot and
/// deadeye on towers, war frenzy on the commander. (Point skills need the
/// aiming confirm dance; see `cast_commander_meteor` for that.)
fn cast_ready_none_skills(app: &mut App) {
    let mut requests: Vec<CastSkill> = Vec::new();
    {
        let mut q = app.world_mut().query::<(bevy::ecs::entity::Entity, &SkillCaster)>();
        for (e, caster) in q.iter(app.world()) {
            for (slot, s) in caster.slots.iter().enumerate() {
                let none_targeted = matches!(s.skill.as_str(), SKILL_MULTISHOT | SKILL_DEADEYE | SKILL_WAR_FRENZY);
                if none_targeted && s.cooldown <= 0.0 {
                    requests.push(CastSkill { caster: e, slot });
                }
            }
        }
    }
    let mut msgs = app.world_mut().resource_mut::<Messages<CastSkill>>();
    for req in requests {
        msgs.write(req);
    }
}

/// BAL2: fuse the first pair of base archers into a marksman (the real
/// gateway, so the signature kit + StatMods ride along). Called at
/// intermissions once the builder has produced a pair. `occupied` stays
/// put — the freed slot is behind the builder's cursor (harness simplification).
fn fuse_archers(app: &mut App) -> bool {
    let mut ids: Vec<bevy::ecs::entity::Entity> = Vec::new();
    let mut pos_b = None;
    {
        let mut q = app
            .world_mut()
            .query::<(bevy::ecs::entity::Entity, &Tower, &Transform)>();
        for (e, t, tf) in q.iter(app.world_mut()) {
            if t.kind == TowerKind::Base(0) && ids.len() < 2 {
                ids.push(e);
                if ids.len() == 2 {
                    pos_b = Some(tf.translation);
                }
            }
        }
    }
    if ids.len() < 2 {
        return false;
    }
    let recipe = app
        .world()
        .resource::<FusionDefs>()
        .list
        .iter()
        .find(|r| r.kind == warden::components::FusionKind::Marksman)
        .expect("marksman recipe")
        .clone();
    let defs = app.world().resource::<TowerDefs>().list.clone();
    let cost_ing = defs[recipe.ingredients[0]].cost + defs[recipe.ingredients[1]].cost;
    let fee = (cost_ing as f32 * recipe.fee_ratio).round() as u32;
    let gold = app.world().resource::<Economy>().gold;
    if gold < fee {
        return false;
    }
    *app.world_mut().resource_mut::<Economy>() = Economy { gold: gold - fee, wood: 0 };
    for e in ids {
        app.world_mut().despawn(e);
    }
    let mut meshes = std::mem::take(&mut *app.world_mut().resource_mut::<Assets<Mesh>>());
    let mut materials =
        std::mem::take(&mut *app.world_mut().resource_mut::<Assets<StandardMaterial>>());
    let mut queue = bevy::ecs::world::CommandQueue::default();
    let _fused = {
        let mut commands = Commands::new(&mut queue, app.world());
        warden::systems::tower::spawn_fused_tower(
            &mut commands,
            &recipe,
            pos_b.unwrap_or(Vec3::ZERO),
            cost_ing + fee,
            &mut meshes,
            &mut materials,
        )
    };
    queue.apply(app.world_mut());
    *app.world_mut().resource_mut::<Assets<Mesh>>() = meshes;
    *app.world_mut().resource_mut::<Assets<StandardMaterial>>() = materials;
    true
}

/// BAL2 headless picking bootstrap (the behavior.rs harness pattern): a
/// window with a cursor + a camera whose viewport can project.
fn enable_picking(app: &mut App, cursor: Vec2) {
    let w = app.world_mut().spawn(Window::default()).id();
    app.world_mut()
        .get_mut::<Window>(w)
        .expect("window")
        .set_cursor_position(Some(cursor));
    let size = UVec2::new(1280, 720);
    let mut cams = app.world_mut().query::<(
        &mut Camera,
        &Projection,
        &Transform,
        &mut GlobalTransform,
    )>();
    for (mut cam, proj, tf, mut gt) in cams.iter_mut(app.world_mut()) {
        cam.viewport = Some(Viewport {
            physical_size: size,
            ..default()
        });
        cam.computed.target_info = Some(RenderTargetInfo {
            physical_size: size,
            scale_factor: 1.0,
        });
        let mut p = proj.clone();
        p.update(1280.0, 720.0);
        cam.computed.clip_from_view = p.get_clip_from_view();
        *gt = GlobalTransform::from(*tf);
    }
}

/// BAL2: cast meteor rain at the enemy pack's centroid through the real
/// request -> aiming -> cursor-confirm path (drives the commander column).
fn cast_commander_meteor(app: &mut App) {
    // Centroid of the living enemies (skip the cast on an empty board).
    let (count, sum) = {
        let mut q = app
            .world_mut()
            .query_filtered::<&Transform, bevy::ecs::query::With<Enemy>>();
        q.iter(app.world()).fold((0u32, Vec3::ZERO), |(n, s), tf| {
            (n + 1, s + tf.translation)
        })
    };
    if count == 0 {
        return;
    }
    let point = sum / count as f32;
    let commander = {
        let mut q = app
            .world_mut()
            .query_filtered::<(bevy::ecs::entity::Entity, &SkillCaster), bevy::ecs::query::With<Commander>>();
        let mut found = None;
        for (e, c) in q.iter(app.world()) {
            found = Some((e, c.clone()));
            break;
        }
        found
    };
    let Some((cmd, caster)) = commander else { return };
    let Some(slot) = caster
        .slots
        .iter()
        .position(|s| s.skill == SKILL_METEOR_RAIN && s.cooldown <= 0.0)
    else {
        return;
    };
    app.world_mut()
        .resource_mut::<Messages<CastSkill>>()
        .write(CastSkill { caster: cmd, slot });
    app.update();
    if app.world().resource::<SkillAiming>().0.is_none() {
        return; // validation refused (dead/paused) — drop it
    }
    enable_picking(app, Vec2::new(640.0, 360.0));
    let cursor = {
        let mut cams = app.world_mut().query::<(&Camera, &GlobalTransform)>();
        let (cam, tf) = cams.single(app.world()).expect("one camera");
        match cam.world_to_viewport(tf, point) {
            Ok(pt) => pt,
            Err(_) => return,
        }
    };
    {
        let mut q = app.world_mut().query_filtered::<&mut Window, ()>();
        for mut w in q.iter_mut(app.world_mut()) {
            w.set_cursor_position(Some(cursor));
        }
    }
    app.world_mut()
        .resource_mut::<ButtonInput<bevy::input::mouse::MouseButton>>()
        .press(bevy::input::mouse::MouseButton::Left);
    app.update();
    app.world_mut()
        .resource_mut::<ButtonInput<bevy::input::mouse::MouseButton>>()
        .clear();
    app.world_mut()
        .resource_mut::<SkillAiming>()
        .0 = None; // safety: never leak aiming into later waves
}

/// Run `remaining` waves (each: choose -> invest -> start -> resolve) from the
/// current intermission. Returns samples plus whether the run is still alive.
/// Drains DamageDealt per frame into `ledger` (double-buffered messages).
fn run_waves(
    app: &mut App,
    remaining: u32,
    occupied: &mut usize,
    cap: u32,
    ledger: &mut DamageLedger,
) -> (Vec<WaveSample>, bool) {
    let mut samples = Vec::new();
    for _ in 0..remaining {
        if *app.world().resource::<State<GameState>>() != GameState::Playing {
            return (samples, false);
        }
        auto_choose(app);
        invest_build(app, occupied);
        // BAL2: fuse archer pairs during the early waves (marksman + deadeye
        // join the tower-skill column).
        if app.world().resource::<WaveState>().current < 3 {
            fuse_archers(app);
        }
        arm_archer_skills(app);

        let start = app.world().resource::<Time>().elapsed_secs();
        let start_hp = app.world().resource::<BaseHp>().hp;
        let start_gold = app.world().resource::<Economy>().gold;
        let start_wave = app.world().resource::<WaveState>().current;

        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Space);
        let mut updates = 0u32;
        let mut resolved = false;
        let cmd = {
            let mut q = app
                .world_mut()
                .query_filtered::<Entity, With<Commander>>();
            q.single(app.world()).ok()
        };
        let mut casts = MessageCursor::<DamageDealt>::default();
        while updates < cap {
            // ~3s into the wave: the pack is on the board — fire every
            // ready no-targeting skill (multishot/deadeye/frenzy) and drop
            // a meteor on the pack's centroid (commander column).
            if updates == 180 {
                cast_ready_none_skills(app);
                cast_commander_meteor(app);
            }
            app.update();
            updates += 1;
            {
                let msgs = app.world().resource::<Messages<DamageDealt>>();
                for d in casts.read(msgs) {
                    // WX6-4b: base origins — Attack is the tower column;
                    // skills split by caster (commander vs tower).
                    match (d.source, cmd) {
                        (DamageOrigin::Attack(_), _) => ledger.tower += d.amount,
                        (DamageOrigin::Skill { caster, .. }, Some(cmd)) if caster == cmd => {
                            ledger.commander += d.amount
                        }
                        (DamageOrigin::Skill { .. }, _) => ledger.tower_skill += d.amount,
                        (DamageOrigin::System, _) => {}
                    }
                }
            }
            let wave = app.world().resource::<WaveState>();
            if wave.current != start_wave && wave.phase == WavePhase::Intermission {
                resolved = true;
                break;
            }
        }
        let end = app.world().resource::<Time>().elapsed_secs();
        let hp = app.world().resource::<BaseHp>().hp;
        let gold = app.world().resource::<Economy>().gold;
        samples.push(WaveSample {
            index: start_wave,
            clear_secs: if resolved { end - start } else { 0.0 },
            leaks: start_hp.saturating_sub(hp),
            gold_gain: gold.saturating_sub(start_gold),
        });
        if !resolved {
            return (samples, false);
        }
    }
    (samples, true)
}

/// Balance readout smoke (harness guard): with a comfortable budget, run a few
/// waves and read their numbers. Guards the harness, not balance values.
#[test]
fn balance_readout_smoke() {
    let mut app = test_app();
    app.update();
    *app.world_mut().resource_mut::<Economy>() = Economy { gold: 260, wood: 0 };
    let mut occupied = 0usize;
    invest_build(&mut app, &mut occupied);
    assert!(occupied >= 3, "loadout must be affordable to exercise the readout");

    let (samples, alive) = run_waves(&mut app, 3, &mut occupied, 4000, &mut DamageLedger::default());
    assert!(alive, "smoke run should stay alive");
    assert_eq!(samples.len(), 3, "three wave samples");

    println!("--- balance readout smoke (occupied {occupied} towers) ---");
    println!("wave | clear(s) | leaks | gold+");
    for s in &samples {
        println!(
            "  {}  |   {:8.2}   |  {:2}   |  {}",
            s.index, s.clear_secs, s.leaks, s.gold_gain
        );
        assert!(s.clear_secs > 0.0, "wave {} clear time must be positive", s.index);
    }
    let wave = app.world().resource::<WaveState>();
    assert!(wave.current >= 3, "expected at least 3 waves consumed, got {}", wave.current);
}

/// Difficulty curve readout: full run under the sane auto-build policy, from the
/// realistic 100 starting gold. Reports per-wave leaks and the win/lose result.
#[test]
fn full_run_difficulty_curve() {
    let mut app = test_app();
    app.update();
    let mut occupied = 0usize;
    let mut ledger = DamageLedger::default();
    invest_build(&mut app, &mut occupied); // spend the 100 starting gold

    let (samples, _alive) = run_waves(&mut app, 10, &mut occupied, 6000, &mut ledger);
    let base = app.world().resource::<BaseHp>();
    let gold = app.world().resource::<Economy>();
    let state = *app.world().resource::<State<GameState>>().get();
    println!("--- full run difficulty curve (auto-build, start 100g) ---");
    println!("wave | clear(s) | leaks | gold+ | towers={occupied}");
    for s in &samples {
        println!(
            "  {}  |   {:8.2}   |  {:2}   |  {}",
            s.index, s.clear_secs, s.leaks, s.gold_gain
        );
    }
    println!(
        "result: state={state:?} waves_cleared={} base_hp={}/{} gold={}",
        samples.len(),
        base.hp,
        base.max_hp,
        gold.gold
    );
    // SK11 skill-contribution readout: damage share by source. The BAL2
    // anchor rows (commander ≤20%, tower-skill share target) live in
    // requirements §14; this harness prints the columns, the anchor test
    // pins the ranges once the design owner sets them.
    let total = ledger.tower + ledger.tower_skill + ledger.commander;
    let pct = |v: f32| if total > 0.0 { v / total * 100.0 } else { 0.0 };
    println!(
        "damage share: towers {:.0} ({:.1}%) | tower skills {:.0} ({:.1}%) | commander skills {:.0} ({:.1}%) | total {:.0}",
        ledger.tower,
        pct(ledger.tower),
        ledger.tower_skill,
        pct(ledger.tower_skill),
        ledger.commander,
        pct(ledger.commander),
        total
    );
    // SK11 acceptance: with archers on the board and skills driven, both
    // the normal-attack and tower-skill columns are non-zero.
    assert!(ledger.tower > 0.0, "tower normal attacks must deal damage");
    assert!(
        ledger.tower_skill > 0.0,
        "tower skills must contribute damage (fire arrows autocast + multishot/deadeye)"
    );
    // BAL2: the commander column drives too (meteor on the pack centroid),
    // so the ≤20% anchor review (§17.6) has all three columns live.
    assert!(
        ledger.commander > 0.0,
        "commander skills must contribute damage (meteor cast per wave)"
    );
    // Anchor REVIEW (report-only; range pinning stays with the design
    // owner per the 2026-09-06 decision to hold numeric convergence).
    let cmd_pct = pct(ledger.commander);
    let skill_pct = pct(ledger.tower_skill);
    println!(
        "anchor review: commander {cmd_pct:.1}% vs <=20% ({}) | tower skills {skill_pct:.1}% vs <=10% ({})",
        if cmd_pct <= 20.0 { "within" } else { "OVER" },
        if skill_pct <= 10.0 { "within" } else { "OVER" },
    );
    // Harness sanity only: at least one wave must have been consumed and a
    // sample produced. Difficulty is the *report*, not an assertion here — the
    // balance-range assertions live in a future BAL2 anchor test.
    let wave = app.world().resource::<WaveState>();
    assert!(
        !samples.is_empty(),
        "harness must produce at least one wave sample, got {}",
        samples.len()
    );
    println!("(wave.current={} samples.len={})", wave.current, samples.len());
}
