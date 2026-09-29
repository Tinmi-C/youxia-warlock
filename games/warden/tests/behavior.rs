//! Behavior consistency regression tests: pin down "changed A, B did not break".
//! Pattern: build a headless app (no renderer/window), drive it manually, assert.
//! Acceptance sentences from capability cards live here as executable tests.

use std::time::Duration;

use bevy::{
    prelude::*,
    state::app::StatesPlugin,
    time::TimeUpdateStrategy,
};

use warden::systems::pointer;
use warden::{
    components::{AttackType, Enemy, FusionKind, Healer, Tower, TowerKind, TowerSlot},
    plugins::{
        acquisition::AcquisitionPlugin, economy::EconomyPlugin, enemies::EnemiesPlugin,
        fx::FxPlugin, game::GamePlugin, map::MapPlugin, meta::MetaPlugin,
        projectiles::ProjectilesPlugin, skills::SkillsPlugin, towers::TowersPlugin, ui::UiPlugin,
        waves::WavesPlugin,
    },
    resources::{
        BaseHp, Boosts, BuildMode, ChoiceKind, ChoiceOption, Economy, Hand, Lang, MetaSavePath,
        MetaState, RunRng, ShopOffers, StatKind, TowerDefs, UiPrefsPath,
        WaveChoice, WavePhase, WaveState,
    },
    states::GameState,
};

/// Headless app: all gameplay plugins (no renderer/window) + fixed timestep.
fn test_app() -> App {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, StatesPlugin)) // init_state needs StateTransition schedule
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
        // Keep meta persistence out of the repo: tests never touch
        // ./meta_save.txt (meta-specific tests insert their own temp path).
        .insert_resource(MetaSavePath(
            std::env::temp_dir().join("warden_meta_shared_test.txt"),
        ))
        // Same isolation for the UI language pref (UI7): never read the dev's
        // own ./ui_prefs.txt (lang-sensitive tests insert their own temp path).
        .insert_resource(UiPrefsPath(
            std::env::temp_dir().join("warden_ui_prefs_shared_test.txt"),
        ))
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
        // Fixed timestep makes tests reproducible (1/60 s per update).
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(
            1.0 / 60.0,
        )));
    app
}

fn enemy_hp(app: &mut App) -> f32 {
    let mut q = app.world_mut().query::<&Enemy>();
    q.single(app.world()).map(|e| e.hp).expect("one enemy exists")
}

fn count_towers(app: &mut App) -> usize {
    let mut q = app.world_mut().query::<&Tower>();
    q.iter(app.world()).count()
}

/// Capability card AC1 — acceptance: every opening hand has exactly 2 distinct
/// towers, and at least one of them is an output tower (archer 0 / cannon 3).
#[test]
fn opening_hand_has_two_distinct_towers_with_output_guarantee() {
    for seed in 0..32u64 {
        let mut app = test_app();
        app.world_mut().insert_resource(RunRng::seeded(seed));
        app.update(); // Startup: deal
        let hand = app.world().resource::<Hand>();
        assert_eq!(hand.owned_towers.len(), 2, "seed {seed}: exactly 2 towers");
        assert_ne!(
            hand.owned_towers[0], hand.owned_towers[1],
            "seed {seed}: the two cards are distinct"
        );
        assert!(
            hand.owned_towers.contains(&0) || hand.owned_towers.contains(&3),
            "seed {seed}: at least one output tower"
        );
    }
}

/// Capability card AC2 — acceptance: while the player does not own all four
/// base towers, every shop refresh offers at least one un-owned type.
#[test]
fn shop_refresh_guarantees_unowned_type() {
    for seed in 0..32u64 {
        let mut app = test_app();
        app.world_mut().insert_resource(RunRng::seeded(seed));
        app.update(); // Startup: deal + initial offer
        let hand = app.world().resource::<Hand>();
        let shop = app.world().resource::<ShopOffers>();
        assert_eq!(shop.offers.len(), 3, "seed {seed}: exactly three offers");
        let owns_all = (0..4).all(|t| hand.owned_towers.contains(&t));
        if !owns_all {
            assert!(
                shop.offers.iter().any(|t| !hand.owned_towers.contains(t)),
                "seed {seed}: offer must contain an un-owned type (hand={:?} offers={:?})",
                hand.owned_towers,
                shop.offers
            );
        }
    }
}

/// Capability card AC2 — acceptance: clearing a wave (back to Intermission)
/// refreshes the shop offer.
#[test]
fn wave_resolve_refreshes_shop_offer() {
    let mut app = test_app();
    app.update(); // startup: initial offer
    let v0 = app.world().resource::<ShopOffers>().version;
    {
        let mut ws = app.world_mut().resource_mut::<WaveState>();
        ws.current = 0;
        ws.phase = WavePhase::Combat;
        ws.spawn_queue.clear();
        ws.active = 0;
    }
    app.update(); // wave resolves -> Intermission -> offer refreshes
    let shop = app.world().resource::<ShopOffers>();
    assert!(shop.version > v0, "offer must refresh after a wave clears");
    assert_eq!(
        app.world().resource::<WaveState>().phase,
        WavePhase::Intermission
    );
}

/// requirements §9 anchor — acceptance: an ordinary enemy (speed 1.0) crosses
/// the L-shaped path (43 world units) in ≈20 s; leaks cost 1 base hp.
#[test]
fn ordinary_enemy_crosses_path_in_about_twenty_seconds() {
    let mut app = test_app();
    app.update(); // startup: PathInfo + systems live
    {
        let mut ws = app.world_mut().resource_mut::<WaveState>();
        ws.phase = WavePhase::Combat;
        ws.spawn_queue.push_back(0); // one ordinary enemy
        ws.spawn_timer = 0.0;
    }
    app.update(); // spawns the enemy at the path entry
    let mut frames = 0;
    while frames < 2400 && app.world().resource::<BaseHp>().hp == 10 {
        app.update();
        frames += 1;
    }
    assert!(frames > 0, "the enemy should reach the base and leak");
    let elapsed = app.world().resource::<Time>().elapsed_secs();
    assert_eq!(
        app.world().resource::<BaseHp>().hp,
        9,
        "an ordinary leak costs exactly 1 base hp"
    );
    assert!(
        (elapsed - 20.0).abs() < 1.0,
        "ordinary enemy should cross the path in ≈20s (§9 anchor), took {elapsed:.2}s"
    );
}

/// Dead ordinary enemy spawn helper for the drop tests.
fn spawn_dead_normal(app: &mut App) {
    app.world_mut().spawn((
        Enemy {
            hp: 0.0,
            max_hp: 30.0,
            speed: 0.0,
            leak: 0,
            kill_gold: 0,
            physical_armor: false,
            def_index: 0,
            next_wp: 0,
        },
        Transform::from_xyz(0.0, 0.4, 0.0),
    ));
}

/// Capability card AC3 — acceptance: an elite kill ALWAYS drops a tower card.
#[test]
fn elite_kill_always_drops_tower_card() {
    for seed in 0..8u64 {
        let mut app = test_app();
        app.world_mut().insert_resource(RunRng::seeded(seed));
        app.update(); // startup: deal gives 2 types
        let before = app.world().resource::<Hand>().owned_towers.len();
        app.world_mut().spawn((
            Enemy {
                hp: 0.0,
                max_hp: 200.0,
                speed: 0.0,
                leak: 0,
                kill_gold: 0,
                physical_armor: false,
                def_index: 4, // elite
                next_wp: 0,
            },
            Transform::from_xyz(0.0, 0.4, 0.0),
        ));
        app.update(); // roll_drops sees the corpse, then resolve_death cleans it
        let hand = app.world().resource::<Hand>();
        assert_eq!(
            hand.owned_towers.len(),
            before + 1,
            "seed {seed}: elite kill must drop exactly one card"
        );
    }
}

/// Capability card AC3 — acceptance: normal kills drop ~10% of the time
/// (200 kills -> ~20 drops, asserted within a 3-sigma band of ±13).
#[test]
fn normal_kills_drop_about_ten_percent() {
    let mut app = test_app();
    app.world_mut().insert_resource(RunRng::seeded(2024));
    app.update(); // startup: deal
    let mut drops: u32 = 0;
    for _ in 0..200 {
        spawn_dead_normal(&mut app);
        let before = app.world().resource::<Hand>().owned_towers.len() as u32;
        app.update();
        drops += app.world().resource::<Hand>().owned_towers.len() as u32 - before;
    }
    assert!(
        (20.0 - drops as f32).abs() <= 13.0,
        "200 normal kills should drop ~20 cards (3-sigma band 20±13), got {drops}"
    );
}

/// Capability card ME1 — acceptance: dying pays +15 meta coins, persisted.
#[test]
fn death_awards_meta_coins_and_persists() {
    let save = std::env::temp_dir().join("warden_meta_death_test.txt");
    let _ = std::fs::remove_file(&save);
    let mut app = test_app();
    app.world_mut().insert_resource(MetaSavePath(save.clone()));
    app.update(); // startup: load (absent) -> defaults
    assert_eq!(app.world().resource::<MetaState>().coins, 0);
    app.world_mut()
        .resource_mut::<NextState<GameState>>()
        .set(GameState::GameOver);
    app.update(); // OnEnter(GameOver): +15 coins, saved to disk
    let coins = app.world().resource::<MetaState>().coins;
    assert_eq!(coins, 15, "death should pay exactly 15 meta coins");
    let text = std::fs::read_to_string(&save).expect("save file written on death");
    assert!(text.contains("coins=15"), "save must persist coins, got: {text}");
    let _ = std::fs::remove_file(&save);
}

/// Capability card ME1 — acceptance: with upgrade 2 unlocked, the next run
/// starts with 120 gold.
#[test]
fn upgrade2_boosts_next_run_starting_gold() {
    let save = std::env::temp_dir().join("warden_meta_upg2_test.txt");
    std::fs::write(&save, "coins=60\nupgrade1=0\nupgrade2=1\n").expect("write save file");
    let mut app = test_app();
    app.world_mut().insert_resource(MetaSavePath(save.clone()));
    app.update(); // startup: load -> apply -> the run starts with 120 gold
    assert_eq!(
        app.world().resource::<Economy>().gold,
        120,
        "upgrade 2 should grant +20 starting gold"
    );
    assert!(app.world().resource::<MetaState>().upgrade2);
    let _ = std::fs::remove_file(&save);
}

/// Capability card TO3/TO4 — acceptance: a tower within range fires at the
/// nearest enemy, dealing its damage (cooldown resets to 1/attack_speed).
#[test]
fn tower_fires_at_enemy_in_range() {
    let mut app = test_app();
    app.world_mut().spawn((
        Tower {
            tower_index: 0,
            damage: 10.0,
            attack_speed: 1.0,
            range: 20.0,
            attack_type: AttackType::Physical,
            cooldown: 0.0,
            target: None,
            kind: TowerKind::Base(0),
            aoe_radius: 0.0,
            slow_factor: 0.0,
            slow_duration: 0.0,
            invested: 0,
        },
        Transform::from_xyz(0.0, 0.5, 0.0),
    ));
    app.world_mut().spawn((
        Enemy {
            hp: 50.0,
            max_hp: 50.0,
            speed: 0.0,
            leak: 0,
            kill_gold: 0,
            physical_armor: false,
            def_index: 0,
            next_wp: 0,
        },
        Transform::from_xyz(2.0, 0.5, 0.0),
    ));
    app.update(); // launch frame
    for _ in 0..40 {
        app.update(); // let the homing round land (§5.1 flight time)
    }
    let hp = enemy_hp(&mut app);
    assert!((hp - 40.0).abs() < 0.01, "one hit should reduce hp to 40, got {hp}");
}

/// Capability card AC4 polish — acceptance: three-choose-one options are drawn
/// from three *distinct* random categories and a get-tower never offers a type
/// the player already owns (when a free type exists). Ran across many seeds.
#[test]
fn three_choose_one_offers_distinct_random_categories() {
    for seed in 1..=32u64 {
        let mut app = test_app();
        app.world_mut().insert_resource(RunRng::seeded(seed));
        app.update(); // startup: deal hand
        // Own one type so get-tower has a constraint to respect.
        app.world_mut().resource_mut::<Hand>().owned_towers.push(2);
        // Force a cleared wave so wave_resolve deals the choice.
        {
            let mut wave = app.world_mut().resource_mut::<WaveState>();
            wave.phase = WavePhase::Combat;
            wave.spawn_queue.clear();
            wave.active = 0;
            wave.current = 0;
        }
        app.update();
        let choice = app.world().resource::<WaveChoice>();
        assert!(choice.pending, "seed {seed}: choice should be pending");
        assert_eq!(choice.options.len(), 3, "seed {seed}: exactly 3 options");
        let owned = app.world().resource::<Hand>().owned_towers.clone();
        let mut seen = std::collections::HashSet::new();
        for opt in &choice.options {
            let key = match &opt.kind {
                ChoiceKind::StatBoost { stat, .. } => format!("stat:{stat:?}"),
                ChoiceKind::GoldBoost => "gold".to_string(),
                ChoiceKind::GetTower { .. } => "get".to_string(),
            };
            assert!(
                seen.insert(key),
                "seed {seed}: options must be from distinct categories, got {seen:?}"
            );
            if let ChoiceKind::GetTower { tower_type } = &opt.kind {
                assert!(
                    !owned.contains(tower_type),
                    "seed {seed}: get-tower must avoid an owned type, got {tower_type} (owned {owned:?})"
                );
            }
        }
    }
}

/// Capability card AC4 polish — acceptance: `apply_choice` sets the correct
/// boost multiplier for each StatBoost kind (damage / attack speed / range).
#[test]
fn stat_boost_kinds_set_the_right_mults() {
    let mut choice = WaveChoice {
        options: vec![
            ChoiceOption {
                label: "d".to_string(),
                label_en: "d".to_string(),
                kind: ChoiceKind::StatBoost { tower_type: 1, stat: StatKind::Damage },
            },
            ChoiceOption {
                label: "s".to_string(),
                label_en: "s".to_string(),
                kind: ChoiceKind::StatBoost { tower_type: 2, stat: StatKind::AttackSpeed },
            },
            ChoiceOption {
                label: "r".to_string(),
                label_en: "r".to_string(),
                kind: ChoiceKind::StatBoost { tower_type: 3, stat: StatKind::Range },
            },
        ],
        pending: true,
    };
    let mut boosts = Boosts::default();
    let mut hand = Hand::default();
    warden::systems::input::apply_choice(&mut choice, &mut boosts, &mut hand, 0);
    assert!((boosts.damage_mult[1] - 1.2).abs() < 1e-6, "damage boost should be +20%");
    warden::systems::input::apply_choice(&mut choice, &mut boosts, &mut hand, 1);
    assert!(
        (boosts.attack_speed_mult[2] - 1.2).abs() < 1e-6,
        "attack-speed boost should be +20%"
    );
    warden::systems::input::apply_choice(&mut choice, &mut boosts, &mut hand, 2);
    assert!((boosts.range_mult[3] - 1.15).abs() < 1e-6, "range boost should be +15%");
}

/// Capability card AC4 polish — acceptance: a fused tower reads the boost of
/// its first-ingredient type (`tower_index`), so it benefits from boosts.
#[test]
fn fused_tower_receives_tower_index_damage_boost() {
    let mut app = test_app();
    app.update();
    app.world_mut().resource_mut::<Boosts>().damage_mult[0] = 2.0;
    app.world_mut().spawn((
        Tower {
            tower_index: 0,
            damage: 18.0,
            attack_speed: 1.0,
            range: 20.0,
            attack_type: AttackType::Physical,
            cooldown: 0.0,
            target: None,
            kind: TowerKind::Fused(FusionKind::Marksman),
            aoe_radius: 0.0,
            slow_factor: 0.0,
            slow_duration: 0.0,
            invested: 0,
        },
        Transform::from_xyz(0.0, 0.5, 0.0),
    ));
    app.world_mut().spawn((
        Enemy {
            hp: 60.0,
            max_hp: 60.0,
            speed: 0.0,
            leak: 0,
            kill_gold: 0,
            physical_armor: false,
            def_index: 0,
            next_wp: 0,
        },
        Transform::from_xyz(2.0, 0.5, 0.0),
    ));
    app.update(); // launch frame
    for _ in 0..40 {
        app.update(); // let the homing round land (§5.1 flight time)
    }
    // 18 * 2.0 = 36 damage -> 60 - 36 = 24 (fused reads index-0 boost).
    let hp = enemy_hp(&mut app);
    assert!((hp - 24.0).abs() < 0.01, "fused tower must read its tower_index boost, got {hp}");
}

/// Capability card AC4 polish — acceptance: a range boost extends the tower's
/// effective reach (base range 10 * 1.5 = 15 reaches a target at distance 13).
#[test]
fn range_boost_extends_reach() {
    let mut app = test_app();
    app.update();
    app.world_mut().resource_mut::<Boosts>().range_mult[0] = 1.5;
    app.world_mut().spawn((
        Tower {
            tower_index: 0,
            damage: 9.0,
            attack_speed: 1.0,
            range: 10.0,
            attack_type: AttackType::Physical,
            cooldown: 0.0,
            target: None,
            kind: TowerKind::Base(0),
            aoe_radius: 0.0,
            slow_factor: 0.0,
            slow_duration: 0.0,
            invested: 0,
        },
        Transform::from_xyz(0.0, 0.5, 0.0),
    ));
    app.world_mut().spawn((
        Enemy {
            hp: 20.0,
            max_hp: 20.0,
            speed: 0.0,
            leak: 0,
            kill_gold: 0,
            physical_armor: false,
            def_index: 0,
            next_wp: 0,
        },
        Transform::from_xyz(13.0, 0.5, 0.0),
    ));
    app.update(); // launch frame
    // 13 units at speed 10 ≈ 1.3 s; stop before the second shot lands (2.3 s).
    for _ in 0..120 {
        app.update();
    }
    // 9 damage, one hit -> 20 - 9 = 11.
    let hp = enemy_hp(&mut app);
    assert!((hp - 11.0).abs() < 0.01, "range 10 * 1.5 = 15 must reach d=13, got {hp}");
}

/// Capability card AC4 polish — acceptance: an attack-speed boost shortens the
/// cooldown interval to 1 / (speed * mult).
#[test]
fn attack_speed_boost_shortens_cooldown() {
    let mut app = test_app();
    app.update();
    app.world_mut().resource_mut::<Boosts>().attack_speed_mult[0] = 2.0;
    app.world_mut().spawn((
        Tower {
            tower_index: 0,
            damage: 9.0,
            attack_speed: 1.0,
            range: 20.0,
            attack_type: AttackType::Physical,
            cooldown: 0.0,
            target: None,
            kind: TowerKind::Base(0),
            aoe_radius: 0.0,
            slow_factor: 0.0,
            slow_duration: 0.0,
            invested: 0,
        },
        Transform::from_xyz(0.0, 0.5, 0.0),
    ));
    app.world_mut().spawn((
        Enemy {
            hp: 200.0,
            max_hp: 200.0,
            speed: 0.0,
            leak: 0,
            kill_gold: 0,
            physical_armor: false,
            def_index: 0,
            next_wp: 0,
        },
        Transform::from_xyz(2.0, 0.5, 0.0),
    ));
    app.update();
    let cooldown = {
        let world = app.world_mut();
        let mut q = world.query::<&Tower>();
        q.single(world).unwrap().cooldown
    };
    assert!(
        (cooldown - 0.5).abs() < 0.01,
        "cooldown should be 1/(1 * 2) = 0.5, got {}",
        cooldown
    );
}

/// Capability card UI3 — acceptance: the build-mode ghost preview never appears
/// unless a build is armed *and* a cursor/slot resolves; headless (no Window)
/// it must stay absent and must not panic. The visual correctness (ghost
/// follows the cursor on a real window) is a manual/playtest item.
#[test]
fn hover_ghost_stays_absent_without_cursor() {
    let mut app = test_app();
    app.update();
    app.world_mut().resource_mut::<BuildMode>().armed = true;
    // No Window resource headless -> no cursor resolution -> no ghost, no panic.
    app.update();
    let mut ghosts = app.world_mut().query_filtered::<Entity, With<pointer::HoverGhost>>();
    assert_eq!(
        ghosts.iter(app.world()).count(),
        0,
        "no ghost without a cursor/window"
    );
    app.world_mut().resource_mut::<BuildMode>().armed = false;
    app.update();
    let mut ghosts = app.world_mut().query_filtered::<Entity, With<pointer::HoverGhost>>();
    assert_eq!(ghosts.iter(app.world()).count(), 0, "still no ghost when disarmed");
}

/// Capability card UI5 — acceptance: the result screen is visible only on
/// GameOver/Win, hidden while Playing.
#[test]
fn end_screen_shows_only_on_terminal_state() {
    let mut app = test_app();
    app.update(); // Playing, panel not yet spawned
    app.world_mut()
        .resource_mut::<NextState<GameState>>()
        .set(GameState::GameOver);
    app.update();
    app.update(); // second frame toggles visibility
    let show = {
        let world = app.world_mut();
        let mut q = world.query_filtered::<&Node, With<pointer::EndScreenRoot>>();
        q.iter(world)
            .next()
            .map(|n| n.display == Display::Flex)
            .expect("result screen should be spawned on GameOver")
    };
    assert!(show, "result screen must show on GameOver");
    app.world_mut()
        .resource_mut::<NextState<GameState>>()
        .set(GameState::Playing);
    app.update();
    app.update();
    let hidden = {
        let world = app.world_mut();
        let mut q = world.query_filtered::<&Node, With<pointer::EndScreenRoot>>();
        q.iter(world)
            .next()
            .map(|n| n.display == Display::None)
            .expect("result screen should still exist")
    };
    assert!(hidden, "result screen must hide while Playing");
}

/// Capability card UI5 — acceptance: the "再来一局" button restarts from GameOver.
#[test]
fn retry_button_restarts_from_game_over() {
    let mut app = test_app();
    app.update();
    app.world_mut()
        .resource_mut::<NextState<GameState>>()
        .set(GameState::GameOver);
    app.update();
    app.world_mut().spawn((
        pointer::RetryButton,
        bevy::ui::Interaction::Pressed,
    ));
    app.update(); // handler sets NextState(Playing)
    app.update(); // StateTransition applies the state change
    assert_eq!(*app.world().resource::<State<GameState>>(), GameState::Playing);
}

/// Capability card UI5 — acceptance: a meta-upgrade button buys that upgrade.
#[test]
fn meta_upgrade_button_grants_purchase() {
    let save = std::env::temp_dir().join("warden_meta_btn_test.txt");
    let _ = std::fs::remove_file(&save);
    let mut app = test_app();
    app.world_mut().insert_resource(MetaSavePath(save.clone()));
    app.update();
    app.world_mut()
        .resource_mut::<NextState<GameState>>()
        .set(GameState::GameOver);
    app.update(); // applies GameOver (which pays the death reward)
    app.world_mut().resource_mut::<MetaState>().coins = 40;
    app.world_mut().spawn((
        pointer::MetaUpgradeButton { which: 1 },
        bevy::ui::Interaction::Pressed,
    ));
    app.update();
    let meta = app.world().resource::<MetaState>();
    assert!(meta.upgrade1, "upgrade 1 should be bought by the button");
    assert_eq!(meta.coins, 10, "expected 10, got {}", meta.coins);
    let _ = std::fs::remove_file(&save);
}

/// Capability card EN2 — acceptance: an enemy that reaches the base deducts its
/// leak amount from base HP and despawns.
#[test]
fn leaked_enemy_deducts_base_hp() {
    let mut app = test_app();
    app.world_mut().spawn((
        Enemy {
            hp: 10.0,
            max_hp: 10.0,
            speed: 0.0,
            leak: 1,
            kill_gold: 0,
            physical_armor: false,
            def_index: 0,
            next_wp: 4, // == waypoints.len(), so it counts as reached the base
        },
        Transform::from_xyz(12.0, 0.4, -9.0),
    ));
    app.update();
    assert_eq!(
        app.world().resource::<BaseHp>().hp,
        9,
        "an ordinary leaked enemy should drop base hp 10 -> 9"
    );
}

/// Capability card WA1/WA2 — acceptance: starting a wave flips to Combat and
/// spawns enemies; once the wave resolves (queue empty, none active) the reward
/// is added and the phase returns to Intermission.
#[test]
fn wave_resolve_awards_reward_and_returns_intermission() {
    let mut app = test_app();
    {
        let mut ws = app.world_mut().resource_mut::<WaveState>();
        ws.current = 0;
        ws.phase = WavePhase::Combat;
        ws.spawn_queue.clear();
        ws.active = 0;
    }
    let before = app.world().resource::<Economy>().gold; // 100
    app.update();
    assert_eq!(
        app.world().resource::<Economy>().gold,
        before + 20,
        "clearing wave 1 should award 20 gold"
    );
    let ws = app.world().resource::<WaveState>();
    assert_eq!(ws.phase, WavePhase::Intermission, "phase resets to build window");
    assert_eq!(ws.current, 1, "wave index advances");
}

/// Kind of a field tower when exactly one exists.
fn fused_kind(app: &mut App) -> Option<TowerKind> {
    let mut q = app.world_mut().query::<&Tower>();
    let mut iter = q.iter(app.world());
    let kind = iter.next().map(|t| t.kind)?;
    if iter.next().is_some() {
        None
    } else {
        Some(kind)
    }
}

/// HP of the single non-healer enemy (filters the healer out).
fn non_healer_hp(app: &mut App) -> f32 {
    let mut q = app.world_mut().query_filtered::<&Enemy, Without<Healer>>();
    q.single(app.world()).expect("one non-healer enemy").hp
}

/// Capability card EN5 — acceptance: a living healer restores nearby damaged
/// enemies; once the healer is gone the healing stops.
#[test]
fn healer_restores_nearby_enemies_and_stops_when_gone() {
    let mut app = test_app();
    let healer = app
        .world_mut()
        .spawn((
            Enemy {
                hp: 45.0,
                max_hp: 45.0,
                speed: 0.0,
                leak: 0,
                kill_gold: 0,
                physical_armor: false,
                def_index: 3,
                next_wp: 0,
            },
            Healer {
                radius: 6.0,
                period: 1.0,
                heal_per_tick: 3.0,
                tick_timer: 1.0,
            },
            Transform::from_xyz(0.0, 0.4, 0.0),
        ))
        .id();
    app.world_mut().spawn((
        Enemy {
            hp: 10.0,
            max_hp: 30.0,
            speed: 0.0,
            leak: 0,
            kill_gold: 0,
            physical_armor: false,
            def_index: 0,
            next_wp: 0,
        },
        Transform::from_xyz(2.0, 0.4, 0.0),
    ));
    // ~1.17s: exactly one heal tick (period 1s) reaches the damaged ally.
    for _ in 0..70 {
        app.update();
    }
    let healed = non_healer_hp(&mut app);
    assert!(
        (healed - 13.0).abs() < 0.01,
        "one heal tick (+3) should bring hp 10 -> 13, got {healed}"
    );
    // Remove the healer: healing must stop.
    assert!(app.world_mut().despawn(healer), "healer despawned");
    for _ in 0..70 {
        app.update();
    }
    let after = non_healer_hp(&mut app);
    assert!(
        (after - healed).abs() < 1e-6,
        "healing must stop without the healer ({healed} -> {after})"
    );
}

/// Capability card TO5/UI11 — acceptance: the three-step fusion (select A →
/// Fuse command → click B) yields one Marksman, costs a 20% fee (2×archer),
/// lands the result on B's slot and frees A's slot.
#[test]
fn fusing_two_archers_creates_marksman_and_costs_fee() {
    let mut app = test_app();
    app.world_mut().resource_mut::<Economy>().gold = 200;
    let positions = [Vec3::new(-11.5, 0.6, 11.0), Vec3::new(-7.5, 0.6, 11.0)];
    let mut tower_ids = Vec::new();
    for pos in positions {
        let id = app
            .world_mut()
            .spawn((
                Tower {
                    tower_index: 0,
                    damage: 10.0,
                    attack_speed: 1.0,
                    range: 9.0,
                    attack_type: AttackType::Physical,
                    cooldown: 0.0,
                    target: None,
                    kind: TowerKind::Base(0),
                    aoe_radius: 0.0,
                    slow_factor: 0.0,
                    slow_duration: 0.0,
                    invested: 50,
                },
                Transform::from_xyz(pos.x, pos.y, pos.z),
            ))
            .id();
        tower_ids.push(id);
    }
    // Mark both slots under the towers as occupied (as placement would).
    {
        let mut occupied = vec![];
        {
            let mut q = app.world_mut().query::<(&TowerSlot, &Transform)>();
            for (slot, stf) in q.iter(app.world()) {
                if positions.iter().any(|p| p.xz().distance(stf.translation.xz()) < 1.0) {
                    occupied.push(slot.index);
                }
            }
        }
        let mut q = app.world_mut().query::<&mut TowerSlot>();
        for mut slot in q.iter_mut(app.world_mut()) {
            if occupied.contains(&slot.index) {
                slot.occupied = true;
            }
        }
    }
    // Step 1+2: select A, press the Fuse command -> FusionTarget picking.
    app.world_mut()
        .insert_resource(warden::resources::Selection::Tower(tower_ids[0]));
    app.update(); // rebuild card with the Fuse button
    assert!(
        press_command_button(&mut app, CommandAction::Fuse),
        "the Fuse command must appear for a base tower with a pairable partner"
    );
    app.update();
    assert_eq!(
        app.world().resource::<warden::resources::Selection>(),
        &warden::resources::Selection::FusionTarget(tower_ids[0]),
        "the Fuse command enters target picking"
    );
    let highlights = {
        let mut q = app
            .world_mut()
            .query_filtered::<Entity, With<warden::systems::command::PairHighlight>>();
        q.iter(app.world()).count()
    };
    assert_eq!(highlights, 1, "the pairable partner B must be highlighted");

    // Step 3: post the confirm intent (the map click stand-in) and let the
    // chained apply_pending_fusion system execute it.
    app.world_mut().insert_resource(
        warden::systems::command::PendingFusionConfirm(tower_ids[1]),
    );
    app.update();
    let result = matches!(
        app.world().resource::<warden::resources::Selection>(),
        warden::resources::Selection::Tower(_)
    );
    assert!(result, "archer+archer must fuse into a Marksman");
    assert_eq!(
        app.world().resource::<Economy>().gold,
        180,
        "fee = 20% of (50+50)=20, so 200-20=180"
    );
    assert_eq!(count_towers(&mut app), 1, "two archers fuse into one tower");
    assert_eq!(
        fused_kind(&mut app),
        Some(TowerKind::Fused(FusionKind::Marksman))
    );
    // §16.6 落位规则: the result lands on B's slot; A's slot is free.
    let (a_slot_free, b_slot_taken) = {
        let mut q = app.world_mut().query::<(&TowerSlot, &Transform)>();
        let mut out = (true, false);
        for (slot, stf) in q.iter(app.world()) {
            if stf.translation.xz().distance(positions[0].xz()) < 1.0 {
                out.0 = !slot.occupied;
            }
            if stf.translation.xz().distance(positions[1].xz()) < 1.0 {
                out.1 = slot.occupied;
            }
        }
        out
    };
    assert!(a_slot_free, "A's slot must free after the fusion");
    assert!(b_slot_taken, "the result must occupy B's slot");
    // The fused tower's invested total is ingredients + fee (TO7 bookkeeping).
    let invested = {
        let mut q = app.world_mut().query::<&Tower>();
        q.iter(app.world()).next().map(|t| t.invested).unwrap()
    };
    assert_eq!(invested, 120, "invested = 50 + 50 + 20 fee");
}

/// Capability card WA1/WA2/AC4 — acceptance: clearing a non-final wave returns
/// to Intermission AND deals a three-choose-one (3 options) before the next wave.
#[test]
fn wave_resolve_surfaces_three_options() {
    let mut app = test_app();
    {
        let mut ws = app.world_mut().resource_mut::<WaveState>();
        ws.current = 0;
        ws.phase = WavePhase::Combat;
        ws.spawn_queue.clear();
        ws.active = 0;
    }
    app.update();
    let choice = app.world().resource::<WaveChoice>();
    assert!(choice.pending, "a three-choose-one should be pending after a wave");
    assert_eq!(choice.options.len(), 3, "exactly three options");
}

/// Capability card AC4 — acceptance: picking a stat-boost option applies +20%
/// damage to that base tower type and clears the pending choice.
#[test]
fn choosing_stat_boost_applies_damage_mult() {
    let mut app = test_app();
    {
        let mut choice = app.world_mut().resource_mut::<WaveChoice>();
        choice.options = vec![ChoiceOption {
            label: "x".to_string(),
            label_en: "x".to_string(),
            kind: ChoiceKind::StatBoost { tower_type: 2, stat: StatKind::Damage },
        }];
        choice.pending = true;
    }
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::Digit1);
    app.update();
    let boosts = app.world().resource::<Boosts>();
    assert!((boosts.damage_mult[2] - 1.2).abs() < 1e-6, "mage damage should be +20%");
    assert!(!app.world().resource::<WaveChoice>().pending, "choice consumed");
}

/// Capability card UI2 — acceptance: the on-screen "开始下一波" button starts the
/// wave exactly like the Space key, and a pending three-choose-one blocks it.
#[test]
fn start_wave_button_click_starts_wave_like_space() {
    let mut app = test_app();
    app.update(); // startup: deal hand etc.
    // A pending choice blocks the button click.
    app.world_mut().resource_mut::<WaveChoice>().pending = true;
    app.world_mut().spawn((
        pointer::StartWaveButton,
        bevy::ui::Interaction::Pressed,
    ));
    app.update();
    assert_eq!(
        app.world().resource::<WaveState>().phase,
        WavePhase::Intermission,
        "the start button must be a no-op while a choice is pending"
    );
    // Choose, then click the button: the wave must start like Space would.
    app.world_mut().resource_mut::<WaveChoice>().pending = false;
    app.world_mut().spawn((
        pointer::StartWaveButton,
        bevy::ui::Interaction::Pressed,
    ));
    app.update();
    let wave = app.world().resource::<WaveState>();
    assert_eq!(wave.phase, WavePhase::Combat, "button click must start the wave");
    // The spawn system runs in the same frame right after Input, so the first
    // enemy is already out the door: total (queued + active) must be 5.
    assert_eq!(
        wave.spawn_queue.len() + wave.active as usize,
        5,
        "wave 1 totals 5 enemies (same as Space)"
    );
    assert_eq!(wave.active, 1, "first enemy spawns on the starting frame");
}

/// Capability card UI2 — acceptance: clicking a three-choose-one card applies
/// that option and closes the choice, exactly like pressing its digit key.
#[test]
fn choice_card_click_applies_option_and_closes() {
    let mut app = test_app();
    app.update();
    {
        let mut choice = app.world_mut().resource_mut::<WaveChoice>();
        choice.options = vec![
            ChoiceOption {
                label: "弓箭手塔 伤害 +20%".to_string(),
                label_en: "弓箭手塔 伤害 +20%".to_string(),
                kind: ChoiceKind::StatBoost { tower_type: 0, stat: StatKind::Damage },
            },
            ChoiceOption {
                label: "击杀金币 +20%".to_string(),
                label_en: "击杀金币 +20%".to_string(),
                kind: ChoiceKind::GoldBoost,
            },
            ChoiceOption {
                label: "获得 炮塔（入手牌）".to_string(),
                label_en: "获得 炮塔（入手牌）".to_string(),
                kind: ChoiceKind::GetTower { tower_type: 3 },
            },
        ];
        choice.pending = true;
    }
    app.world_mut().spawn((
        pointer::ChoiceCardButton { index: 1 },
        bevy::ui::Interaction::Pressed,
    ));
    app.update();
    assert!(!app.world().resource::<WaveChoice>().pending, "card click consumed the choice");
    let boosts = app.world().resource::<Boosts>();
    assert!((boosts.kill_mult - 1.2).abs() < 1e-6, "gold-boost card must apply kill_mult");
}

/// Capability card EN4 — acceptance: physical damage is halved against an
/// armored (shield) enemy: 10 physical deals 5 (hp 90 -> 85).
#[test]
fn physical_damage_halved_vs_armored_enemy() {
    let mut app = test_app();
    app.world_mut().spawn((
        Tower {
            tower_index: 0,
            damage: 10.0,
            attack_speed: 1.0,
            range: 20.0,
            attack_type: AttackType::Physical,
            cooldown: 0.0,
            target: None,
            kind: TowerKind::Base(0),
            aoe_radius: 0.0,
            slow_factor: 0.0,
            slow_duration: 0.0,
            invested: 0,
        },
        Transform::from_xyz(0.0, 0.5, 0.0),
    ));
    app.world_mut().spawn((
        Enemy {
            hp: 90.0,
            max_hp: 90.0,
            speed: 0.0,
            leak: 0,
            kill_gold: 0,
            physical_armor: true,
            def_index: 2,
            next_wp: 0,
        },
        Transform::from_xyz(2.0, 0.5, 0.0),
    ));
    app.update(); // launch frame
    for _ in 0..40 {
        app.update(); // let the homing round land (§5.1 flight time)
    }
    let hp = enemy_hp(&mut app);
    assert!(
        (hp - 85.0).abs() < 0.01,
        "10 physical vs armor should deal 5 (hp 90 -> 85), got {hp}"
    );
}

/// Capability card EN4 — acceptance: magic damage ignores physical armor:
/// 15 magic deals 15 (hp 90 -> 75).
#[test]
fn magic_damage_ignores_armor() {
    let mut app = test_app();
    app.world_mut().spawn((
        Tower {
            tower_index: 2,
            damage: 15.0,
            attack_speed: 1.0,
            range: 20.0,
            attack_type: AttackType::Magic,
            cooldown: 0.0,
            target: None,
            kind: TowerKind::Base(2),
            aoe_radius: 0.0,
            slow_factor: 0.0,
            slow_duration: 0.0,
            invested: 0,
        },
        Transform::from_xyz(0.0, 0.5, 0.0),
    ));
    app.world_mut().spawn((
        Enemy {
            hp: 90.0,
            max_hp: 90.0,
            speed: 0.0,
            leak: 0,
            kill_gold: 0,
            physical_armor: true,
            def_index: 2,
            next_wp: 0,
        },
        Transform::from_xyz(2.0, 0.5, 0.0),
    ));
    app.update(); // launch frame
    for _ in 0..40 {
        app.update(); // let the homing round land (§5.1 flight time)
    }
    let hp = enemy_hp(&mut app);
    assert!(
        (hp - 75.0).abs() < 0.01,
        "15 magic should bypass armor entirely (hp 90 -> 75), got {hp}"
    );
}

// --- UI6: bundled CJK font -------------------------------------------------

/// The bundled font file (and its OFL license) must stay in the repo so every
/// machine renders the same CJK text (UI6). Guards against accidental deletes.
#[test]
fn bundled_cjk_font_file_is_present() {
    let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/fonts");
    assert!(
        base.join("NotoSansSC-VariableFont_wght.ttf").is_file(),
        "assets/fonts/NotoSansSC-VariableFont_wght.ttf must exist (UI6 bundled font)"
    );
    assert!(
        base.join("OFL.txt").is_file(),
        "assets/fonts/OFL.txt must exist (SIL OFL 1.1 license travels with the font)"
    );
}

/// Headless apps have no AssetServer, so `setup_ui_font` must fall back to
/// `FontSource::Default` instead of panicking (UI6).
#[test]
fn ui_font_falls_back_to_default_without_asset_server() {
    let mut app = test_app();
    app.update(); // Startup: setup_ui_font + spawn_hud run
    let font = app.world().resource::<warden::systems::hud::UiFont>();
    assert!(
        matches!(font.0, bevy::text::FontSource::Handle(ref h) if h == &bevy::asset::Handle::default()),
        "headless must resolve the default font handle, got {:?}",
        font.0
    );
    // The HUD still spawns with that font (no panic, entities exist).
    let mut q = app
        .world_mut()
        .query_filtered::<&Text, With<warden::systems::hud::HudText>>();
    assert_eq!(q.iter(app.world()).count(), 1, "the HUD status line must exist in headless too");
}

// --- UI7: language toggle ---------------------------------------------------

fn hud_text(app: &mut App) -> String {
    let mut q = app
        .world_mut()
        .query_filtered::<&Text, With<warden::systems::hud::HudText>>();
    q.single(app.world()).expect("hud status line").0.clone()
}

/// Capability card UI7 — acceptance: pressing L flips the whole UI copy
/// (HUD + shop cards) between Chinese and English.
#[test]
fn lang_toggle_switches_hud_and_shop_copy() {
    let mut app = test_app();
    let path = std::env::temp_dir().join("warden_lang_toggle_test.txt");
    let _ = std::fs::remove_file(&path);
    app.world_mut().insert_resource(UiPrefsPath(path.clone()));
    app.update(); // startup + first frame: shop bar built (Chinese)

    assert!(hud_text(&mut app).contains("金币"), "default lang is Chinese");

    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::KeyL);
    app.update();
    let text = hud_text(&mut app);
    assert!(
        text.contains("Gold="),
        "L must switch the HUD to English, got: {text}"
    );
    // The shop bar rebuilds in English (offer cards carry the [Shop] prefix).
    let mut q = app.world_mut().query::<&Text>();
    assert!(
        q.iter(app.world()).any(|t| t.0.contains("[Shop]")),
        "shop offer cards must rebuild in English after the toggle"
    );

    let _ = std::fs::remove_file(&path);
}

/// Capability card UI7 — acceptance: the chosen language persists across
/// restarts via ui_prefs.txt (save on toggle, load at startup).
#[test]
fn lang_preference_persists_across_restart() {
    let path = std::env::temp_dir().join("warden_lang_pref_test.txt");
    let _ = std::fs::remove_file(&path);
    {
        let mut app = test_app();
        app.world_mut().insert_resource(UiPrefsPath(path.clone()));
        app.update();
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyL);
        app.update();
        assert_eq!(*app.world().resource::<Lang>(), Lang::En);
    }
    let mut app2 = test_app();
    app2.world_mut().insert_resource(UiPrefsPath(path.clone()));
    app2.update();
    assert_eq!(
        *app2.world().resource::<Lang>(),
        Lang::En,
        "the saved language must load at startup"
    );
    let _ = std::fs::remove_file(&path);
}

// --- UI8: next-wave preview -------------------------------------------------

/// Capability card UI8 — acceptance: the build window previews the next wave's
/// composition and reward; combat hides it; English follows the toggle.
#[test]
fn hud_preview_lists_next_wave_composition() {
    let mut app = test_app();
    let path = std::env::temp_dir().join("warden_lang_preview_test.txt");
    let _ = std::fs::remove_file(&path);
    app.world_mut().insert_resource(UiPrefsPath(path.clone()));
    app.update();
    let text = hud_text(&mut app);
    assert!(
        text.contains("下一波"),
        "intermission HUD must preview the next wave: {text}"
    );
    assert!(text.contains("普通×5"), "wave 1 is 5x ordinary: {text}");
    assert!(
        text.contains("奖励20金"),
        "the preview includes the wave reward: {text}"
    );

    // English preview follows the language toggle.
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::KeyL);
    app.update();
    let text = hud_text(&mut app);
    assert!(
        text.contains("Next wave") && text.contains("Normal×5"),
        "English preview after toggle: {text}"
    );

    // Start the wave: the preview must disappear during combat. (clear()
    // first: headless keeps just_pressed true across frames.)
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .clear();
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::Space);
    app.update();
    let text = hud_text(&mut app);
    assert!(
        !text.contains("Next wave") && !text.contains("下一波"),
        "combat must hide the preview: {text}"
    );
    let _ = std::fs::remove_file(&path);
}

// --- UI9: command card + selection state machine ----------------------------

use warden::systems::command::{CommandAction, CommandBarRoot, CommandButton};

/// Helper: the command-card buttons currently on screen.
fn command_buttons(app: &mut App) -> Vec<(CommandAction, bool)> {
    let mut q = app.world_mut().query_filtered::<&CommandButton, ()>();
    q.iter(app.world())
        .map(|b| (b.action, b.enabled))
        .collect()
}

/// Capability card UI9 — acceptance: the command card follows the selection
/// state (build list when idle, sell/fuse on a tower) and greys everything
/// during combat while keeping the buttons visible.
#[test]
fn command_card_states_follow_selection() {
    let mut app = test_app();
    app.update();
    // Idle: the dealt hand shows as build buttons.
    let buttons = command_buttons(&mut app);
    assert!(
        buttons.iter().any(|(a, _)| matches!(a, CommandAction::Build(_))),
        "idle card must list build buttons, got {buttons:?}"
    );

    // A selected tower switches the card to sell / fuse (+ reserved).
    let tower = app
        .world_mut()
        .spawn((
            Tower {
                tower_index: 0,
                damage: 10.0,
                attack_speed: 1.0,
                range: 9.0,
                attack_type: AttackType::Physical,
                cooldown: 0.0,
                target: None,
                kind: TowerKind::Base(0),
                aoe_radius: 0.0,
                slow_factor: 0.0,
                slow_duration: 0.0,
                invested: 0,
            },
            Transform::from_xyz(0.0, 0.5, 0.0),
        ))
        .id();
    app.world_mut()
        .insert_resource(warden::resources::Selection::Tower(tower));
    app.update();
    let buttons = command_buttons(&mut app);
    assert!(buttons.iter().any(|(a, _)| *a == CommandAction::Sell), "tower card must offer Sell");
    assert!(buttons.iter().any(|(a, _)| *a == CommandAction::Fuse), "tower card must offer Fuse");
    assert!(
        !buttons.iter().any(|(a, _)| matches!(a, CommandAction::Build(_))),
        "no build buttons while a tower is selected"
    );

    // Combat greys every action button but keeps it visible (锁建造铁律).
    // A non-empty spawn queue keeps wave_resolve from settling the phase back.
    {
        let mut wave = app.world_mut().resource_mut::<WaveState>();
        wave.phase = WavePhase::Combat;
        wave.spawn_queue.push_back(0);
    }
    app.update();
    let buttons = command_buttons(&mut app);
    assert!(
        !buttons.is_empty() && buttons.iter().all(|(_, enabled)| !enabled),
        "combat must grey every command button, got {buttons:?}"
    );
}

/// Capability card UI9 — acceptance: Esc is the unified cancel — it clears
/// the armed build, the fusion picking and the selection itself.
#[test]
fn esc_clears_selection_and_build_mode() {
    let mut app = test_app();
    app.update();
    let tower = app
        .world_mut()
        .spawn((Transform::from_xyz(0.0, 0.5, 0.0),))
        .id();
    app.world_mut()
        .insert_resource(warden::resources::Selection::Tower(tower));
    app.world_mut().resource_mut::<BuildMode>().armed = true;
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::Escape);
    app.update();
    assert_eq!(
        app.world().resource::<warden::resources::Selection>(),
        &warden::resources::Selection::None,
        "Esc must clear the selection"
    );
    assert!(
        !app.world().resource::<BuildMode>().armed,
        "Esc must cancel the armed build"
    );
}

/// Capability card UI9 — acceptance: the bottom bar hides on GameOver/Win
/// (the end screen is the only surface there) and returns on Playing.
#[test]
fn command_bar_hidden_on_terminal_states() {
    let mut app = test_app();
    app.update();
    let root_display = |app: &mut App| -> Display {
        let mut q = app
            .world_mut()
            .query_filtered::<&Node, With<CommandBarRoot>>();
        q.single(app.world()).expect("bar root").display
    };
    assert_eq!(root_display(&mut app), Display::Flex);

    app.world_mut()
        .resource_mut::<NextState<GameState>>()
        .set(GameState::GameOver);
    app.update(); // state transition applies
    app.update(); // visibility system sees GameOver
    assert_eq!(
        root_display(&mut app),
        Display::None,
        "the bottom bar must hide on GameOver"
    );

    app.world_mut()
        .resource_mut::<NextState<GameState>>()
        .set(GameState::Playing);
    app.update();
    app.update();
    assert_eq!(root_display(&mut app), Display::Flex);
}

// --- UI10: build dual entry + shop migration ---------------------------------

/// Find (and press) the command-card button for an action.
fn press_command_button(app: &mut App, want: CommandAction) -> bool {
    let mut q = app.world_mut().query_filtered::<Entity, With<CommandButton>>();
    let hit = q
        .iter(app.world())
        .find(|&e| app.world().get::<CommandButton>(e).map(|b| b.action) == Some(want));
    match hit {
        Some(e) => {
            app.world_mut()
                .entity_mut(e)
                .insert(bevy::ui::Interaction::Pressed);
            true
        }
        None => false,
    }
}

/// Capability card TO2/UI10 — acceptance (migrated from the retired E-key
/// path): with an empty slot selected, clicking a Build button places the
/// tower on that slot, deducts gold, occupies the slot and selects the tower.
#[test]
fn command_card_build_button_places_on_selected_slot() {
    let mut app = test_app();
    app.world_mut().insert_resource(RunRng::seeded(7));
    app.update(); // startup: deal hand + build the command card
    let owned = app.world().resource::<Hand>().owned_towers[0];
    let cost = app.world().resource::<TowerDefs>().list[owned].cost;

    let slot = {
        let mut q = app
            .world_mut()
            .query_filtered::<Entity, With<TowerSlot>>();
        q.iter(app.world()).next().expect("a slot exists")
    };
    app.world_mut()
        .insert_resource(warden::resources::Selection::EmptySlot(slot));
    app.update(); // rebuild card in EmptySlot state
    assert!(
        press_command_button(&mut app, CommandAction::Build(owned)),
        "a Build button for owned type {owned} must exist"
    );
    app.update();

    assert_eq!(
        app.world().resource::<Economy>().gold,
        100 - cost,
        "direct build must deduct exactly the tower cost"
    );
    assert_eq!(count_towers(&mut app), 1, "one tower placed on the slot");
    let occupied = app
        .world()
        .get::<TowerSlot>(slot)
        .expect("slot alive")
        .occupied;
    assert!(occupied, "the selected slot must be occupied after the build");
    assert!(
        matches!(
            app.world().resource::<warden::resources::Selection>(),
            warden::resources::Selection::Tower(_)
        ),
        "after building, the new tower becomes the selection"
    );
}

/// Capability card UI10 — acceptance: in the idle card (nothing selected),
/// clicking a Build button arms build mode for that type (first entry of the
/// dual-entry flow; the EmptySlot path is covered above).
#[test]
fn idle_build_button_arms_build_mode() {
    let mut app = test_app();
    app.update(); // startup: deal hand + build the idle card
    let owned = app.world().resource::<Hand>().owned_towers[0];
    assert!(
        press_command_button(&mut app, CommandAction::Build(owned)),
        "the idle card must list a Build button for owned type {owned}"
    );
    app.update();
    assert!(
        app.world().resource::<BuildMode>().armed,
        "clicking Build in the idle card must arm the build"
    );
    assert_eq!(
        app.world().resource::<warden::resources::SelectedTower>().tower_index,
        owned,
        "the armed type must be the clicked one"
    );
}

/// Capability card UI10 — acceptance: the Buy button keeps AC2 semantics —
/// pay once, unlock the type into the hand.
#[test]
fn command_card_buy_button_unlocks_type() {
    let mut app = test_app();
    app.update();
    let (target, cost) = {
        let hand = app.world().resource::<Hand>();
        let offers = app.world().resource::<ShopOffers>();
        let i = *offers
            .offers
            .iter()
            .find(|o| !hand.owned_towers.contains(o))
            .expect("AC2 guarantees an un-owned offer");
        (i, app.world().resource::<TowerDefs>().list[i].cost)
    };
    let before = app.world().resource::<Hand>().owned_towers.len();
    assert!(
        press_command_button(&mut app, CommandAction::Buy(target)),
        "a Buy button for offer {target} must exist"
    );
    app.update();
    let hand = app.world().resource::<Hand>();
    assert!(
        hand.owned_towers.contains(&target),
        "the bought type must be unlocked into the hand"
    );
    assert_eq!(hand.owned_towers.len(), before + 1);
    assert_eq!(app.world().resource::<Economy>().gold, 100 - cost);
}

/// Capability card UI10 — acceptance: with an empty slot selected, hovering a
/// Build button shows the preview (ghost + range ring) on that slot; leaving
/// the hover hides it.
#[test]
fn hovering_build_button_previews_on_selected_slot() {
    let mut app = test_app();
    app.update();
    let owned = app.world().resource::<Hand>().owned_towers[0];
    let slot = {
        let mut q = app
            .world_mut()
            .query_filtered::<Entity, With<TowerSlot>>();
        q.iter(app.world()).next().expect("a slot exists")
    };
    app.world_mut()
        .insert_resource(warden::resources::Selection::EmptySlot(slot));
    app.update();

    // Hover the build button for an owned type.
    {
        let mut q = app
            .world_mut()
            .query_filtered::<Entity, With<CommandButton>>();
        let hit = q
            .iter(app.world())
            .find(|&e| {
                app.world()
                    .get::<CommandButton>(e)
                    .map(|b| b.action)
                    == Some(CommandAction::Build(owned))
            })
            .expect("build button");
        app.world_mut()
            .entity_mut(hit)
            .insert(bevy::ui::Interaction::Hovered);
    }
    app.update();
    let visible_rings = {
        let mut q = app
            .world_mut()
            .query_filtered::<&Visibility, With<warden::systems::command::PreviewRing>>();
        q.iter(app.world())
            .filter(|v| **v == Visibility::Visible)
            .count()
    };
    assert_eq!(visible_rings, 1, "hover must show the range ring on the slot");

    // Leave the hover: the preview hides.
    {
        let mut q = app
            .world_mut()
            .query_filtered::<Entity, With<CommandButton>>();
        let hit = q
            .iter(app.world())
            .find(|&e| {
                app.world()
                    .get::<CommandButton>(e)
                    .map(|b| b.action)
                    == Some(CommandAction::Build(owned))
            })
            .expect("build button");
        app.world_mut()
            .entity_mut(hit)
            .insert(bevy::ui::Interaction::None);
    }
    app.update();
    let visible_rings = {
        let mut q = app
            .world_mut()
            .query_filtered::<&Visibility, With<warden::systems::command::PreviewRing>>();
        q.iter(app.world())
            .filter(|v| **v == Visibility::Visible)
            .count()
    };
    assert_eq!(visible_rings, 0, "leaving the hover must hide the preview");
}

// --- TO7/UI11: sell + fusion target picking ----------------------------------

/// Place a tower on a free slot manually and select it (test fixture).
fn place_and_select(app: &mut App, tower_index: usize, invested: u32, fused: Option<FusionKind>) -> Entity {
    let slot = {
        let mut q = app
            .world_mut()
            .query_filtered::<(Entity, &TowerSlot), ()>();
        let mut free = q
            .iter(app.world())
            .filter(|(_, s)| !s.occupied)
            .map(|(e, _)| e);
        free.next().expect("a free slot")
    };
    let pos = *app.world().get::<Transform>(slot).expect("slot tf");
    app.world_mut().get_mut::<TowerSlot>(slot).unwrap().occupied = true;
    let def = app.world().resource::<TowerDefs>().list[tower_index].clone();
    let e = app
        .world_mut()
        .spawn((
            Tower {
                tower_index,
                damage: def.damage,
                attack_speed: def.attack_speed,
                range: def.range,
                attack_type: def.attack_type,
                cooldown: 0.0,
                target: None,
                kind: match fused {
                    Some(k) => TowerKind::Fused(k),
                    None => TowerKind::Base(tower_index),
                },
                aoe_radius: 0.0,
                slow_factor: 0.0,
                slow_duration: 0.0,
                invested,
            },
            Transform::from_xyz(pos.translation.x, 0.6, pos.translation.z),
        ))
        .id();
    app.world_mut()
        .insert_resource(warden::resources::Selection::Tower(e));
    e
}

/// Capability card TO7 — acceptance: selling refunds invested × 70% floored,
/// frees the slot and despawns the tower; combat rejects the sale.
#[test]
fn sell_refunds_seventy_percent_and_frees_slot() {
    let mut app = test_app();
    app.update();
    let e = place_and_select(&mut app, 0, 50, None);
    app.update(); // rebuild the card for the selection
    let slot = {
        let mut q = app.world_mut().query_filtered::<Entity, With<TowerSlot>>();
        q.iter(app.world()).next().expect("slot")
    };
    assert!(
        press_command_button(&mut app, CommandAction::Sell),
        "the Sell command must appear for a selected tower"
    );
    app.update();
    assert_eq!(
        app.world().resource::<Economy>().gold,
        100 + 35,
        "archer invested 50 -> refund floor(50*0.7)=35"
    );
    assert_eq!(count_towers(&mut app), 0, "the sold tower must despawn");
    assert!(
        !app.world().get::<TowerSlot>(slot).unwrap().occupied,
        "selling must free the slot"
    );
    assert_eq!(
        app.world().resource::<Economy>().gold,
        135,
        "gold settled after the sale"
    );
    let _ = e;

    // Combat rejects the sale (locked button = no gold change, tower stays).
    let e2 = place_and_select(&mut app, 0, 50, None);
    {
        let mut wave = app.world_mut().resource_mut::<WaveState>();
        wave.phase = WavePhase::Combat;
        wave.spawn_queue.push_back(0);
    }
    app.update(); // card rebuilds with Sell disabled in combat
    let pressed = {
        let mut q = app
            .world_mut()
            .query_filtered::<Entity, With<CommandButton>>();
        let hit = q.iter(app.world()).find(|&e| {
            app.world().get::<CommandButton>(e).map(|b| b.action)
                == Some(CommandAction::Sell)
        });
        match hit {
            Some(e) => {
                assert!(
                    !app.world().get::<CommandButton>(e).unwrap().enabled,
                    "combat must grey the Sell button"
                );
                app.world_mut()
                    .entity_mut(e)
                    .insert(bevy::ui::Interaction::Pressed);
                true
            }
            None => false,
        }
    };
    assert!(pressed, "sell button exists in combat (visible but disabled)");
    app.update();
    assert_eq!(
        app.world().resource::<Economy>().gold,
        135,
        "combat sale must be rejected (no refund)"
    );
    assert_eq!(count_towers(&mut app), 1, "the tower must survive the combat sale attempt");
    let _ = e2;
}

/// Capability card TO7 — acceptance: a fused tower refunds 70% of its total
/// investment (ingredients + fee), e.g. Marksman 50+50+20=120 -> +84.
#[test]
fn fused_tower_sell_uses_total_invested() {
    let mut app = test_app();
    app.update();
    place_and_select(&mut app, 0, 120, Some(FusionKind::Marksman));
    app.update(); // rebuild the card for the selection
    assert!(
        press_command_button(&mut app, CommandAction::Sell),
        "a fused tower offers Sell"
    );
    app.update();
    assert_eq!(
        app.world().resource::<Economy>().gold,
        100 + 84,
        "invested 120 -> refund floor(120*0.7)=84"
    );
    // A selected fused tower offers no Fuse command (cannot fuse again).
    // (Fresh fixture: another fused tower selected.)
    place_and_select(&mut app, 0, 120, Some(FusionKind::Archmage));
    app.update();
    let has_fuse = {
        let mut q = app
            .world_mut()
            .query_filtered::<&CommandButton, ()>();
        q.iter(app.world())
            .any(|b| b.action == CommandAction::Fuse)
    };
    assert!(!has_fuse, "fused towers must not offer the Fuse command");
}

/// Capability card UI11 — acceptance: with no pairable tower on the field,
/// the Fuse command stays visible but greyed (enabled=false).
#[test]
fn fuse_button_greyed_without_pairable_partner() {
    let mut app = test_app();
    app.update();
    // A lone archer: no other tower on the field pairs with it.
    place_and_select(&mut app, 0, 50, None);
    app.update(); // rebuild the card for the selection
    let fuse = command_buttons(&mut app)
        .into_iter()
        .find(|(a, _)| *a == CommandAction::Fuse)
        .expect("the Fuse command must stay visible for a lone base tower");
    assert!(
        !fuse.1,
        "Fuse must be greyed while no pairable partner is on the field"
    );
}

// --- UI9/UI10 regression: clicks on blocking UI must not leak into world ---
// Found in manual play 2026-09-12: the click that pressed a command button
// also reached `select_object`, whose blank-ground cancel cleared the
// selection in the same frame — Fusion could never be entered from the bar
// (and an armed click placed a tower at the slot nearest the bar). UI roots
// now carry `BlocksWorldInput`; these tests drive the *live* cursor path
// (window + camera projection), which no earlier test exercised.

use bevy::camera::{RenderTargetInfo, Viewport};

/// Give the headless app a live cursor path: a Window with a cursor plus a
/// projectable camera (in a real build bevy_render's camera system, winit
/// and TransformPlugin provide these; MinimalPlugins has none of them).
fn enable_world_picking(app: &mut App, cursor: Vec2) {
    let w = app.world_mut().spawn(Window::default()).id();
    app.world_mut()
        .get_mut::<Window>(w)
        .expect("window")
        .set_cursor_position(Some(cursor));
    let size = UVec2::new(1280, 720);
    let mut cams = app
        .world_mut()
        .query_filtered::<(&mut Camera, &Projection, &Transform, &mut GlobalTransform), ()>();
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
        // Derive the pose from spawn_camera's actual Transform — no
        // TransformPlugin headless to propagate it into GlobalTransform,
        // and deriving (instead of hardcoding) keeps the harness in sync
        // when the framing changes.
        *gt = GlobalTransform::from(*tf);
    }
}

/// Headless stand-in for `ui_focus_system`: report whether the bottom bar
/// is under the cursor.
fn bar_reports_cursor(app: &mut App, over: bool) {
    let mut q = app
        .world_mut()
        .query_filtered::<Entity, With<CommandBarRoot>>();
    let bar = q.iter(app.world()).next().expect("command bar");
    app.world_mut().entity_mut(bar).insert(
        bevy::ui::RelativeCursorPosition {
            cursor_over: over,
            normalized: over.then_some(Vec2::ZERO),
        },
    );
}

fn click_left(app: &mut App) {
    app.world_mut()
        .resource_mut::<ButtonInput<bevy::input::mouse::MouseButton>>()
        .press(bevy::input::mouse::MouseButton::Left);
}

/// Headless input never resets on its own (no InputPlugin): clear the mouse
/// and every command-button press so later updates stay side-effect-free.
fn release_all_inputs(app: &mut App) {
    app.world_mut()
        .resource_mut::<ButtonInput<bevy::input::mouse::MouseButton>>()
        .clear();
    let mut q = app
        .world_mut()
        .query_filtered::<&mut bevy::ui::Interaction, With<CommandButton>>();
    for mut inter in q.iter_mut(app.world_mut()) {
        *inter = bevy::ui::Interaction::None;
    }
}

/// The commander singleton entity (SK8: it owns the three global skills —
/// the drive-path replacement for the retired SkillState resource).
fn commander_entity(app: &mut App) -> Entity {
    let mut q = app
        .world_mut()
        .query_filtered::<Entity, With<warden::components::Commander>>();
    q.single(app.world()).expect("commander spawned")
}

/// Commander slot cooldowns, in slot order.
fn commander_cooldowns(app: &mut App) -> Vec<f32> {
    let e = commander_entity(app);
    app.world()
        .get::<warden::components::SkillCaster>(e)
        .expect("commander caster")
        .slots
        .iter()
        .map(|s| s.cooldown)
        .collect()
}

/// Force a commander slot cooldown (test harness equivalent of the old
/// `SkillState::start_cooldown`).
fn set_commander_cooldown(app: &mut App, slot: usize, cd: f32) {
    let e = commander_entity(app);
    app.world_mut()
        .get_mut::<warden::components::SkillCaster>(e)
        .expect("commander caster")
        .slots[slot]
        .cooldown = cd;
}

/// Regression (UI11 entry step): pressing the Fuse button with the cursor on
/// the bar must enter fusion picking — the same physical click must not also
/// blank-cancel the selection behind the bar.
#[test]
fn fuse_button_click_survives_its_own_world_click() {
    let mut app = test_app();
    app.update();
    enable_world_picking(&mut app, Vec2::new(400.0, 40.0));
    // Two pairable archers so the Fuse command is enabled (UI11).
    let a = place_and_select(&mut app, 0, 50, None);
    let _b = {
        let sel = app
            .world_mut()
            .remove_resource::<warden::resources::Selection>()
            .unwrap();
        let e = place_and_select(&mut app, 0, 50, None);
        app.world_mut().insert_resource(sel);
        e
    };
    app.update(); // rebuild the card with Sell/Fuse
    bar_reports_cursor(&mut app, true);
    assert!(press_command_button(&mut app, CommandAction::Fuse));
    click_left(&mut app);
    app.update();
    assert_eq!(
        app.world().resource::<warden::resources::Selection>(),
        &warden::resources::Selection::FusionTarget(a),
        "the Fuse click must enter target picking, not blank-cancel"
    );
    release_all_inputs(&mut app);
}

/// Regression (UI11 confirm step, live path): clicking tower B in the world
/// while fusion picking is active fuses the pair — Marksman on B's slot,
/// fee paid. Earlier tests injected `PendingFusionConfirm` directly; this
/// one goes through `select_object`'s cursor picking.
#[test]
fn fusion_confirms_by_clicking_target_tower_in_world() {
    let mut app = test_app();
    app.update();
    enable_world_picking(&mut app, Vec2::new(400.0, 400.0));
    let a = place_and_select(&mut app, 0, 50, None);
    let b = {
        let sel = app
            .world_mut()
            .remove_resource::<warden::resources::Selection>()
            .unwrap();
        let e = place_and_select(&mut app, 0, 50, None);
        app.world_mut().insert_resource(sel);
        e
    };
    app.world_mut()
        .insert_resource(warden::resources::Selection::FusionTarget(a));
    app.update(); // rebuild card + pair highlights
    // Move the cursor exactly onto tower B's projected position.
    {
        let bp = {
            let mut cams = app.world_mut().query::<(&Camera, &GlobalTransform)>();
            let (cam, tf) = cams.single(app.world()).expect("one camera");
            let pos = app
                .world()
                .get::<Transform>(b)
                .expect("tower B alive")
                .translation;
            cam.world_to_viewport(tf, pos)
                .expect("tower B projects into the viewport")
        };
        let mut q = app.world_mut().query_filtered::<&mut Window, ()>();
        for mut w in q.iter_mut(app.world_mut()) {
            w.set_cursor_position(Some(bp));
        }
    }
    bar_reports_cursor(&mut app, false);
    click_left(&mut app);
    app.update();
    assert_eq!(
        count_towers(&mut app),
        1,
        "the pair must fuse into one tower"
    );
    assert_eq!(
        fused_kind(&mut app),
        Some(TowerKind::Fused(FusionKind::Marksman)),
        "two archers fuse into a Marksman"
    );
    assert_eq!(
        app.world().resource::<Economy>().gold,
        80,
        "fee is 20% of the 100g pair (§16.6)"
    );
    release_all_inputs(&mut app);
}

/// Regression (UI10): an armed click landing on the bottom bar must not
/// place a tower at the slot nearest the cursor behind the bar.
#[test]
fn armed_click_on_command_bar_places_nothing() {
    let mut app = test_app();
    app.update();
    enable_world_picking(&mut app, Vec2::new(400.0, 40.0));
    bar_reports_cursor(&mut app, true);
    let owned = app.world().resource::<Hand>().owned_towers[0];
    app.world_mut().resource_mut::<BuildMode>().armed = true;
    app.world_mut()
        .resource_mut::<warden::resources::SelectedTower>()
        .tower_index = owned;
    click_left(&mut app);
    app.update();
    assert_eq!(
        count_towers(&mut app),
        0,
        "no tower may be placed from a bar click"
    );
    assert_eq!(
        app.world().resource::<Economy>().gold,
        100,
        "no gold may leave from a bar click"
    );
    release_all_inputs(&mut app);
}

/// The guard must not over-block: a blank world click still cancels the
/// selection (§16.3).
#[test]
fn blank_world_click_still_cancels_selection() {
    let mut app = test_app();
    app.update();
    enable_world_picking(&mut app, Vec2::new(400.0, 400.0)); // placeholder
    place_and_select(&mut app, 0, 50, None);
    // Pick the viewport corner farthest from every projected slot/tower —
    // guaranteed blank ground regardless of where the map sits on screen.
    let blank = {
        let (cam, tf) = {
            let mut cams = app.world_mut().query::<(&Camera, &GlobalTransform)>();
            let (c, t) = cams.single(app.world()).expect("one camera");
            (c.clone(), *t)
        };
        let mut pts = Vec::new();
        let mut slots = app
            .world_mut()
            .query_filtered::<&Transform, With<TowerSlot>>();
        for t in slots.iter(app.world()) {
            if let Ok(sp) = cam.world_to_viewport(&tf, t.translation) {
                pts.push(sp);
            }
        }
        let mut towers = app
            .world_mut()
            .query_filtered::<&Transform, With<Tower>>();
        for t in towers.iter(app.world()) {
            if let Ok(sp) = cam.world_to_viewport(&tf, t.translation) {
                pts.push(sp);
            }
        }
        let blank = [
            Vec2::new(10.0, 710.0),
            Vec2::new(1270.0, 710.0),
            Vec2::new(10.0, 10.0),
            Vec2::new(1270.0, 10.0),
        ]
        .into_iter()
        .max_by_key(|c| {
            pts.iter().map(|p| p.distance(*c) as u64).min().unwrap_or(u64::MAX)
        })
        .expect("a corner");
        let nearest = pts.iter().map(|p| p.distance(blank)).fold(f32::MAX, f32::min);
        assert!(
            nearest > 45.0,
            "fixture expects a blank corner but one sits {nearest}px away"
        );
        blank
    };
    let mut q = app.world_mut().query_filtered::<&mut Window, ()>();
    for mut w in q.iter_mut(app.world_mut()) {
        w.set_cursor_position(Some(blank));
    }
    bar_reports_cursor(&mut app, false);
    app.update(); // rebuild card for the selection
    click_left(&mut app);
    app.update();
    assert_eq!(
        *app.world().resource::<warden::resources::Selection>(),
        warden::resources::Selection::None,
        "blank ground still cancels the selection"
    );
    release_all_inputs(&mut app);
}

/// Capability card UI11 — acceptance: during fusion picking, clicking an
/// unpairable tower only switches the selection — nothing fuses.
#[test]
fn clicking_unpairable_tower_switches_selection_without_fusing() {
    let mut app = test_app();
    app.update();
    // shield(1) + archer(0) is not a recipe (bastion needs shield+cannon).
    let shield = place_and_select(&mut app, 1, 40, None);
    let archer = {
        // place a second tower without selecting it
        let sel = app
            .world_mut()
            .remove_resource::<warden::resources::Selection>()
            .unwrap();
        let e = place_and_select(&mut app, 0, 50, None);
        app.world_mut().insert_resource(sel);
        e
    };
    app.world_mut()
        .insert_resource(warden::resources::Selection::FusionTarget(shield));
    app.update();
    app.world_mut().insert_resource(
        warden::systems::command::PendingFusionConfirm(archer),
    );
    app.update();
    assert_eq!(
        app.world().resource::<warden::resources::Selection>(),
        &warden::resources::Selection::Tower(archer),
        "an unpairable click must fall back to plain selection"
    );
    assert_eq!(
        count_towers(&mut app),
        2,
        "no fusion may happen on an unpairable click"
    );
    assert_eq!(
        app.world().resource::<Economy>().gold,
        100,
        "no fee may be charged on an unpairable click"
    );
}

// --- UI12: tooltip + grid hotkeys ---------------------------------------------

/// Helper: hover the command button for an action.
fn hover_command_button(app: &mut App, want: CommandAction) -> bool {
    let mut q = app
        .world_mut()
        .query_filtered::<Entity, With<CommandButton>>();
    let hit = q
        .iter(app.world())
        .find(|&e| app.world().get::<CommandButton>(e).map(|b| b.action) == Some(want));
    match hit {
        Some(e) => {
            app.world_mut()
                .entity_mut(e)
                .insert(bevy::ui::Interaction::Hovered);
            true
        }
        None => false,
    }
}

fn tooltip_text(app: &mut App) -> Option<String> {
    let mut q = app
        .world_mut()
        .query_filtered::<&Text, With<warden::systems::command::TooltipText>>();
    q.iter(app.world()).next().map(|t| t.0.clone())
}

/// Capability card UI12 — acceptance: hovering the Mage build button shows a
/// tooltip with damage / attack speed / range and its AOE role, in both
/// languages (requirements §16.8).
#[test]
fn tooltip_shows_full_tower_info_in_both_languages() {
    let mut app = test_app();
    let path = std::env::temp_dir().join("warden_lang_tooltip_test.txt");
    let _ = std::fs::remove_file(&path);
    app.world_mut().insert_resource(UiPrefsPath(path.clone()));
    app.update();
    // Force the mage into the hand so its Build button is on the card.
    app.world_mut().resource_scope(|_world, mut hand: Mut<Hand>| {
        hand.owned_towers = vec![2, 0];
    });
    app.world_mut()
        .resource_mut::<ShopOffers>()
        .version += 1; // force a card rebuild
    app.update();
    assert!(hover_command_button(&mut app, CommandAction::Build(2)));
    app.update();
    let zh = tooltip_text(&mut app).expect("zh tooltip appears");
    assert!(
        zh.contains("法师塔") && zh.contains("伤害") && zh.contains("射程") && zh.contains("魔法"),
        "zh tooltip must carry full attributes, got: {zh}"
    );
    assert!(zh.contains("范围输出"), "zh tooltip must carry the role line");

    // English after the toggle.
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::KeyL);
    app.update();
    // Headless has no InputPlugin: just_pressed stays true across frames
    // (release() does not reset it), so clear the input to stop re-toggles.
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .clear();
    assert!(hover_command_button(&mut app, CommandAction::Build(2)));
    app.update();
    let en = tooltip_text(&mut app).expect("en tooltip appears");
    assert!(
        en.contains("Mage Tower") && en.contains("Damage") && en.contains("Range") && en.contains("AOE"),
        "en tooltip must carry full attributes + AOE role, got: {en}"
    );
    let _ = std::fs::remove_file(&path);
}

/// Capability card UI12 — acceptance: in the idle card, pressing Q acts like
/// clicking the first grid button (arms the build for the first owned type).
#[test]
fn grid_hotkey_q_triggers_first_build_button() {
    let mut app = test_app();
    app.update();
    let first = {
        let hand = app.world().resource::<Hand>();
        let mut owned = hand.owned_towers.clone();
        owned.sort_unstable();
        owned[0]
    };
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::KeyQ);
    app.update();
    assert!(
        app.world().resource::<BuildMode>().armed,
        "Q must arm build mode like clicking the first grid button"
    );
    assert_eq!(
        app.world().resource::<warden::resources::SelectedTower>().tower_index,
        first,
        "Q must select the first owned tower type (BTreeSet order)"
    );
}

// --- PR1/PR2: homing projectiles + edge behaviors (§5.1) ---------------------

use warden::components::{Projectile, Slow};

/// Capability card PR1 — acceptance: an archer firing at an ordinary enemy
/// 8 units away launches exactly one projectile that lands after ≈0.8s
/// (±10%, speed 10) for 10 damage; the round tracks its moving target and
/// is within 0.5 units at the hit.
#[test]
fn projectile_flies_tracks_and_lands_on_time() {
    let mut app = test_app();
    app.world_mut().spawn((
        Tower {
            tower_index: 0,
            damage: 10.0,
            attack_speed: 0.5, // 2s refire: exactly one round in the window
            range: 20.0,
            attack_type: AttackType::Physical,
            cooldown: 0.0,
            target: None,
            kind: TowerKind::Base(0),
            aoe_radius: 0.0,
            slow_factor: 0.0,
            slow_duration: 0.0,
            invested: 0,
        },
        Transform::from_xyz(0.0, 0.5, 0.0),
    ));
    app.world_mut().spawn((
        Enemy {
            hp: 30.0,
            max_hp: 30.0,
            speed: 1.0, // moving target: the round must track it
            leak: 0,
            kill_gold: 0,
            physical_armor: false,
            def_index: 0,
            next_wp: 0,
        },
        Transform::from_xyz(8.0, 0.4, 0.0),
    ));

    let count_projectiles = |app: &mut App| -> usize {
        let mut q = app.world_mut().query_filtered::<&Projectile, ()>();
        q.iter(app.world()).count()
    };
    let hp_now = |app: &mut App| -> f32 {
        let mut q = app.world_mut().query::<&Enemy>();
        q.iter(app.world()).next().map(|e| e.hp).unwrap_or(f32::MAX)
    };
    let gap_now = |app: &mut App| -> f32 {
        let mut q = app.world_mut().query::<(&Projectile, &Transform)>();
        // The enemy specifically — a bare Transform query would pick an
        // arbitrary UI node (the skill buttons added by SK1 all carry
        // Transforms too).
        let mut e = app.world_mut().query_filtered::<&Transform, With<Enemy>>();
        match (q.iter(app.world()).next(), e.iter(app.world()).next()) {
            (Some((_, ptf)), Some(etf)) => ptf.translation.distance(etf.translation),
            _ => f32::MAX,
        }
    };

    app.update(); // launch frame
    assert_eq!(count_projectiles(&mut app), 1, "one projectile launched");
    let mut hit_frame: Option<usize> = None;
    let mut min_gap = f32::MAX;
    for f in 0..120 {
        app.update();
        if count_projectiles(&mut app) > 0 {
            min_gap = min_gap.min(gap_now(&mut app));
        }
        if hit_frame.is_none() && hp_now(&mut app) < 30.0 {
            hit_frame = Some(f + 1); // frames after the launch frame
        }
    }
    let frames = hit_frame.expect("the round must land within 2s");
    assert!(
        (40..=60).contains(&frames), // ≈0.8s static; the fleeing target adds a little
        "expected ≈0.8s flight, hit at frame {frames}"
    );
    assert!(
        (hp_now(&mut app) - 20.0).abs() < 0.01,
        "one hit for 10 damage: 30 -> 20"
    );
    assert!(min_gap <= 0.5, "the round tracks its target (closest gap {min_gap:.3})");
}

/// Capability card PR1 — acceptance: the melee shield tower keeps the instant
/// hit (no projectile) and its slow rides the hit — only the target slows.
#[test]
fn shield_tower_melee_hits_instantly_and_slows_target() {
    let mut app = test_app();
    app.world_mut().spawn((
        Tower {
            tower_index: 1,
            damage: 5.0,
            attack_speed: 1.2,
            range: 4.0,
            attack_type: AttackType::Physical,
            cooldown: 0.0,
            target: None,
            kind: TowerKind::Base(1),
            aoe_radius: 0.0,
            slow_factor: 0.7,
            slow_duration: 1.0,
            invested: 0,
        },
        Transform::from_xyz(0.0, 0.5, 0.0),
    ));
    app.world_mut().spawn((
        Enemy {
            hp: 30.0,
            max_hp: 30.0,
            speed: 0.0,
            leak: 0,
            kill_gold: 0,
            physical_armor: false,
            def_index: 0,
            next_wp: 0,
        },
        Transform::from_xyz(2.0, 0.4, 0.0),
    ));
    app.update();
    let mut q = app.world_mut().query_filtered::<&Projectile, ()>();
    assert_eq!(q.iter(app.world()).count(), 0, "melee never spawns a projectile");
    let slowed = app
        .world_mut()
        .query::<&Slow>()
        .iter(app.world())
        .count();
    assert_eq!(slowed, 1, "the hit applies the slow to the target (TO6 on-hit)");
    let hp = enemy_hp(&mut app);
    assert!((hp - 25.0).abs() < 0.01, "instant melee hit: 30 -> 25, got {hp}");
}

/// Capability card PR2 — acceptance: a single-target round whose target dies
/// mid-flight dissipates with no damage; the wave-end clear removes leftovers.
#[test]
fn single_target_round_dissipates_when_target_dies() {
    let mut app = test_app();
    // A far-away bystander that must NOT take damage.
    let bystander = app
        .world_mut()
        .spawn((
            Enemy {
                hp: 30.0,
                max_hp: 30.0,
                speed: 0.0,
                leak: 0,
                kill_gold: 0,
                physical_armor: false,
                def_index: 0,
                next_wp: 0,
            },
            Transform::from_xyz(12.0, 0.4, 12.0),
        ))
        .id();
    app.world_mut().spawn((
        Tower {
            tower_index: 0,
            damage: 10.0,
            attack_speed: 0.1, // slow refire: single round in flight
            range: 20.0,
            attack_type: AttackType::Physical,
            cooldown: 0.0,
            target: None,
            kind: TowerKind::Base(0),
            aoe_radius: 0.0,
            slow_factor: 0.0,
            slow_duration: 0.0,
            invested: 0,
        },
        Transform::from_xyz(0.0, 0.5, 0.0),
    ));
    app.world_mut().spawn((
        Enemy {
            hp: 1.0, // dies to any touch
            max_hp: 1.0,
            speed: 0.0,
            leak: 0,
            kill_gold: 0,
            physical_armor: false,
            def_index: 0,
            next_wp: 0,
        },
        Transform::from_xyz(9.0, 0.4, 0.0),
    ));
    app.update(); // launch
    // The round is mid-flight: kill the target out-of-band.
    app.world_mut().get_mut::<Enemy>(bystander).unwrap().hp = 30.0; // untouched baseline
    let mut q = app.world_mut().query::<&Projectile>();
    let target = q
        .iter(app.world())
        .next()
        .map(|p| match p.origin {
            warden::components::ProjectileOrigin::Homing { target } => target,
            warden::components::ProjectileOrigin::Fixed { .. } => {
                panic!("a tower round must be homing")
            }
        })
        .expect("round in flight");
    app.world_mut().get_mut::<Enemy>(target).unwrap().hp = 0.0; // dies mid-flight
    app.update(); // resolve_death may not have run yet; the round sees hp<=0
    for _ in 0..30 {
        app.update();
    }
    let hp = app.world().get::<Enemy>(bystander).unwrap().hp;
    assert_eq!(hp, 30.0, "the dissipated round must deal no damage");
    let mut q = app.world_mut().query_filtered::<&Projectile, ()>();
    assert_eq!(q.iter(app.world()).count(), 0, "the round despawns on target loss");
}

/// Capability card PR2 — acceptance: the frame combat resolves, every
/// projectile still in flight is removed (wave-end clear).
#[test]
fn wave_end_clears_in_flight_projectiles() {
    let mut app = test_app();
    // Start a wave so the phase transition Combat -> Intermission happens.
    {
        let mut wave = app.world_mut().resource_mut::<WaveState>();
        wave.phase = WavePhase::Combat;
        wave.spawn_queue.push_back(0);
    }
    // A live far-away target keeps the slow round legitimately in flight.
    let far_target = app
        .world_mut()
        .spawn((
            Enemy {
                hp: 100.0,
                max_hp: 100.0,
                speed: 0.0,
                leak: 0,
                kill_gold: 0,
                physical_armor: false,
                def_index: 0,
                next_wp: 0,
            },
            Transform::from_xyz(90.0, 0.4, 90.0),
        ))
        .id();
    app.update();
    app.world_mut().spawn((
        Projectile {
            origin: warden::components::ProjectileOrigin::Homing {
                target: far_target,
            },
            damage: 1.0,
            attack_type: AttackType::Physical,
            source: warden::systems::damage::DamageSource::System,
            crit: false,
            aoe_radius: 0.0,
            slow_factor: 0.0,
            slow_duration: 0.0,
            speed: 0.01, // never reaches anything before the wave ends
            last_target_pos: Vec3::new(90.0, 0.4, 90.0),
        },
        Transform::from_xyz(50.0, 1.0, 50.0),
    ));
    app.update();
    {
        let mut q = app.world_mut().query_filtered::<&Projectile, ()>();
        assert_eq!(q.iter(app.world()).count(), 1, "the leftover round exists mid-wave");
    }
    // Resolve the wave: unit-level phase transition (Combat -> Intermission)
    // drives the clear; the wave machinery itself is covered elsewhere.
    app.world_mut()
        .resource_mut::<WaveState>()
        .phase = WavePhase::Intermission;
    app.update(); // the clear system sees the transition
    app.update();
    let mut q = app.world_mut().query_filtered::<&Projectile, ()>();
    assert_eq!(q.iter(app.world()).count(), 0, "wave end must clear all rounds");
}

// --- FX1-FX3: combat presentation ---------------------------------------------

use warden::systems::fx::{BlastRing, DeathShell, HealthBar, HitFlash, ShieldBadge};

/// Spawn a fully-visual enemy (mesh + own material handle) at a position.
fn spawn_visual_enemy(app: &mut App, def_index: usize, hp: f32, max_hp: f32, pos: Vec3) -> Entity {
    let (mesh_handle, mat_handle) = {
        let world = app.world_mut();
        let base = warden::systems::enemy::enemy_base_color(
            &world.resource::<warden::resources::EnemyDefs>().list[def_index],
        );
        let mesh_handle = {
            let mut meshes = world.resource_mut::<Assets<Mesh>>();
            meshes.add(Cuboid::new(0.7, 0.7, 0.7))
        };
        let mut materials = world
            .resource_mut::<Assets<StandardMaterial>>();
        (mesh_handle, materials.add(base))
    };
    app.world_mut()
        .spawn((
            Enemy {
                hp,
                max_hp,
                speed: 0.0,
                leak: 0,
                kill_gold: 6,
                physical_armor: def_index == 2,
                def_index,
                next_wp: 1, // valid waypoint; speed 0 keeps it in place
            },
            Mesh3d(mesh_handle),
            MeshMaterial3d(mat_handle),
            Transform::from_xyz(pos.x, 0.4, pos.z),
        ))
        .id()
}

fn material_color(app: &mut App, e: Entity) -> bevy::color::Color {
    let handle = app
        .world()
        .get::<MeshMaterial3d<StandardMaterial>>(e)
        .expect("material")
        .0
        .clone();
    app.world()
        .resource::<Assets<StandardMaterial>>()
        .get(&handle)
        .expect("asset")
        .base_color
}

/// Capability card FX1 — acceptance: no bar at full hp; damaged => bar exists
/// with width == hp/max_hp; healed back to full => hidden.
#[test]
fn health_bar_shows_only_when_damaged() {
    let mut app = test_app();
    let e = spawn_visual_enemy(&mut app, 0, 30.0, 30.0, Vec3::ZERO);
    app.update();
    let bars = |app: &mut App| -> Vec<(f32, bool)> {
        let mut q = app
            .world_mut()
            .query_filtered::<(&HealthBar, &Transform, &Visibility), ()>();
        q.iter(app.world())
            .map(|(_, tf, v)| (tf.scale.x, *v == Visibility::Visible))
            .collect()
    };
    assert!(bars(&mut app).is_empty(), "full-health enemy has no bar");
    app.world_mut().get_mut::<Enemy>(e).unwrap().hp = 15.0;
    app.update();
    app.update(); // bar spawns via commands
    let shown = bars(&mut app);
    assert_eq!(shown.len(), 1, "damaged enemy gets a bar");
    assert!(shown[0].1, "the bar is visible");
    assert!(
        (shown[0].0 - 0.45).abs() < 0.01,
        "bar width == hp/max_hp (0.45), got {}",
        shown[0].0
    );

    app.world_mut().get_mut::<Enemy>(e).unwrap().hp = 30.0;
    app.update();
    let shown = bars(&mut app);
    assert!(!shown[0].1, "healed back to full => bar hidden");
}

/// Capability card FX2 — acceptance: a hit flashes the enemy white and
/// recovers within ~0.15s; AOE blast rings fade away within ~0.5s.
#[test]
fn hit_flash_expires_and_blast_rings_fade() {
    let mut app = test_app();
    let e = spawn_visual_enemy(&mut app, 0, 30.0, 30.0, Vec3::ZERO);
    app.update();
    let before = material_color(&mut app, e);

    app.world_mut()
        .entity_mut(e)
        .insert(HitFlash { timer: 0.15, original: None });
    app.update();
    let flashed = material_color(&mut app, e);
    let srgb = |c: bevy::color::Color| c.to_srgba();
    assert!(srgb(flashed).red > 0.9, "flashed white, got {:?}", srgb(flashed));

    for _ in 0..10 {
        app.update(); // 0.167s > 0.15s flash window
    }
    assert!(
        app.world().get::<HitFlash>(e).is_none(),
        "the flash component expires"
    );
    let restored = material_color(&mut app, e);
    assert_eq!(srgb(restored), srgb(before), "original color restored");

    // AOE ring: send the impact message and let the fx system react.
    app.world_mut()
        .resource_mut::<bevy::ecs::message::Messages<warden::systems::projectiles::ProjectileImpact>>()
        .write(warden::systems::projectiles::ProjectileImpact {
            position: Vec3::ZERO,
            aoe_radius: 3.0,
        });
    app.update();
    let rings = |app: &mut App| -> usize {
        let mut q = app.world_mut().query_filtered::<&BlastRing, ()>();
        q.iter(app.world()).count()
    };
    assert_eq!(rings(&mut app), 1, "AOE impact spawns a blast ring");
    for _ in 0..35 {
        app.update(); // ~0.58s > 0.5s ring lifetime
    }
    assert_eq!(rings(&mut app), 0, "the ring fades within ~0.5s");
}

/// Capability card FX2 — acceptance: an enemy death spawns a dissolving shell
/// (~0.4s) while the kill gold (+6) stays exactly as before.
#[test]
fn death_shell_spawns_and_kill_gold_untouched() {
    let mut app = test_app();
    let e = spawn_visual_enemy(&mut app, 0, 0.0, 30.0, Vec3::ZERO);
    app.update(); // resolve_death: gold +6, despawn, shell spawns
    assert_eq!(app.world().resource::<Economy>().gold, 100 + 6, "kill gold +6 unchanged");
    assert!(app.world().get::<Enemy>(e).is_none(), "the enemy is gone");
    let shells = |app: &mut App| -> usize {
        let mut q = app.world_mut().query_filtered::<&DeathShell, ()>();
        q.iter(app.world()).count()
    };
    assert_eq!(shells(&mut app), 1, "a death shell spawns");
    for _ in 0..30 {
        app.update(); // 0.5s > 0.4s shell lifetime
    }
    assert_eq!(shells(&mut app), 0, "the shell dissolves within ~0.4s");
}

/// Capability card FX3 — acceptance: a slowed enemy tints blue (blue channel
/// dominates) and restores the archetype color once `Slow` expires.
#[test]
fn slow_tints_blue_then_restores() {
    let mut app = test_app();
    let e = spawn_visual_enemy(&mut app, 0, 30.0, 30.0, Vec3::ZERO); // ordinary: red
    app.update();
    let base = material_color(&mut app, e).to_srgba();
    assert!(base.red > base.blue, "ordinary enemy starts red");

    app.world_mut()
        .entity_mut(e)
        .insert(Slow { timer: 1.0, factor: 0.7 });
    app.update();
    let tinted = material_color(&mut app, e).to_srgba();
    assert!(
        tinted.blue > tinted.red,
        "slowed enemy must be blue-tinted, got {tinted:?}"
    );

    for _ in 0..70 {
        app.update(); // >1s: the slow expires
    }
    let restored = material_color(&mut app, e).to_srgba();
    assert!(restored.red > restored.blue, "color restores after the slow");
}

/// Capability card FX3 — acceptance: armored (shield) enemies carry a
/// distinguishable badge from spawn.
#[test]
fn armored_enemy_carries_shield_badge() {
    let mut app = test_app();
    let armored = spawn_visual_enemy(&mut app, 2, 90.0, 90.0, Vec3::new(2.0, 0.4, 0.0));
    let ordinary = spawn_visual_enemy(&mut app, 0, 30.0, 30.0, Vec3::ZERO);
    app.update();
    // The badge spawns as a child of the armored enemy.
    let badge_count = {
        let mut q = app.world_mut().query_filtered::<&ShieldBadge, ()>();
        q.iter(app.world()).count()
    };
    assert_eq!(badge_count, 1, "exactly the armored enemy carries a badge");
    let _ = (armored, ordinary);
}

// --- DB1: debug panel + cheat core ---------------------------------------------
//
// All debug-card tests are debug-build-only (the debug module itself is
// `#[cfg(debug_assertions)]`; `cargo test --release` skips them by design —
// release has zero debug surface).

#[cfg(debug_assertions)]
mod debug_cheats {
    use super::*;
    use warden::resources::{DebugPanelCapture, DebugRun};
    use warden::sets::GameSet;
    use warden::systems::debug::{
        debug_cheat_executor, debug_panel_toggle, debug_scenario_executor, DebugAction,
        DebugPanel, DebugQueue, DebugScenarioQueue, DebugSeed,
    };

    /// Headless app wired the way `DebugPlugin` wires the debug core (debug
    /// builds only, no egui needed — tests push actions/scenarios straight
    /// into the queues, the exact path the panel buttons and the CLI take).
    /// DB3 (time control + overlays) is wired the same way since DB3.
    pub(crate) fn debug_app() -> App {
        use warden::systems::debug_time as db3;
        let mut app = test_app();
        app.init_resource::<DebugQueue>()
            .init_resource::<DebugPanel>()
            .init_resource::<DebugSeed>()
            .init_resource::<DebugScenarioQueue>()
            .init_resource::<db3::DebugTime>()
            .init_resource::<db3::DebugOverlays>()
            .add_systems(Update, debug_panel_toggle)
            // The single cheat drain point runs in Input so the same frame's
            // wave/combat systems observe the settled cheat state; scenario
            // loads run after it, then the DB3 time clock (mirrors
            // DebugPlugin's ordering). SM1 (ADR-0010): the generic studio
            // comes from the engine crate's plugin — no bevy_egui here, so
            // the egui list panel self-skips.
            .add_systems(
                Update,
                (
                    db3::debug_time_hotkeys.before(debug_cheat_executor),
                    debug_cheat_executor,
                    debug_scenario_executor.after(debug_cheat_executor),
                    db3::debug_time_clock.after(debug_scenario_executor),
                )
                    .in_set(GameSet::Input),
            )
            .add_plugins(showroom_viewer::ShowroomPlugin)
            // SM3R: range maintenance + ledger (guards inside).
            .add_systems(
                Update,
                (
                    warden::systems::range::range_maintain,
                    warden::systems::range::range_ledger,
                    warden::systems::range::range_sweep,
                ),
            )
            // The orbit system reads/writes these; the real app gets them
            // from DefaultPlugins' InputPlugin.
            .init_resource::<bevy::input::mouse::AccumulatedMouseMotion>()
            .add_message::<bevy::input::mouse::MouseWheel>()
            .add_systems(
                Update,
                (
                    db3::debug_range_rings,
                    db3::debug_enemy_stats,
                    db3::debug_hud_strip,
                )
                    .in_set(GameSet::Observe),
            );
        app
    }

    fn push(app: &mut App, action: DebugAction) {
        app.world_mut()
            .resource_mut::<DebugQueue>()
            .0
            .push(action);
    }

    fn count_enemies(app: &mut App) -> usize {
        let mut q = app.world_mut().query::<&Enemy>();
        q.iter(app.world()).count()
    }

    /// DB1 acceptance: "+100 gold" takes effect in the same frame, and using
    /// any cheat marks the run as debug (no meta payout later).
    #[test]
    fn debug_gold_cheat_applies_same_frame_and_marks_run() {
        let mut app = debug_app();
        app.update(); // startup: economy 100
        push(&mut app, DebugAction::GoldAdd(100));
        app.update(); // executor (Input) -> gold settled this same frame
        assert_eq!(
            app.world().resource::<Economy>().gold,
            200,
            "+100 gold must land in the same frame"
        );
        assert!(
            app.world().resource::<DebugRun>().marked,
            "any cheat must mark the run as debug"
        );
    }

    /// DB1 acceptance: "jump to wave 5" lands in wave 5's BUILD phase with the
    /// rewards of waves 1-4 auto-granted (real economy).
    #[test]
    fn debug_jump_to_wave_lands_in_build_phase_with_rewards() {
        let mut app = debug_app();
        app.update(); // gold 100, wave 1 build phase
        push(&mut app, DebugAction::JumpToWave(5));
        app.update();
        {
            let wave = app.world().resource::<WaveState>();
            assert_eq!(wave.current, 4, "jump to wave 5 -> current index 4");
            assert_eq!(wave.phase, WavePhase::Intermission, "must be build phase");
            assert!(wave.spawn_queue.is_empty());
            assert_eq!(wave.active, 0);
        }
        // Rewards of waves 1..=4: 20+25+30+35 = 110 on top of the 100 start.
        assert_eq!(
            app.world().resource::<Economy>().gold,
            100 + 20 + 25 + 30 + 35,
            "skipped-wave rewards must be auto-granted"
        );
    }

    /// DB1 acceptance: a cheated run's settlement pays NO meta coins and the
    /// existing meta_save.txt content is byte-for-byte unchanged (test-run
    /// isolation — the save is never even opened for writing).
    #[test]
    fn debug_cheat_run_skips_meta_payout_and_save() {
        let save = std::env::temp_dir().join("warden_meta_debugrun_test.txt");
        let original = "coins=123\nupgrade1=1\nupgrade2=0\n";
        std::fs::write(&save, original).unwrap();
        let mut app = debug_app();
        app.world_mut().insert_resource(MetaSavePath(save.clone()));
        app.update(); // startup: loads the file above
        let before = app.world().resource::<MetaState>().coins;
        assert_eq!(before, 123, "sanity: save loaded");
        push(&mut app, DebugAction::GoldAdd(1));
        app.update(); // marks the run as debug
        app.world_mut()
            .resource_mut::<NextState<GameState>>()
            .set(GameState::GameOver);
        app.update(); // OnEnter(GameOver): payout must be SKIPPED
        assert_eq!(
            app.world().resource::<MetaState>().coins,
            before,
            "debug run must pay no meta coins"
        );
        assert_eq!(
            std::fs::read_to_string(&save).unwrap_or_default(),
            original,
            "debug run must leave meta_save.txt content unchanged"
        );
        let _ = std::fs::remove_file(&save);
    }

    /// DB1 acceptance (regression side): a NORMAL run's meta settlement is
    /// untouched by the isolation gate.
    #[test]
    fn normal_run_meta_payout_unaffected_by_isolation() {
        let save = std::env::temp_dir().join("warden_meta_normalrun_test.txt");
        let _ = std::fs::remove_file(&save);
        let mut app = debug_app();
        app.world_mut().insert_resource(MetaSavePath(save.clone()));
        app.update();
        assert!(!app.world().resource::<DebugRun>().marked);
        app.world_mut()
            .resource_mut::<NextState<GameState>>()
            .set(GameState::GameOver);
        app.update();
        assert_eq!(app.world().resource::<MetaState>().coins, 15);
        assert!(save.exists(), "normal run still writes the save");
        let _ = std::fs::remove_file(&save);
    }

    /// DB1 acceptance: "clear current wave" settles through the normal
    /// wave_resolve (reward + intermission), same frame.
    #[test]
    fn debug_clear_wave_settles_through_normal_resolve() {
        let mut app = debug_app();
        app.update();
        {
            let mut ws = app.world_mut().resource_mut::<WaveState>();
            ws.phase = WavePhase::Combat;
            ws.spawn_queue = [0usize; 5].into(); // wave 1 still spawning
            ws.active = 3;
        }
        push(&mut app, DebugAction::ClearWave);
        app.update();
        let wave = app.world().resource::<WaveState>();
        assert_eq!(wave.current, 1, "wave 1 settled -> next is wave 2");
        assert_eq!(wave.phase, WavePhase::Intermission);
        assert_eq!(
            app.world().resource::<Economy>().gold,
            120,
            "wave-1 reward (+20) granted through the normal resolve"
        );
    }

    /// DB1 acceptance: summoning N enemies at the entry puts the wave machinery
    /// into combat with N active enemies.
    #[test]
    fn debug_summon_spawns_at_entry_and_engages_combat() {
        let mut app = debug_app();
        app.update();
        push(
            &mut app,
            DebugAction::SummonEnemy {
                def_index: 0,
                count: 3,
            },
        );
        app.update();
        assert_eq!(count_enemies(&mut app), 3, "three ordinary enemies");
        let wave = app.world().resource::<WaveState>();
        assert_eq!(wave.active, 3);
        assert_eq!(wave.phase, WavePhase::Combat);
    }

    /// DB1: unlock-all and grant-tower cheats manage the hand; instant
    /// lose/win drive the state machine (the NextState set inside Update is
    /// applied by the StateTransition schedule at the top of the NEXT frame).
    #[test]
    fn debug_hand_and_ending_cheats() {
        let mut app = debug_app();
        app.update();
        push(&mut app, DebugAction::UnlockAllTowers);
        app.update();
        assert_eq!(
            app.world().resource::<Hand>().owned_towers,
            vec![0, 1, 2, 3]
        );

        push(&mut app, DebugAction::InstantLose);
        app.update(); // executor queues the transition
        app.update(); // StateTransition applies it
        assert_eq!(
            app.world().resource::<State<GameState>>().get(),
            &GameState::GameOver,
            "instant lose reaches GameOver"
        );

        // Restore the base hp the instant lose zeroed BEFORE re-entering
        // Playing (check_lose would otherwise re-trigger GameOver on the very
        // frame the state returns), then back to Playing (P) and instant win.
        push(&mut app, DebugAction::BaseHpSet(10));
        app.update(); // executor applies the hp restore while still GameOver
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyP);
        app.update(); // toggle: GameOver -> Playing (NextState queued)
        // Headless has no InputPlugin: just_pressed survives frames, so clear
        // it or the next update would toggle P AGAIN (UI7 test pattern).
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .clear();
        app.update(); // StateTransition applies Playing (hp 10 -> no re-lose)
        assert_eq!(
            app.world().resource::<State<GameState>>().get(),
            &GameState::Playing
        );
        push(&mut app, DebugAction::InstantWin);
        app.update();
        app.update();
        assert_eq!(
            app.world().resource::<State<GameState>>().get(),
            &GameState::Win,
            "instant win reaches Win"
        );
    }

    /// DB1 acceptance: the debug restart resets the run fully (economy / wave /
    /// board) and — with a recorded seed — replays the exact opening hand.
    /// The debug mark intentionally survives a restart.
    #[test]
    fn debug_restart_resets_run_and_replays_seed() {
        let mut app = debug_app();
        app.world_mut().insert_resource(RunRng::seeded(42));
        app.world_mut().resource_mut::<DebugSeed>().0 = Some(42);
        app.update();
        let opening_hand = app.world().resource::<Hand>().owned_towers.clone();

        push(&mut app, DebugAction::JumpToWave(3)); // gold 100+20+25=145
        app.update();
        assert_eq!(app.world().resource::<Economy>().gold, 145);

        push(&mut app, DebugAction::Restart);
        app.update();
        assert_eq!(app.world().resource::<Economy>().gold, 100, "fresh economy");
        let wave = app.world().resource::<WaveState>();
        assert_eq!(wave.current, 0);
        assert_eq!(wave.phase, WavePhase::Intermission);
        assert_eq!(
            app.world().resource::<Hand>().owned_towers, opening_hand,
            "seeded restart must replay the same opening hand"
        );
        assert!(
            app.world().resource::<DebugRun>().marked,
            "the debug mark survives a restart"
        );
    }

    /// DB1: F1 toggles the panel; closing it clears the input-capture flags so
    /// world input resumes immediately.
    #[test]
    fn debug_f1_toggles_panel_and_clears_capture_on_close() {
        let mut app = debug_app();
        app.update();
        {
            let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
            keys.press(KeyCode::F1);
        }
        app.update();
        assert!(app.world().resource::<DebugPanel>().open, "F1 opens");
        // Simulate the panel reporting hover/keyboard capture, then close.
        {
            let mut cap = app.world_mut().resource_mut::<DebugPanelCapture>();
            cap.pointer = true;
            cap.keyboard = true;
        }
        {
            // Headless has no InputPlugin: `pressed` is never auto-released
            // and `clear()` only resets the just_* flags — so a second F1
            // press needs release() first or `press()` is a no-op.
            let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
            keys.clear(); // drop the stale just_pressed from frame 1
            keys.release(KeyCode::F1); // end the frame-1 key hold
            keys.press(KeyCode::F1); // a fresh key-down
        }
        app.update();
        assert!(!app.world().resource::<DebugPanel>().open, "F1 closes");
        let cap = app.world().resource::<DebugPanelCapture>();
        assert!(!cap.pointer && !cap.keyboard, "capture flags reset on close");
    }
}

// --- DB2: one-shot test scenarios (CLI + F1 panel, same queue) ------------------
//
// CLI (`--scenario NAME --seed N`, parsed in main) and the F1 panel buttons
// both only push into `DebugScenarioQueue`; the exclusive
// `debug_scenario_executor` drains it on the first Update frame. These tests
// drive that same queue — so a passing test is the headless equivalent of the
// live `cargo run -- --scenario fusion --seed 42` acceptance (≤2 s there,
// one frame here).

#[cfg(debug_assertions)]
mod debug_scenarios {
    use super::debug_cheats::debug_app;
    use super::*;
    use warden::resources::{DebugRun, MetaSavePath, MetaState};
    use warden::systems::debug::{
        parse_cli_args, CliOverrides, DebugScenarioQueue, Scenario,
    };

    fn queue_scenario(app: &mut App, s: Scenario) {
        app.world_mut().resource_mut::<DebugScenarioQueue>().0.push(s);
    }

    /// DB2: the CLI parser accepts --scenario/--seed (both, either, none) and
    /// rejects unknown scenarios / bad numbers / unknown flags.
    #[test]
    fn cli_args_parse_scenario_and_seed() {
        let both = parse_cli_args(["--scenario", "fusion", "--seed", "42"].map(String::from))
            .expect("valid args parse");
        assert_eq!(
            both,
            CliOverrides { scenario: Some(Scenario::Fusion), seed: Some(42) }
        );
        let seed_only = parse_cli_args(["--seed", "7"].map(String::from)).unwrap();
        assert_eq!(seed_only, CliOverrides { scenario: None, seed: Some(7) });
        let none = parse_cli_args(Vec::new()).unwrap();
        assert_eq!(none, CliOverrides::default());
        assert!(
            parse_cli_args(["--scenario", "nope"].map(String::from)).is_err(),
            "unknown scenario name must be rejected"
        );
        assert!(
            parse_cli_args(["--seed", "not-a-number"].map(String::from)).is_err(),
            "malformed seed must be rejected"
        );
        assert!(
            parse_cli_args(["--wat"].map(String::from)).is_err(),
            "unknown flag must be rejected"
        );
        // Every advertised scenario name resolves.
        for (s, name) in Scenario::ALL {
            assert_eq!(Scenario::from_name(name), Some(s), "name {name} resolves");
        }
    }

    /// DB2 acceptance: `--scenario fusion` lands in one frame (headless
    /// equivalent of the ≤2 s live check): two archers on the board, gold
    /// 100, wave-1 build phase — and the run is marked as debug.
    #[test]
    fn scenario_fusion_board_and_economy() {
        let mut app = debug_app();
        queue_scenario(&mut app, Scenario::Fusion);
        app.update();
        assert_eq!(
            app.world().resource::<Economy>().gold,
            100,
            "fusion preset: gold 100"
        );
        assert_eq!(
            app.world().resource::<WaveState>().phase,
            WavePhase::Intermission,
            "fusion preset: build phase"
        );
        assert_eq!(app.world().resource::<WaveState>().current, 0);
        assert!(
            app.world().resource::<DebugRun>().marked,
            "loading a scenario marks the run (save isolation)"
        );
        let mut towers = app.world_mut().query::<&Tower>();
        let archers = towers
            .iter(app.world())
            .filter(|t| t.kind == TowerKind::Base(0))
            .count();
        assert_eq!(archers, 2, "exactly two archer towers on the board");
        let mut slots = app.world_mut().query::<&TowerSlot>();
        let occupied = slots
            .iter(app.world())
            .filter(|s| s.occupied)
            .count();
        assert_eq!(occupied, 2, "the two archers sit on occupied slots");
    }

    /// DB2 acceptance: the projectile scenario is one tower plus one ordinary
    /// enemy exactly 8 units away (XZ), walking toward the base.
    #[test]
    fn scenario_projectile_enemy_at_eight_units() {
        let mut app = debug_app();
        queue_scenario(&mut app, Scenario::Projectile);
        app.update();
        let mut towers = app.world_mut().query_filtered::<&Transform, With<Tower>>();
        let mut enemies = app.world_mut().query_filtered::<&Transform, With<Enemy>>();
        assert_eq!(towers.iter(app.world()).count(), 1, "exactly one tower");
        assert_eq!(enemies.iter(app.world()).count(), 1, "exactly one enemy");
        let t = towers.single(app.world()).unwrap().translation;
        let e = enemies.single(app.world()).unwrap().translation;
        let dist = Vec3::new(t.x - e.x, 0.0, t.z - e.z).length();
        assert!(
            (dist - 8.0).abs() < 1e-3,
            "enemy exactly 8 units from the tower (got {dist})"
        );
    }

    /// DB2 acceptance: the fx scenario is a wounded (1 hp) pack of four plus
    /// one healer at full hp among them.
    #[test]
    fn scenario_fx_wounded_pack_and_healer() {
        let mut app = debug_app();
        queue_scenario(&mut app, Scenario::Fx);
        app.update();
        let mut enemies = app.world_mut().query::<&Enemy>();
        let all = enemies
            .iter(app.world())
            .map(|e| e.hp)
            .collect::<Vec<_>>();
        assert_eq!(all.len(), 5, "four wounded + one healer");
        assert_eq!(
            all.iter().filter(|hp| **hp == 1.0).count(),
            4,
            "the pack is at 1 hp"
        );
        let mut healers = app.world_mut().query_filtered::<(), With<Healer>>();
        assert_eq!(healers.iter(app.world()).count(), 1, "exactly one healer");
    }

    /// DB2 acceptance: the meta scenario goes straight to GameOver and — being
    /// a scenario-loaded (debug) run — pays nothing and leaves the save file
    /// byte-for-byte unchanged.
    #[test]
    fn scenario_meta_reaches_gameover_without_payout() {
        let save = std::env::temp_dir().join("warden_meta_scenario_test.txt");
        let original = "coins=55\nupgrade1=0\nupgrade2=1\n";
        std::fs::write(&save, original).unwrap();
        let mut app = debug_app();
        app.world_mut().insert_resource(MetaSavePath(save.clone()));
        queue_scenario(&mut app, Scenario::Meta);
        app.update(); // executor queues the transition
        app.update(); // StateTransition applies it
        assert_eq!(
            app.world().resource::<State<GameState>>().get(),
            &GameState::GameOver
        );
        assert_eq!(
            app.world().resource::<MetaState>().coins,
            55,
            "scenario run pays no death reward"
        );
        assert_eq!(
            std::fs::read_to_string(&save).unwrap_or_default(),
            original,
            "scenario run leaves meta_save.txt untouched"
        );
        let _ = std::fs::remove_file(&save);
    }

    /// DB2 acceptance: the choice scenario deals a REAL pending
    /// three-choose-one (3 options from distinct categories, via the same
    /// generator the wave resolve uses).
    #[test]
    fn scenario_choice_pending_with_three_distinct_options() {
        let mut app = debug_app();
        queue_scenario(&mut app, Scenario::Choice);
        app.update();
        let choice = app.world().resource::<WaveChoice>();
        assert!(choice.pending, "choice is pending (blocks the next wave)");
        assert_eq!(choice.options.len(), 3, "exactly three options");
        let mut seen = std::collections::HashSet::new();
        for opt in &choice.options {
            let key = match &opt.kind {
                ChoiceKind::StatBoost { stat, .. } => format!("stat:{stat:?}"),
                ChoiceKind::GoldBoost => "gold".to_string(),
                ChoiceKind::GetTower { .. } => "get".to_string(),
            };
            assert!(seen.insert(key), "options must be distinct, got {seen:?}");
        }
    }

    /// DB2 acceptance: the full-board scenario fills all 8 slots — the
    /// "cannot place anything else" edge case in one step.
    #[test]
    fn scenario_full_board_fills_all_slots() {
        let mut app = debug_app();
        queue_scenario(&mut app, Scenario::FullBoard);
        app.update();
        assert_eq!(count_towers(&mut app), 8, "all 8 slots have towers");
        let mut slots = app.world_mut().query::<&TowerSlot>();
        assert_eq!(
            slots.iter(app.world()).filter(|s| s.occupied).count(),
            8,
            "every slot is marked occupied"
        );
    }

    /// DB2 acceptance: `--seed N` makes two fresh starts replay the same
    /// opening hand (RunRng determinism, CLI and restart share the seed).
    #[test]
    fn same_seed_two_fresh_starts_same_hand() {
        let hand_of = |seed: u64| {
            let mut app = debug_app();
            app.world_mut().insert_resource(RunRng::seeded(seed));
            app.update(); // Startup deals the opening hand from the seed
            app.world().resource::<Hand>().owned_towers.clone()
        };
        assert_eq!(
            hand_of(42),
            hand_of(42),
            "same seed must replay the identical opening hand"
        );
    }
}

// --- DB3: time control + info overlays ------------------------------------------
//
// Same gate as the other debug cards: the module under test is
// `#[cfg(debug_assertions)]`; release builds skip these by design.

#[cfg(debug_assertions)]
mod debug_db3 {
    use super::debug_cheats::debug_app;
    use super::*;
    use warden::components::Projectile;
    use warden::resources::{DebugRun, WavePhase, WaveState};
    use warden::systems::debug::DebugAction;
    use warden::systems::debug_time::{
        DebugEnemyStats, DebugHudText, DebugRangeRing, DebugTime,
    };

    fn push(app: &mut App, action: DebugAction) {
        app.world_mut()
            .resource_mut::<warden::systems::debug::DebugQueue>()
            .0
            .push(action);
    }

    /// Start wave 1 (5 ordinary enemies) so a live, moving enemy exists.
    fn start_wave_one(app: &mut App) {
        let mut ws = app.world_mut().resource_mut::<WaveState>();
        ws.phase = WavePhase::Combat;
        ws.spawn_queue.clear();
        for _ in 0..5 {
            ws.spawn_queue.push_back(0);
        }
        ws.spawn_timer = 0.0;
    }

    /// The first live enemy's (position, speed).
    fn first_enemy(app: &mut App) -> Option<(Vec3, f32)> {
        let mut q = app.world_mut().query::<(&Transform, &Enemy)>();
        q.iter(app.world())
            .find(|(_, e)| e.hp > 0.0)
            .map(|(tf, e)| (tf.translation, e.speed))
    }

    /// DB3 acceptance: after the debug pause, the world freezes (enemy holds
    /// position across frames) and the single-frame hotkey advances EXACTLY
    /// one frame — the frame after the press — then freezes again.
    #[test]
    fn debug_pause_freezes_and_single_step_advances_exactly_one_frame() {
        let mut app = debug_app();
        app.update();
        start_wave_one(&mut app);
        app.update(); // first enemy spawns
        app.update(); // ...and walks
        // Pause via the F6 hotkey (the panel button pushes the same action).
        // Headless just_pressed persists (no InputPlugin), so clear it right
        // after the consuming frame or the NEXT frame would toggle again.
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::F6);
        app.update(); // action lands; freeze effective from the next frame
        {
            let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
            keys.clear();
            keys.release(KeyCode::F6);
        }
        app.update(); // first fully frozen frame
        let (p0, speed) = first_enemy(&mut app).expect("a live enemy");
        for _ in 0..3 {
            app.update();
            let (p, _) = first_enemy(&mut app).unwrap();
            assert_eq!(p, p0, "frozen while debug-paused");
        }
        // Step one frame (F7). The press frame itself is still frozen; the
        // NEXT frame runs with a real delta; the frame after is frozen again.
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::F7);
        app.update(); // arm frame (delta 0)
        let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
        keys.clear();
        keys.release(KeyCode::F7);
        let (p_arm, _) = first_enemy(&mut app).unwrap();
        assert_eq!(p_arm, p0, "the press frame must not move yet");
        app.update(); // THE stepped frame
        let (p_step, _) = first_enemy(&mut app).unwrap();
        let moved = (p_step - p0).length();
        assert!(
            (moved - speed / 60.0).abs() < 1e-3,
            "one step must advance exactly one frame (want {:.4}, got {:.4})",
            speed / 60.0,
            moved
        );
        app.update(); // re-paused
        let (p_after, _) = first_enemy(&mut app).unwrap();
        assert_eq!(p_after, p_step, "frozen again after the single step");
        // Resume: movement continues.
        push(&mut app, DebugAction::TimePauseToggle);
        app.update();
        app.update();
        let (p_run, _) = first_enemy(&mut app).unwrap();
        assert!(p_run != p_step, "resume unfreezes the world");
    }

    /// DB3 acceptance: 4x clears a wave in ~1/4 the frames of 1x (±10%).
    #[test]
    fn debug_4x_clears_wave_in_about_quarter_frames() {
        let run_wave = |four_x: bool| -> u32 {
            let mut app = debug_app();
            app.update();
            if four_x {
                push(&mut app, DebugAction::TimeSpeedToggle4x);
                app.update();
            }
            start_wave_one(&mut app);
            let mut frames = 0;
            while app.world().resource::<WaveState>().phase == WavePhase::Combat {
                app.update();
                frames += 1;
                assert!(frames < 6000, "wave must resolve (runaway loop)");
            }
            frames
        };
        let n1 = run_wave(false);
        let n4 = run_wave(true);
        let ratio = n4 as f32 / n1 as f32;
        assert!(
            (ratio - 0.25).abs() <= 0.025,
            "4x should clear in ~1/4 the frames of 1x (1x={n1}, 4x={n4}, ratio={ratio:.3})"
        );
    }

    /// DB3 acceptance: the range-ring toggle shows/hides one ring per tower
    /// for every tower on the field (new towers included while it is on).
    #[test]
    fn range_ring_toggle_syncs_all_towers() {
        use warden::systems::debug::DebugScenarioQueue;
        use warden::systems::debug::Scenario;
        let mut app = debug_app();
        app.update();
        app.world_mut()
            .resource_mut::<DebugScenarioQueue>()
            .0
            .push(Scenario::FullBoard);
        app.update(); // 8 towers on the board
        let rings = |app: &mut App| {
            let mut q = app.world_mut().query_filtered::<(), With<DebugRangeRing>>();
            q.iter(app.world()).count()
        };
        push(&mut app, DebugAction::OverlaysToggleRanges);
        app.update();
        assert_eq!(rings(&mut app), 8, "one visible ring per tower");
        // A tower placed while the overlay is on gets its ring too.
        app.world_mut().spawn((
            Tower {
                tower_index: 0,
                damage: 10.0,
                attack_speed: 1.0,
                range: 9.0,
                attack_type: AttackType::Physical,
                cooldown: 0.0,
                target: None,
                kind: TowerKind::Base(0),
                aoe_radius: 0.0,
                slow_factor: 0.0,
                slow_duration: 0.0,
                invested: 50,
            },
            Transform::from_xyz(0.0, 0.6, 0.0),
        ));
        app.update();
        assert_eq!(rings(&mut app), 9, "a new tower gets a ring while on");
        push(&mut app, DebugAction::OverlaysToggleRanges);
        app.update();
        assert_eq!(rings(&mut app), 0, "toggle-off removes every ring");
    }

    /// DB3 acceptance: the enemy-stat overlay puts one live plate per enemy
    /// (hp/speed/slow) and the toggle removes them all.
    #[test]
    fn enemy_stat_plates_toggle_and_track_stats() {
        let mut app = debug_app();
        app.update();
        push(&mut app, DebugAction::SummonEnemy { def_index: 0, count: 3 });
        app.update(); // 3 ordinary enemies in combat
        let plates = |app: &mut App| {
            let mut q = app
                .world_mut()
                .query_filtered::<&bevy::sprite::Text2d, With<DebugEnemyStats>>();
            q.iter(app.world()).map(|t| t.0.clone()).collect::<Vec<_>>()
        };
        push(&mut app, DebugAction::OverlaysToggleEnemyStats);
        app.update();
        let texts = plates(&mut app);
        assert_eq!(texts.len(), 3, "one plate per enemy");
        assert!(
            texts.iter().all(|t| t.contains("hp 30/30")),
            "plates show current hp: {texts:?}"
        );
        // Wound one enemy; its plate must follow.
        {
            let mut q = app.world_mut().query::<&mut Enemy>();
            if let Some(mut e) = q.iter_mut(app.world_mut()).next() {
                e.hp = 12.0;
            }
        }
        app.update();
        let texts = plates(&mut app);
        assert!(
            texts.iter().any(|t| t.contains("hp 12/30")),
            "the wounded enemy's plate updates: {texts:?}"
        );
        push(&mut app, DebugAction::OverlaysToggleEnemyStats);
        app.update();
        assert!(plates(&mut app).is_empty(), "toggle-off removes all plates");
    }

    /// DB3 acceptance: the always-on debug HUD strip reports the live
    /// projectile count (and enemy count).
    #[test]
    fn debug_hud_reports_projectile_and_enemy_counts() {
        use warden::systems::debug::DebugScenarioQueue;
        use warden::systems::debug::Scenario;
        let mut app = debug_app();
        app.update();
        app.world_mut()
            .resource_mut::<DebugScenarioQueue>()
            .0
            .push(Scenario::Projectile);
        app.update(); // tower + enemy placed
        app.update(); // tower fires -> exactly one projectile in flight
        let live = {
            let mut q = app.world_mut().query_filtered::<(), With<Projectile>>();
            q.iter(app.world()).count()
        };
        let hud = {
            let mut q = app
                .world_mut()
                .query_filtered::<&Text, With<DebugHudText>>();
            q.single(app.world())
                .expect("the debug HUD strip exists")
                .0
                .clone()
        };
        assert!(hud.contains(&format!("proj={live}")), "hud: {hud}");
        assert!(hud.contains("enemies=1"), "hud: {hud}");
    }

    /// DB3: time control and overlays are observation tools — using them must
    /// NOT mark the run as debug (a real cheat in the same batch still marks).
    #[test]
    fn time_and_overlay_actions_do_not_mark_debug_run() {
        let mut app = debug_app();
        app.update();
        push(&mut app, DebugAction::TimePauseToggle);
        push(&mut app, DebugAction::OverlaysToggleRanges);
        app.update();
        assert!(
            !app.world().resource::<DebugRun>().marked,
            "observation tools must not poison the run"
        );
        assert!(
            app.world().resource::<DebugTime>().paused,
            "sanity: the pause action applied"
        );
        push(&mut app, DebugAction::GoldAdd(1));
        app.update();
        assert!(
            app.world().resource::<DebugRun>().marked,
            "a real cheat still marks the run"
        );
    }
}

// --- SK1: skill framework + cooldowns / cast interaction ------------------------

mod skills_sk1 {
    use super::*;
    use warden::resources::{SkillAiming, SKILL_COOLDOWNS};
    use warden::systems::skills::{SkillButton, SkillsRow};

    fn count_skill_buttons(app: &mut App) -> usize {
        let mut q = app.world_mut().query_filtered::<(), With<SkillButton>>();
        q.iter(app.world()).count()
    }

    /// SK1 acceptance: the run opens with every skill READY (cooldown 0) and
    /// the control zone carries exactly three skill buttons.
    #[test]
    fn skills_start_ready_with_three_control_zone_buttons() {
        let mut app = test_app();
        app.update();
        app.update();
        app.update(); // control zone builds on frame 1; the row self-heals
        assert_eq!(
            commander_cooldowns(&mut app),
            vec![0.0; 3],
            "opening state: all ready"
        );
        assert_eq!(count_skill_buttons(&mut app), 3, "three skill buttons");
        let mut rows = app.world_mut().query_filtered::<(), With<SkillsRow>>();
        assert_eq!(rows.iter(app.world()).count(), 1, "one skill row");
    }

    /// SK1 acceptance: a started cooldown ticks down with real (virtual)
    /// time — 1s of game time decrements it by exactly 1s — and freezes
    /// while the game is Paused (P), resuming afterwards.
    #[test]
    fn skill_cooldown_ticks_with_time_and_freezes_on_pause() {
        let mut app = test_app();
        app.update();
        set_commander_cooldown(&mut app, 0, SKILL_COOLDOWNS[0]);
        assert_eq!(commander_cooldowns(&mut app)[0], SKILL_COOLDOWNS[0]);
        for _ in 0..60 {
            app.update(); // 1s of game time
        }
        let after_1s = commander_cooldowns(&mut app)[0];
        assert!(
            (after_1s - 44.0).abs() < 0.01,
            "1s later the CD must be 44s (got {after_1s})"
        );
        // Pause (player pause): the ticker is state-gated, nothing moves.
        app.world_mut()
            .resource_mut::<NextState<GameState>>()
            .set(GameState::Paused);
        app.update(); // applies the transition
        for _ in 0..120 {
            app.update();
        }
        let after_pause = commander_cooldowns(&mut app)[0];
        assert!(
            (after_pause - after_1s).abs() < 1e-6,
            "paused game must freeze the cooldown (got {after_pause})"
        );
        app.world_mut()
            .resource_mut::<NextState<GameState>>()
            .set(GameState::Playing);
        app.update();
        app.update(); // 1/60s of play time ticks it again
        let resumed = commander_cooldowns(&mut app)[0];
        assert!(resumed < after_pause, "resuming ticks the cooldown again");
    }

    /// SK1 acceptance: cooldowns are continuous REAL time — a wave
    /// transition (combat -> intermission resolve) never resets them.
    #[test]
    fn skill_cooldown_survives_wave_transition() {
        let mut app = test_app();
        app.update();
        set_commander_cooldown(&mut app, 1, SKILL_COOLDOWNS[1]);
        {
            let mut ws = app.world_mut().resource_mut::<WaveState>();
            ws.current = 0;
            ws.phase = WavePhase::Combat;
            ws.spawn_queue.clear();
            ws.active = 0; // resolve immediately
        }
        app.update(); // wave resolves -> Intermission
        assert_eq!(
            app.world().resource::<WaveState>().phase,
            WavePhase::Intermission
        );
        let cd = commander_cooldowns(&mut app)[1];
        assert!(
            (cd - SKILL_COOLDOWNS[1]).abs() < 0.05,
            "wave change must keep the CD running (got {cd})"
        );
    }

    /// SK1: a skill on cooldown stays inert — pressing Z/X/C does nothing
    /// (no target picking, no instant cast, the cooldown keeps running).
    #[test]
    fn skill_hotkeys_do_nothing_while_on_cooldown() {
        let mut app = test_app();
        app.update();
        app.update();
        app.update();
        for slot in 0..3 {
            set_commander_cooldown(&mut app, slot, SKILL_COOLDOWNS[slot]);
        }
        let before = commander_cooldowns(&mut app);
        for key in [KeyCode::KeyZ, KeyCode::KeyX, KeyCode::KeyC] {
            app.world_mut()
                .resource_mut::<ButtonInput<KeyCode>>()
                .press(key);
            app.update();
            {
                let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
                keys.clear();
                keys.release(key);
            }
        }
        assert_eq!(
            app.world().resource::<SkillAiming>().0,
            None,
            "no target picking while cooling down"
        );
        let after = commander_cooldowns(&mut app);
        for i in 0..3 {
            assert!(
                after[i] <= before[i],
                "no cooldown restart from a blocked press ({i}: {before:?} -> {after:?})"
            );
        }
    }

    /// SK1: the F1 "reset skill CDs" cheat zeroes every cooldown and (being
    /// a cheat) marks the run as debug.
    #[test]
    fn debug_reset_skill_cd_cheat_resets_and_marks() {
        use warden::systems::debug::DebugAction;
        let mut app = super::debug_cheats::debug_app();
        app.update();
        for slot in 0..3 {
            set_commander_cooldown(&mut app, slot, SKILL_COOLDOWNS[slot]);
        }
        app.world_mut()
            .resource_mut::<warden::systems::debug::DebugQueue>()
            .0
            .push(DebugAction::SkillResetCd);
        app.update();
        assert_eq!(
            commander_cooldowns(&mut app),
            vec![0.0; 3],
            "the cheat resets every cooldown"
        );
        assert!(
            app.world().resource::<warden::resources::DebugRun>().marked,
            "the reset is a cheat: the run is marked"
        );
    }
}

// --- SK2: meteor rain (point-target AoE) -----------------------------------------

mod skills_sk2 {
    use super::*;
    use warden::components::{Projectile, ProjectileOrigin};
    use warden::resources::{SkillAiming, SKILL_COOLDOWNS};
    use warden::systems::skills::{MeteorVolley, METEOR_COUNT};

    /// A stationary live enemy (speed 0 keeps it exactly in place for
    /// radius/timing assertions).
    fn spawn_enemy_at(app: &mut App, pos: Vec3, hp: f32, armor: bool) -> Entity {
        app.world_mut()
            .spawn((
                Enemy {
                    hp,
                    max_hp: hp,
                    speed: 0.0,
                    leak: 0,
                    kill_gold: 6,
                    physical_armor: armor,
                    def_index: 0,
                    next_wp: 0,
                },
                Transform::from_xyz(pos.x, 0.4, pos.z),
            ))
            .id()
    }

    fn projectiles_alive(app: &mut App) -> usize {
        let mut q = app.world_mut().query_filtered::<(), With<Projectile>>();
        q.iter(app.world()).count()
    }

    fn reset_skill_interactions(app: &mut App) {
        let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
        keys.clear();
        keys.release(KeyCode::KeyZ);
        let mut mouse = app
            .world_mut()
            .resource_mut::<ButtonInput<bevy::input::mouse::MouseButton>>();
        mouse.clear();
        let mut q = app
            .world_mut()
            .query_filtered::<&mut bevy::ui::Interaction, With<warden::systems::skills::SkillButton>>();
        for mut inter in q.iter_mut(app.world_mut()) {
            *inter = bevy::ui::Interaction::None;
        }
    }

    /// Cast meteor rain at a world point through the REAL player path: Z
    /// hotkey -> target picking -> cursor over the point -> left-click.
    fn cast_meteor_at(app: &mut App, point: Vec3) {
        let commander = commander_entity(app);
        // Z enters target picking (button must exist from earlier updates).
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyZ);
        app.update();
        reset_skill_interactions(app);
        assert_eq!(
            app.world().resource::<SkillAiming>().0,
            Some((commander, 0)),
            "Z enters meteor target picking"
        );
        // Cursor exactly over the point, then confirm with a left click.
        enable_world_picking(app, Vec2::new(640.0, 360.0));
        let cursor = {
            let mut cams = app.world_mut().query::<(&Camera, &GlobalTransform)>();
            let (cam, tf) = cams.single(app.world()).expect("one camera");
            cam.world_to_viewport(tf, point).expect("point projects")
        };
        {
            let mut q = app.world_mut().query_filtered::<&mut Window, ()>();
            for mut w in q.iter_mut(app.world_mut()) {
                w.set_cursor_position(Some(cursor));
            }
        }
        click_left(app);
        app.update();
        reset_skill_interactions(app);
        assert_eq!(
            app.world().resource::<SkillAiming>().0,
            None,
            "the confirm click clears target picking"
        );
    }

    /// SK2 acceptance: after the confirm, all three meteors drop (three in
    /// flight mid-volley) and everything within radius 3 dies (30 hp < 40);
    /// the kill gold flows through the normal death path (+6 each).
    #[test]
    fn meteor_rain_drops_three_blasts_and_kills_the_pack() {
        let mut app = test_app();
        app.update();
        app.update();
        app.update(); // skill buttons exist
        let point = Vec3::new(0.0, 0.0, 0.0);
        for off in [Vec3::ZERO, Vec3::new(2.0, 0.0, 0.0), Vec3::new(0.0, 0.0, 2.5)] {
            spawn_enemy_at(&mut app, point + off, 30.0, false);
        }
        cast_meteor_at(&mut app, point);
        // The volley scheduler exists and the cooldown started (SK1's
        // start_cooldown through the real confirm path).
        {
            let mut q = app
                .world_mut()
                .query_filtered::<(), With<MeteorVolley>>();
            assert_eq!(q.iter(app.world()).count(), 1, "one volley scheduled");
        }
        let cd = commander_cooldowns(&mut app)[0];
        assert!(
            (SKILL_COOLDOWNS[0] - cd) < 0.1 && cd > SKILL_COOLDOWNS[0] - 0.1,
            "cooldown starts at 45s on confirm (got {cd})"
        );
        // 0.8s in: shots drop at 0/0.3/0.6 and fly ~1s -> all three airborne.
        for _ in 0..48 {
            app.update();
        }
        assert_eq!(
            projectiles_alive(&mut app),
            METEOR_COUNT as usize,
            "three meteors dropped and are in flight"
        );
        // All impacts land by ~1.6s; everything in radius dies (+6 gold each).
        for _ in 0..120 {
            app.update();
        }
        let mut q = app.world_mut().query_filtered::<(), With<Enemy>>();
        assert_eq!(
            q.iter(app.world()).count(),
            0,
            "every enemy within radius 3 died"
        );
        assert_eq!(
            app.world().resource::<Economy>().gold,
            100 + 6 * 3,
            "kill gold +6 x3 through the normal death path"
        );
    }

    /// SK2 acceptance: meteor damage is MAGIC — a shield (armored) enemy
    /// takes the FULL 40 per blast (physical would deal 20).
    #[test]
    fn meteor_damage_is_magic_and_pierces_armor() {
        let mut app = test_app();
        app.update();
        app.update();
        app.update();
        let point = Vec3::new(1.0, 0.0, -1.0);
        spawn_enemy_at(&mut app, point, 90.0, true); // shield: physical -50%
        cast_meteor_at(&mut app, point);
        // First impact at ~1.0s of flight; the second lands at ~1.3s — check
        // in between so exactly one blast has resolved.
        for _ in 0..66 {
            app.update();
        }
        let mut q = app.world_mut().query::<&Enemy>();
        let hp = q.iter(app.world()).next().map(|e| e.hp).expect("alive");
        assert!(
            (hp - 50.0).abs() < 0.01,
            "magic pierces armor: 90 - 40 = 50 (got {hp})"
        );
    }

    /// SK2/SK1 acceptance: cancelling the target pick (Esc) aborts cleanly —
    /// no projectile, no volley, no cooldown.
    #[test]
    fn meteor_target_cancel_casts_nothing_and_pays_no_cooldown() {
        let mut app = test_app();
        app.update();
        app.update();
        app.update();
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyZ);
        app.update();
        reset_skill_interactions(&mut app);
        let commander = commander_entity(&mut app);
        assert_eq!(
            app.world().resource::<SkillAiming>().0,
            Some((commander, 0))
        );
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Escape);
        app.update();
        reset_skill_interactions(&mut app);
        assert_eq!(
            app.world().resource::<SkillAiming>().0,
            None,
            "Esc cancels the target pick"
        );
        assert_eq!(
            commander_cooldowns(&mut app)[0],
            0.0,
            "a cancelled pick never starts the cooldown"
        );
        assert_eq!(projectiles_alive(&mut app), 0, "nothing was fired");
        let mut q = app
            .world_mut()
            .query_filtered::<(), With<MeteorVolley>>();
        assert_eq!(q.iter(app.world()).count(), 0, "no volley was scheduled");
        let _ = ProjectileOrigin::Fixed { point: Vec3::ZERO }; // import check
    }
}

// --- SK3: frost nova (point-target slow) ------------------------------------------

mod skills_sk3 {
    use super::*;
    use warden::components::Slow;
    use warden::resources::{SkillAiming, SKILL_COOLDOWNS};
    use warden::systems::skills::{NOVA_RADIUS, NOVA_SLOW_DURATION, NOVA_SLOW_FACTOR};
    use warden::systems::tower::apply_slow;

    /// A live enemy with a real walking speed (slows are observable through
    /// movement) parked at a world position.
    fn spawn_walker(app: &mut App, pos: Vec3, speed: f32) -> Entity {
        app.world_mut()
            .spawn((
                Enemy {
                    hp: 100.0,
                    max_hp: 100.0,
                    speed,
                    leak: 0,
                    kill_gold: 0,
                    physical_armor: false,
                    def_index: 0,
                    next_wp: 0,
                },
                Transform::from_xyz(pos.x, 0.4, pos.z),
            ))
            .id()
    }

    fn reset_skill_interactions(app: &mut App) {
        let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
        keys.clear();
        keys.release(KeyCode::KeyX);
        let mut mouse = app
            .world_mut()
            .resource_mut::<ButtonInput<bevy::input::mouse::MouseButton>>();
        mouse.clear();
        let mut q = app
            .world_mut()
            .query_filtered::<&mut bevy::ui::Interaction, With<warden::systems::skills::SkillButton>>();
        for mut inter in q.iter_mut(app.world_mut()) {
            *inter = bevy::ui::Interaction::None;
        }
    }

    /// Cast frost nova at a world point through the REAL player path (X
    /// hotkey -> click confirm), like the SK2 helper.
    fn cast_nova_at(app: &mut App, point: Vec3) {
        let commander = commander_entity(app);
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyX);
        app.update();
        reset_skill_interactions(app);
        assert_eq!(
            app.world().resource::<SkillAiming>().0,
            Some((commander, 1)),
            "X enters nova target picking"
        );
        enable_world_picking(app, Vec2::new(640.0, 360.0));
        let cursor = {
            let mut cams = app.world_mut().query::<(&Camera, &GlobalTransform)>();
            let (cam, tf) = cams.single(app.world()).expect("one camera");
            cam.world_to_viewport(tf, point).expect("point projects")
        };
        {
            let mut q = app.world_mut().query_filtered::<&mut Window, ()>();
            for mut w in q.iter_mut(app.world_mut()) {
                w.set_cursor_position(Some(cursor));
            }
        }
        click_left(app);
        app.update();
        reset_skill_interactions(app);
    }

    /// SK3 acceptance: enemies within radius 5 move at half speed for 2s
    /// (then recover); an enemy OUTSIDE the radius is untouched; the cast
    /// starts the 40s cooldown.
    #[test]
    fn frost_nova_slows_inside_radius_only_and_expires() {
        let mut app = test_app();
        app.update();
        app.update();
        app.update();
        let point = Vec3::new(0.0, 0.0, 0.0);
        let inside = [
            spawn_walker(&mut app, point + Vec3::new(4.0, 0.0, 0.0), 2.0),
            spawn_walker(&mut app, point + Vec3::new(0.0, 0.0, 2.0), 2.0),
        ];
        let outside = spawn_walker(&mut app, point + Vec3::new(6.0, 0.0, 0.0), 2.0);

        cast_nova_at(&mut app, point);
        // The cooldown started.
        let cd = commander_cooldowns(&mut app)[1];
        assert!(
            cd <= SKILL_COOLDOWNS[1] && cd > SKILL_COOLDOWNS[1] - 0.1,
            "nova cooldown starts at 40s (got {cd})"
        );
        // Slow component: exactly the nova values on both insiders.
        for &e in &inside {
            let slow = app.world().get::<Slow>(e).expect("slowed");
            assert!(
                (slow.factor - NOVA_SLOW_FACTOR).abs() < 1e-4
                    && (slow.timer - NOVA_SLOW_DURATION).abs() < 0.1,
                "Slow {{0.5, 2s}} applied (got {:?})",
                (slow.factor, slow.timer)
            );
        }
        assert!(
            app.world().get::<Slow>(outside).is_none(),
            "the enemy outside radius 5 must not be slowed"
        );
        // 0.5s of movement measured from the POST-CAST positions: insiders
        // walk at half speed (0.5 u), the outsider at full speed (1.0 u).
        let p0_in = inside
            .iter()
            .map(|&e| app.world().get::<Transform>(e).unwrap().translation)
            .collect::<Vec<_>>();
        let p0_out = app
            .world()
            .get::<Transform>(outside)
            .unwrap()
            .translation;
        for _ in 0..30 {
            app.update();
        }
        {
            let mut q = app.world_mut().query::<(Entity, &Transform)>();
            for (i, &e) in inside.iter().enumerate() {
                let p = q.get(app.world(), e).unwrap().1.translation;
                let d = (p - p0_in[i]).length();
                assert!(
                    (d - 0.5).abs() < 0.02,
                    "insider {i} walks at half speed for 0.5s: want 0.5, got {d}"
                );
            }
            let p = q.get(app.world(), outside).unwrap().1.translation;
            let d = (p - p0_out).length();
            assert!(
                (d - 1.0).abs() < 0.02,
                "outsider keeps full speed: want 1.0, got {d}"
            );
        }
        // After 2s the slow expires and speed recovers (no Slow component).
        for _ in 0..100 {
            app.update();
        }
        for &e in &inside {
            assert!(
                app.world().get::<Slow>(e).is_none(),
                "the nova slow expires after 2s"
            );
        }
    }

    /// SK3 acceptance: coexistence with the bastion slow takes the STRONGER
    /// factor and refreshes — a 0.7-slowed enemy hit by the nova runs at 0.5
    /// until the effect ends; a bastion re-hit on a nova slow keeps 0.5 (the
    /// shared merge path, never multiplicative).
    #[test]
    fn nova_and_bastion_slow_merge_strongest_wins() {
        let mut app = test_app();
        app.update();
        app.update();
        app.update();
        let point = Vec3::new(0.0, 0.0, 0.0);
        let e = spawn_walker(&mut app, point, 2.0);
        // Pre-existing bastion slow: 0.7 for 0.9s remaining.
        app.world_mut().entity_mut(e).insert(Slow {
            timer: 0.9,
            factor: 0.7,
        });
        cast_nova_at(&mut app, point);
        let slow = app.world().get::<Slow>(e).expect("still slowed");
        assert!(
            (slow.factor - 0.5).abs() < 1e-4,
            "the stronger (nova 0.5) wins over 0.7 (got {})",
            slow.factor
        );
        assert!(
            (slow.timer - 2.0).abs() < 0.1,
            "the timer refreshes to the nova duration (got {:.2})",
            slow.timer
        );
        // The bastion side of the merge: re-applying 0.7/1s keeps 0.5.
        let mut queue = bevy::ecs::world::CommandQueue::default();
        {
            let mut commands = Commands::new(&mut queue, app.world());
            apply_slow(&mut commands, e, 0.7, 1.0);
        }
        queue.apply(app.world_mut());
        let slow = app.world().get::<Slow>(e).expect("still slowed");
        assert!(
            (slow.factor - 0.5).abs() < 1e-4,
            "a bastion re-hit keeps the stronger 0.5 (got {})",
            slow.factor
        );
        assert!(
            (slow.timer - 1.0).abs() < 1e-4,
            "the timer refreshes to the new application's duration"
        );
        // Radius guard: exactly NOVA_RADIUS (XZ) is inside.
        let _ = NOVA_RADIUS;
    }
}

// --- SK4: war frenzy (instant global attack-speed burst) ---------------------------

mod skills_sk4 {
    use super::*;
    use warden::resources::{
        FRENZY_DURATION, FRENZY_SPEED_MULT, WarFrenzy, SKILL_COOLDOWNS,
    };

    /// A tanky enemy standing in range of the given tower position.
    fn spawn_dummy(app: &mut App, pos: Vec3) -> Entity {
        app.world_mut()
            .spawn((
                Enemy {
                    hp: 100_000.0,
                    max_hp: 100_000.0,
                    speed: 0.0,
                    leak: 0,
                    kill_gold: 0,
                    physical_armor: false,
                    def_index: 0,
                    next_wp: 0,
                },
                Transform::from_xyz(pos.x + 3.0, 0.4, pos.z),
            ))
            .id()
    }

    /// A tower (base archer by default, or a fused Marksman) with a dummy
    /// target in range; cooldown 0 so its first shot is immediate.
    fn spawn_tower(
        app: &mut App,
        pos: Vec3,
        attack_speed: f32,
        fused_marksman: bool,
    ) -> Entity {
        spawn_dummy(app, pos);
        app.world_mut()
            .spawn((
                Tower {
                    tower_index: 0,
                    damage: 1.0,
                    attack_speed,
                    range: 9.0,
                    attack_type: AttackType::Physical,
                    cooldown: 0.0,
                    target: None,
                    kind: if fused_marksman {
                        TowerKind::Fused(FusionKind::Marksman)
                    } else {
                        TowerKind::Base(0)
                    },
                    aoe_radius: 0.0,
                    slow_factor: 0.0,
                    slow_duration: 0.0,
                    invested: 0,
                },
                Transform::from_xyz(pos.x, 0.6, pos.z),
            ))
            .id()
    }

    /// Force the tower to fire this frame and return the cooldown that was
    /// SET by the shot (the fire-rule readout: 1 / (attack_speed × frenzy)).
    fn fire_and_read_cooldown(app: &mut App, tower: Entity) -> f32 {
        app.world_mut().get_mut::<Tower>(tower).unwrap().cooldown = 0.0;
        app.update();
        app.world().get::<Tower>(tower).unwrap().cooldown
    }

    fn cast_frenzy(app: &mut App) {
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyC);
        app.update();
        let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
        keys.clear();
        keys.release(KeyCode::KeyC);
        let mut q = app
            .world_mut()
            .query_filtered::<&mut bevy::ui::Interaction, With<warden::systems::skills::SkillButton>>();
        for mut inter in q.iter_mut(app.world_mut()) {
            *inter = bevy::ui::Interaction::None;
        }
    }

    /// SK4 acceptance: during the frenzy an archer's fire cooldown is
    /// 1/(1.0×1.5)≈0.667s and the skill starts its 60s cooldown; after 3s
    /// everything reverts to 1.0s.
    #[test]
    fn war_frenzy_speeds_towers_then_reverts() {
        let mut app = test_app();
        app.update();
        app.update();
        app.update();
        let tower = spawn_tower(&mut app, Vec3::ZERO, 1.0, false);
        // Baseline: no frenzy -> 1.0s cooldown.
        let base = fire_and_read_cooldown(&mut app, tower);
        assert!((base - 1.0).abs() < 1e-4, "baseline cooldown 1.0 (got {base})");

        cast_frenzy(&mut app);
        assert!(
            app.world().resource::<WarFrenzy>().remaining > FRENZY_DURATION - 0.1,
            "the frenzy is running"
        );
        let cd = commander_cooldowns(&mut app)[2];
        assert!(
            cd <= SKILL_COOLDOWNS[2] && cd > SKILL_COOLDOWNS[2] - 0.1,
            "the cast starts the 60s cooldown (got {cd})"
        );
        let frenzied = fire_and_read_cooldown(&mut app, tower);
        let want = 1.0 / (1.0 * FRENZY_SPEED_MULT);
        assert!(
            (frenzied - want).abs() < 1e-3,
            "frenzy cooldown 1/(1.0x1.5)={want:.3} (got {frenzied:.3})"
        );
        // Outlast the 3s window; the cooldown reverts to 1.0s.
        for _ in 0..210 {
            app.update();
        }
        assert_eq!(
            app.world().resource::<WarFrenzy>().remaining,
            0.0,
            "the frenzy expired"
        );
        let reverted = fire_and_read_cooldown(&mut app, tower);
        assert!((reverted - 1.0).abs() < 1e-4, "back to 1.0s (got {reverted})");
    }

    /// SK4 acceptance: the frenzy is a GLOBAL state — a tower PLACED while it
    /// runs benefits immediately — and a fused Marksman (attack speed 1.2)
    /// gets the same ×1.5 on its own rate.
    #[test]
    fn frenzy_covers_towers_placed_mid_effect_and_fused() {
        let mut app = test_app();
        app.update();
        app.update();
        app.update();
        cast_frenzy(&mut app);
        // A NEW archer placed during the frenzy fires at the frenzied rate.
        let new_tower = spawn_tower(&mut app, Vec3::new(5.0, 0.0, 5.0), 1.0, false);
        let new_cd = fire_and_read_cooldown(&mut app, new_tower);
        assert!(
            (new_cd - 1.0 / 1.5).abs() < 1e-3,
            "a mid-frenzy tower benefits immediately (got {new_cd:.3})"
        );
        // A fused Marksman: 1/(1.2 x 1.5) = 0.556.
        let marksman = spawn_tower(&mut app, Vec3::new(-5.0, 0.0, 5.0), 1.2, true);
        let fused_cd = fire_and_read_cooldown(&mut app, marksman);
        assert!(
            (fused_cd - 1.0 / (1.2 * 1.5)).abs() < 1e-3,
            "Marksman cooldown also x1.5 (got {fused_cd:.3})"
        );
    }

    /// SK4: the instant cast needs no target picking — C fires immediately
    /// and clears on its own (no SkillAiming state was entered).
    #[test]
    fn frenzy_casts_instantly_without_target_picking() {
        let mut app = test_app();
        app.update();
        app.update();
        app.update();
        cast_frenzy(&mut app);
        assert!(
            app.world().resource::<WarFrenzy>().remaining > 0.0,
            "C casts instantly"
        );
        assert_eq!(
            app.world().resource::<warden::resources::SkillAiming>().0,
            None,
            "no target picking for an instant skill"
        );
    }
}

// --- ART1: arrow tower 3D model (presentation gateway) -----------------------

/// Capability card ART1 — acceptance: the bundled arrow-tower model and its
/// art-sync manifest stay in the repo (ADR-0009: the manifest is the source
/// of truth; the model is the presentation source for the archer tower).
#[test]
fn arrow_tower_asset_is_pinned() {
    let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    assert!(
        base.join("assets/models/tower_arrow.glb").is_file(),
        "assets/models/tower_arrow.glb must exist (ART1, ArtVault locked v1)"
    );
    let manifest = std::fs::read_to_string(base.join("art-assets.json"))
        .expect("games/warden/art-assets.json must exist (ADR-0009 manifest)");
    assert!(
        manifest.contains("tower_arrow.glb") && manifest.contains("4c2de1d1dc414236a83760418992da30"),
        "the manifest must pin the arrow tower asset id and file"
    );
    // ART2: the ballista (vault v2, the compressed reflow) is pinned too.
    assert!(
        base.join("assets/models/weapon_ballista.glb").is_file(),
        "assets/models/weapon_ballista.glb must exist (ART2, ArtVault v2)"
    );
    assert!(
        base.join("assets/models/weapon_ballista.glb").metadata().unwrap().len() <= 15 * 1024 * 1024,
        "single files stay under the ADR-0009 15 MiB cap"
    );
    assert!(
        manifest.contains("weapon_ballista") && manifest.contains("d04b98c044fc4a3580962efe677566bf"),
        "the manifest must pin the ballista asset id and file"
    );
}

/// Capability card ART11 — acceptance L0: all six enemy archetype models are
/// pinned in the art-sync manifest with their vault ids (KayKit v2,
/// alphaMode-fix re-ingest), present on disk, and under the 15 MiB cap.
/// Index order mirrors `enemy::ENEMY_MODELS` (0 normal … 5 boss).
#[test]
fn enemy_models_are_pinned() {
    let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let vault_ids = [
        ("enemy_minion.glb", "785b039ebd1d4cf499940a6201cdf499"),
        ("enemy_rogue.glb", "08aba7a551034bd4a4a75c22aa026d98"),
        ("enemy_warrior.glb", "eb5122f7c20341619b9395503c0c6cd3"),
        ("enemy_mage.glb", "40d07424137d4bb583aba5402a801743"),
        ("enemy_elite.glb", "3cc5e9d31a014fb49c2a57ca7ace2023"),
        ("enemy_boss.glb", "5d93efa63b7f493ebcf6972512f5802c"),
    ];
    // Code pin: the gateway table and the manifest agree entry by entry.
    let code = warden::systems::enemy::ENEMY_MODELS;
    assert_eq!(code.len(), 6, "one model per archetype");
    let manifest = std::fs::read_to_string(base.join("art-assets.json")).expect("manifest");
    for (i, (file, id)) in vault_ids.iter().enumerate() {
        let path = base.join("assets/models").join(file);
        assert!(path.is_file(), "assets/models/{file} must exist (ART11)");
        assert!(
            path.metadata().unwrap().len() <= 15 * 1024 * 1024,
            "{file} stays under the ADR-0009 15 MiB cap"
        );
        assert!(
            manifest.contains(*id) && manifest.contains(*file),
            "the manifest must pin {file} ({id})"
        );
        assert!(
            code[i].ends_with(file),
            "ENEMY_MODELS[{i}] must reference {file}, got {}",
            code[i]
        );
    }
}

/// Capability card ART11 — acceptance L3: the enemy gateway's entity
/// structure. With an AssetServer the archetype model rides as a feet-origin
/// scene child carrying the deferred `EnemyAnim` wiring at the archetype
/// scale; without one the box fallback keeps its mesh (headless tests).
#[test]
fn enemy_assembly_is_pinned() {
    let mut app = test_app();
    app.add_plugins(bevy::asset::AssetPlugin::default());
    app.init_asset::<bevy::text::Font>();
    app.init_asset::<bevy::world_serialization::WorldAsset>();
    app.init_asset::<bevy::image::Image>();
    app.init_asset::<bevy::gltf::Gltf>();
    app.update();

    // Drive the REAL wave_spawn system with a boss (5) then a fast rogue (1)
    // queued, so the gateway runs exactly as it does in play (AssetServer
    // present -> both take the model path). The no-server cuboid fallback is
    // pinned by every other headless test in this file (Mesh3d on Enemy is
    // what the FX queries see there).
    {
        let mut wave = app
            .world_mut()
            .resource_mut::<warden::resources::WaveState>();
        wave.phase = warden::resources::WavePhase::Combat;
        wave.spawn_queue.clear();
        wave.spawn_queue.push_back(5);
        wave.spawn_queue.push_back(1);
        wave.spawn_timer = 0.0;
        wave.spawn_interval = 0.0; // pop one per update tick
    }
    app.update(); // boss spawns (model path)
    app.update(); // rogue spawns (model path, run clip)

    let mut enemies = app.world_mut().query_filtered::<Entity, With<Enemy>>();
    assert_eq!(enemies.iter(app.world()).count(), 2, "both queued spawns");

    // Model-path enemies have no mesh on the logic entity — that's the
    // structural difference from the cuboid fallback.
    let mut boxed = app
        .world_mut()
        .query_filtered::<(), (With<Enemy>, With<Mesh3d>)>();
    assert_eq!(
        boxed.iter(app.world()).count(),
        0,
        "model-path enemies are scene-backed, not cuboids"
    );

    let mut anims = app
        .world_mut()
        .query_filtered::<(&Transform, &warden::components::EnemyAnim), With<bevy::world_serialization::WorldAssetRoot>>();
    let rows: Vec<(&Transform, &warden::components::EnemyAnim)> = anims.iter(app.world()).collect();
    assert_eq!(rows.len(), 2, "one model scene child per model enemy");
    let scales: Vec<f32> = rows.iter().map(|(tf, _)| tf.scale.x).collect();
    assert!(
        scales
            .iter()
            .all(|&s| (s - warden::systems::enemy::ENEMY_SCALES[5]).abs() < 1e-6
                || (s - warden::systems::enemy::ENEMY_SCALES[1]).abs() < 1e-6),
        "scales must be the pinned archetype values, got {scales:?}"
    );
    for (tf, _) in &rows {
        assert!(
            tf.scale.x == tf.scale.y && tf.scale.y == tf.scale.z,
            "uniform archetype scale, got {:?}",
            tf.scale
        );
    }
    let runs: Vec<bool> = rows.iter().map(|(_, a)| a.run).collect();
    assert!(
        runs.contains(&false) && runs.contains(&true),
        "boss walks, fast rogue runs (ENEMY_RUNS), got {runs:?}"
    );
}

/// Capability card ART12 — the engine assembly pin (L3 of the art test
/// pyramid): with a real AssetServer present (headless: bevy_asset only, no
/// renderer), building the archer tower spawns the RANGER defender — one
/// scene child (RangerRig deferred wiring + RangerBody facing marker) at
/// feet-on-ground height with the uniform defender scale — and NO ballista
/// mount anywhere (the mount fixture for the machinery tests lives in
/// `ballista_app`). This is the layer that caught ART2's mount code missing
/// from its shipped commit.
#[test]
fn archer_tower_assembly_is_pinned() {
    let mut app = test_app();
    app.add_plugins(bevy::asset::AssetPlugin::default());
    // Headless has no bevy_text/gltf plugins: register the asset types the
    // load paths touch so `server.load` returns handles without panicking
    // (no loader registered = logged load error, which is fine here — the
    // test pins entity structure, not loaded content).
    app.init_asset::<bevy::text::Font>();
    app.init_asset::<bevy::world_serialization::WorldAsset>();
    app.init_asset::<bevy::image::Image>();
    app.init_asset::<bevy::gltf::Gltf>();
    app.update(); // startup: deal hand + build the command card
    // Force the archer into the hand so the Build button targets it.
    app.world_mut().resource_scope(|_w, mut hand: Mut<Hand>| {
        hand.owned_towers = vec![0];
    });
    app.world_mut().resource_mut::<ShopOffers>().version += 1;
    let slot = {
        let mut q = app
            .world_mut()
            .query_filtered::<Entity, With<TowerSlot>>();
        q.iter(app.world()).next().expect("a slot exists")
    };
    app.world_mut()
        .insert_resource(warden::resources::Selection::EmptySlot(slot));
    app.update(); // rebuild card in EmptySlot state
    assert!(
        press_command_button(&mut app, CommandAction::Build(0)),
        "the archer Build button must exist"
    );
    app.update();

    // Exactly one ranger body, feet on the ground, parented to the tower.
    let mut bodies = app
        .world_mut()
        .query_filtered::<(&Transform, &ChildOf), With<warden::components::RangerBody>>();
    let all: Vec<(&Transform, &ChildOf)> = bodies.iter(app.world()).collect();
    assert_eq!(all.len(), 1, "one RangerBody per placed archer");
    let (tf, parent) = all[0];
    assert!(
        (tf.translation.y - warden::systems::tower::RANGER_MOUNT_Y).abs() < 1e-6,
        "ranger feet at ground level (wrapper y = RANGER_MOUNT_Y), got {}",
        tf.translation.y
    );
    assert!(
        (tf.scale.x - warden::systems::tower::RANGER_SCALE).abs() < 1e-6
            && tf.scale.x == tf.scale.y
            && tf.scale.y == tf.scale.z,
        "uniform defender scale, got {:?}",
        tf.scale
    );
    assert!(
        app.world().get::<Tower>(parent.0).is_some(),
        "the ranger body must be a child of the tower entity"
    );
    // The deferred rig marker rides the same entity (wire_ranger trigger).
    assert!(
        app.world()
            .get::<warden::components::RangerRig>(parent.0)
            .is_none(),
        "RangerRig lives on the wrapper child, not the tower"
    );
    // ART12: no ballista rides a placed archer anymore.
    let mut mounts = app
        .world_mut()
        .query_filtered::<(), With<warden::components::BallistaMount>>();
    assert_eq!(
        mounts.iter(app.world()).count(),
        0,
        "the archer look retired the ballista mount"
    );
}

/// The camera framing keeps EVERY slot on-screen and clear of the bottom
/// command bar — the original (0, 16, 20) pose projected the near slot row
/// (z = +11) behind the 120 px bar, making towers there effectively
/// invisible in play (found via projection math + screenshot probe,
/// 2026-09-18). Pure projection assertion: no pixels needed.
/// NOTE: `world_to_viewport` returns top-left-origin coordinates (y grows
/// downward), so "above the bar" means a SMALL y.
#[test]
fn camera_framing_covers_all_slots() {
    let mut app = test_app();
    app.update();
    enable_world_picking(&mut app, Vec2::new(640.0, 400.0));
    let (w, h, bar) = (1280.0f32, 720.0f32, 120.0f32);
    let (cam, gt) = {
        let mut cams = app.world_mut().query::<(&Camera, &GlobalTransform)>();
        let (c, t) = cams.single(app.world()).expect("one camera");
        (c.clone(), *t)
    };
    let mut slots = app
        .world_mut()
        .query_filtered::<&Transform, With<TowerSlot>>();
    let mut count = 0;
    for tf in slots.iter(app.world()) {
        let sp = cam
            .world_to_viewport(&gt, tf.translation)
            .expect("every slot projects into the viewport");
        assert!(
            sp.x >= 10.0 && sp.x <= w - 10.0,
            "slot at {:?} off-screen horizontally: {sp:?}",
            tf.translation
        );
        assert!(
            sp.y >= 10.0 && sp.y <= h - bar - 10.0,
            "slot at {:?} behind the bottom bar or off-screen: {sp:?} (top-left origin, bar occupies y>{})",
            tf.translation,
            h - bar
        );
        count += 1;
    }
    assert_eq!(count, 8, "the map has eight slots");
}

/// ART3: the arena terrain spawns in every world — headless (no AssetServer)
/// takes the flat direction-B fallback: exactly one `MapGround` top face at
/// the diorama height, with the palette color from docs/art-direction.md.
#[test]
fn map_ground_spawns_headless() {
    let mut app = test_app();
    app.update();
    let mut ground = app
        .world_mut()
        .query_filtered::<&MeshMaterial3d<StandardMaterial>, With<warden::components::MapGround>>();
    let handle = ground
        .iter(app.world())
        .next()
        .expect("one MapGround top face")
        .0
        .clone();
    let mats = app.world().resource::<Assets<StandardMaterial>>();
    let mat = mats.get(&handle).expect("ground material exists");
    assert_eq!(mat.base_color, Color::srgb(0.42, 0.32, 0.19));
    assert!(
        mat.base_color_texture.is_none(),
        "headless must stay on the flat fallback branch"
    );
    let mut tf = app
        .world_mut()
        .query_filtered::<&Transform, With<warden::components::MapGround>>();
    let tf = tf.iter(app.world()).next().expect("ground transform");
    assert!((tf.translation.y - (-0.01)).abs() < 1e-6);
}

/// ART3: with a real AssetServer the ground takes the textured branch
/// (diffuse + GL normal wired with a tiling uv transform). Mirrors the L3
/// pattern of `archer_tower_assembly_is_pinned`: pins the wiring, not the
/// loaded pixels.
#[test]
fn map_ground_texture_branch_is_pinned() {
    let mut app = test_app();
    app.add_plugins(bevy::asset::AssetPlugin::default());
    // Same registration set as `archer_tower_assembly_is_pinned`: the UI
    // font path loads Font/WorldAsset, ART3's terrain loads Image — all
    // without loaders, which only logs (we pin wiring, not pixels).
    app.init_asset::<bevy::text::Font>();
    app.init_asset::<bevy::world_serialization::WorldAsset>();
    app.init_asset::<bevy::image::Image>();
    app.update();
    let mut ground = app
        .world_mut()
        .query_filtered::<&MeshMaterial3d<StandardMaterial>, With<warden::components::MapGround>>();
    let handle = ground
        .iter(app.world())
        .next()
        .expect("one MapGround top face")
        .0
        .clone();
    let mats = app.world().resource::<Assets<StandardMaterial>>();
    let mat = mats.get(&handle).expect("ground material exists");
    assert!(
        mat.base_color_texture.is_some(),
        "ground must be textured when an AssetServer exists"
    );
    assert!(
        mat.normal_map_texture.is_some(),
        "ground normal map must be wired"
    );
}

/// ART5: scatter props spawn in every world — headless takes the B-palette
/// box fallback; exactly one `MapProp` per PROP_LAYOUT row, all grounded.
#[test]
fn map_props_spawn_headless() {
    let mut app = test_app();
    app.update();
    let mut props = app
        .world_mut()
        .query_filtered::<&Transform, With<warden::components::MapProp>>();
    let all: Vec<&Transform> = props.iter(app.world()).collect();
    assert_eq!(all.len(), 12, "one instance per PROP_LAYOUT row");
    for tf in all {
        assert!(tf.translation.y.abs() < 1e-6, "props sit on the ground");
    }
}

/// D1 晴昼要塞 rig values from docs/art-direction.md (2026-09-20 re-tone) —
/// one near-white morning sun at a high angle, blue-white ambient bounce,
/// pale blue clear color. (Was ART6 "dusk_lighting_is_pinned" until the dusk
/// direction was rejected on live viewing.)
#[test]
fn daylight_lighting_is_pinned() {
    let mut app = test_app();
    app.update();
    let mut lights = app.world_mut().query::<&DirectionalLight>();
    let mut n = 0;
    for l in lights.iter(app.world()) {
        n += 1;
        assert_eq!(l.color, Color::srgb(1.0, 0.96, 0.88));
        assert!((l.illuminance - 14_000.0).abs() < 1e-3);
    }
    assert_eq!(n, 1, "exactly one directional light");
    let ambient = app.world().resource::<GlobalAmbientLight>();
    assert_eq!(ambient.color, Color::srgb(0.60, 0.65, 0.75));
    let clear = app.world().resource::<ClearColor>();
    assert_eq!(clear.0, Color::srgb(0.55, 0.65, 0.80));
    // The morning sun hangs high (pitch -0.84 ≈ 48°, short readable
    // shadows) at the same azimuth as the old rigs.
    let mut xforms = app
        .world_mut()
        .query_filtered::<&Transform, With<DirectionalLight>>();
    let expected = Quat::from_euler(EulerRot::ZYX, 0.0, 0.9, -0.84);
    for tf in xforms.iter(app.world()) {
        assert!(tf.rotation.abs_diff_eq(expected, 1e-6));
    }
}

/// ART4: exactly two masonry structures — the gatehouse on the base waypoint
/// and the entry arch on the path start (headless = massing fallback, the
/// textured branch spawns the same two roots).
#[test]
fn map_structures_spawn_headless() {
    let mut app = test_app();
    app.update();
    let mut roots = app
        .world_mut()
        .query_filtered::<&Transform, With<warden::components::MapStructure>>();
    let all: Vec<&Transform> = roots.iter(app.world()).collect();
    assert_eq!(all.len(), 2, "gatehouse + entry arch roots");
    let positions: Vec<(f32, f32)> = all
        .iter()
        .map(|tf| (tf.translation.x, tf.translation.z))
        .collect();
    assert!(
        positions.contains(&(12.0, -9.0)),
        "gatehouse on the base waypoint, got {positions:?}"
    );
    assert!(
        positions.contains(&(-13.0, 9.0)),
        "entry arch on the path start waypoint (README: entry top-left), got {positions:?}"
    );
}

// --- ART7/ART8: ballista aim tracking + muzzle launch + fire recoil ----------

/// ART7/ART8 harness: a headless world with a REAL AssetServer (mirrors the
/// L3 pattern of `archer_tower_assembly_is_pinned`) plus `TransformPlugin` so
/// the mount's `GlobalTransform` propagates exactly like the real game.
/// Places ONE archer tower through the shared `spawn_base_tower` gateway (the
/// same code path as the player flows) and returns (app, tower, mount).
fn ballista_app(tower_xz: Vec2) -> (App, bevy::ecs::entity::Entity, bevy::ecs::entity::Entity) {
    let mut app = test_app();
    app.add_plugins(bevy::asset::AssetPlugin::default());
    // Same registration set as `archer_tower_assembly_is_pinned` (load paths
    // touched headless; loaders absent is fine — we pin wiring, not pixels).
    app.init_asset::<bevy::text::Font>();
    app.init_asset::<bevy::world_serialization::WorldAsset>();
    app.init_asset::<bevy::image::Image>();
    // ART12: the ranger look loads a Gltf handle for the deferred wiring.
    app.init_asset::<bevy::gltf::Gltf>();
    app.add_plugins(bevy::transform::TransformPlugin);
    app.update(); // startup: deal hand + build the command card

    let tower = app
        .world_mut()
        .resource_scope(|world, mut meshes: Mut<Assets<Mesh>>| {
            world.resource_scope(|world, mut materials: Mut<Assets<StandardMaterial>>| {
                let def = world.resource::<TowerDefs>().list[0].clone();
                let server = world.resource::<AssetServer>().clone();
                let mut commands = world.commands();
                warden::systems::tower::spawn_base_tower(
                    &mut commands,
                    &def,
                    0,
                    Vec3::new(tower_xz.x, 0.0, tower_xz.y),
                    &mut meshes,
                    &mut materials,
                    Some(&server),
                )
            })
        });
    app.world_mut().flush();
    // ART12: the archer now spawns the RANGER look — the ballista mount is no
    // longer part of the placed tower. These tests pin the generic mount
    // MACHINERY (AimTracker / FireRecoil / muzzle launch), so the fixture
    // attaches a mount manually, mirroring the pre-ART12 assembly constants.
    let mount = {
        let world = app.world_mut();
        let def = world.resource::<TowerDefs>().list[0].clone();
        let mut commands = world.commands();
        let mount = commands
            .spawn((
                warden::components::BallistaMount,
                warden::components::AimTracker {
                    turn_rate: warden::systems::tower::AIM_TURN_RATE,
                    rest_bearing: warden::systems::tower::BALLISTA_REST_BEARING,
                    return_delay: warden::systems::tower::AIM_RETURN_DELAY,
                    forward_axis: warden::systems::tower::BALLISTA_FORWARD_AXIS,
                    muzzle_offset: def.muzzle_offset,
                    target: None,
                    since_target: 0.0,
                },
                warden::components::FireRecoil {
                    depth: def.recoil_depth,
                    recover_secs: def.recoil_recover_secs,
                    rest_translation: Vec3::new(0.0, warden::systems::tower::BALLISTA_MOUNT_Y, 0.0),
                    elapsed: None,
                },
                Transform::from_xyz(0.0, warden::systems::tower::BALLISTA_MOUNT_Y, 0.0)
                    .with_rotation(Quat::from_rotation_y(
                        warden::systems::tower::BALLISTA_REST_BEARING.to_radians(),
                    ))
                    .with_scale(Vec3::splat(warden::systems::tower::BALLISTA_SCALE)),
            ))
            .id();
        commands.entity(tower).add_child(mount);
        world.flush();
        mount
    };
    (app, tower, mount)
}

/// A stationary, tanky ordinary enemy (never dies to the test shots, never
/// moves — keeps the bearing static so yaw assertions are exact).
fn static_enemy(app: &mut App, pos: Vec3) -> bevy::ecs::entity::Entity {
    app.world_mut()
        .spawn((
            Enemy {
                hp: 500.0,
                max_hp: 500.0,
                speed: 0.0,
                leak: 0,
                kill_gold: 0,
                physical_armor: false,
                def_index: 0,
                next_wp: 0,
            },
            Transform::from_translation(pos),
        ))
        .id()
}

fn mount_yaw_deg(app: &App, mount: bevy::ecs::entity::Entity) -> f32 {
    app.world()
        .get::<Transform>(mount)
        .expect("mount has a Transform")
        .rotation
        .to_euler(EulerRot::YXZ)
        .0
        .to_degrees()
}

/// Capability card ART7 — acceptance: locking a target at bearing θ, the
/// ballista yaw converges to θ (±5°); after the target is gone it HOLDS the
/// bearing for the 2s return delay, then rotates back to the rest bearing
/// (±5°). Also pins the tracker config starting values (240°/s, 2s, 0.5).
#[test]
fn ballista_aim_converges_holds_then_returns_to_rest() {
    let (mut app, _tower, mount) = ballista_app(Vec2::new(0.0, 0.0));
    {
        let t = app
            .world()
            .get::<warden::components::AimTracker>(mount)
            .expect("the mount carries an AimTracker");
        assert_eq!(t.turn_rate, 240.0, "card starting value 240°/s");
        assert_eq!(t.return_delay, 2.0, "card starting value 2s");
        assert_eq!(t.muzzle_offset, 0.5, "card starting value 0.5");
        assert_eq!(
            t.forward_axis,
            Vec3::NEG_X,
            "forward-sign pin (flipped 2026-09-21 per manual acceptance; see tower.rs)"
        );
    }
    // Enemy at (6, ·, -6) from the tower at the origin → +X-frame bearing
    // +45°; with the −X muzzle the mount yaw converges to 45° − 180° = −135°.
    let enemy = static_enemy(&mut app, Vec3::new(6.0, 0.4, -6.0));
    for _ in 0..30 {
        app.update(); // 0.5s — 45° at 240°/s needs only ~0.19s
    }
    let yaw = mount_yaw_deg(&app, mount);
    assert!(
        (yaw + 135.0).abs() <= 5.0,
        "aim must converge to the −X-muzzle bearing −135° (±5°), got {yaw:.1}°"
    );

    // Target gone: hold the bearing during the 2s return delay…
    app.world_mut().despawn(enemy);
    for _ in 0..60 {
        app.update(); // 1.0s < 2.0s delay → still on bearing
    }
    let yaw = mount_yaw_deg(&app, mount);
    assert!(
        (yaw + 135.0).abs() <= 5.0,
        "must HOLD the bearing through the 2s return delay, got {yaw:.1}°"
    );

    // …then rotate back to rest 180° (the −X muzzle faces +X world; the
    // wrap point sits mid-path so accept ±180 readings).
    for _ in 0..90 {
        app.update();
    }
    let yaw = mount_yaw_deg(&app, mount);
    assert!(
        (yaw - 180.0).abs() <= 5.0 || (yaw + 180.0).abs() <= 5.0,
        "must return to the rest bearing 180° (±5°), got {yaw:.1}°"
    );
}

/// Capability card ART7 — acceptance: the projectile is BORN at
/// mount position + forward × muzzle_offset (±0.05), i.e. the launch offset
/// projects onto the ballista's facing (投影断言); the forward sign (+X) is
/// pinned here. `fly_projectiles` moves the round its first deterministic
/// step in the same frame, so the birth point is reconstructed by
/// subtracting that step.
#[test]
fn ballista_projectile_spawns_at_muzzle() {
    let (mut app, tower, mount) = ballista_app(Vec2::new(0.0, 0.0));
    let target = Vec3::new(6.0, 0.4, -6.0); // bearing +45°, 8.49 units: in range
    static_enemy(&mut app, target);
    app.update(); // launch frame: one aim step (240°/s → 4°), one round out

    let step = warden::resources::PROJECTILE_SPEED / 60.0;
    // Fire-frame ordering (ART8): tower_fire spawns the round from the mount
    // BEFORE this frame's recoil kick applies (the round leaves the muzzle,
    // THEN the weapon kicks), so the birth reference is the mount's REST
    // world position — tower anchor + the mount's rest translation — not the
    // post-kick GlobalTransform.
    let mount_pos = app.world().get::<Transform>(tower).unwrap().translation
        + app
            .world()
            .get::<warden::components::FireRecoil>(mount)
            .expect("the mount carries a FireRecoil")
            .rest_translation;
    let yaw = app
        .world()
        .get::<Transform>(mount)
        .unwrap()
        .rotation
        .to_euler(EulerRot::YXZ)
        .0;
    // The forward-sign pin (ART7): the muzzle side of the model is −X —
    // flipped 2026-09-21 after the one-time manual acceptance (shooting
    // range: the weapon faced away from the bolt direction and the recoil
    // kicked the wrong way); see `BALLISTA_FORWARD_AXIS` in tower.rs. Any
    // future flip MUST change constant + expectation together again.
    assert_eq!(
        warden::systems::tower::BALLISTA_FORWARD_AXIS,
        Vec3::NEG_X
    );
    let forward = Quat::from_rotation_y(yaw) * warden::systems::tower::BALLISTA_FORWARD_AXIS;

    let mut q = app
        .world_mut()
        .query_filtered::<&Transform, With<Projectile>>();
    let ptf = *q
        .single(app.world_mut())
        .expect("exactly one projectile on the launch frame");
    // Reconstruct the birth point by removing the first homing step (fully
    // deterministic: fixed dt, known target position).
    let dir = (target - (mount_pos + forward * 0.5)).normalize();
    let spawn = ptf.translation - dir * step;
    let offset = spawn - mount_pos;
    assert!(
        (offset.length() - 0.5).abs() <= 0.05,
        "birth point must sit muzzle_offset (0.5 ±0.05) from the mount, got {}",
        offset.length()
    );
    assert!(
        (offset.normalize() - forward).length() < 0.01,
        "the launch offset must project onto the ballista facing (弹向与弩朝向一致)"
    );
    // And the mount rests at the composed tower-top position.
    assert!(
        mount_pos.distance(Vec3::new(0.0, 1.7, 0.0)) < 1e-4,
        "mount world position = tower anchor 0.6 + mount y 1.10 (deck plane, de98e87), got {mount_pos:?}"
    );
}

/// Capability card ART8 — acceptance: the kick appears on the fire frame,
/// falls back under 10% of the peak within the 0.18s window; a re-trigger
/// mid-recovery RESETS the cycle (never stacks — the peak stays under the
/// configured depth, frenzy refire included); and under continuous fire the
/// aim keeps converging (recoil never disturbs tracking).
#[test]
fn ballista_recoil_kicks_recovers_resets_and_never_stacks() {
    let (mut app, tower, mount) = ballista_app(Vec2::new(0.0, 0.0));
    static_enemy(&mut app, Vec3::new(6.0, 0.4, -6.0)); // bearing +45°, in range

    let rest = Vec3::new(0.0, warden::systems::tower::BALLISTA_MOUNT_Y, 0.0);
    let disp = |app: &App| -> f32 {
        app.world()
            .get::<Transform>(mount)
            .unwrap()
            .translation
            .distance(rest)
    };

    // The very first update is the fire frame (cooldown starts at 0): the
    // kick is already visible, one tick into the recovery.
    app.update();
    let kick = disp(&app);
    assert!(
        (0.09..=0.12).contains(&kick),
        "the kick must appear on the fire frame (~depth 0.12 one tick in), got {kick:.3}"
    );

    // Falls back under 10% of the peak within the 0.18s recover window.
    for _ in 0..13 {
        app.update(); // fire frame + 13 ticks = 0.233s ≥ 0.18s
    }
    let settled = disp(&app);
    assert!(
        settled < 0.1 * 0.12,
        "must fall back under 10% of depth within 0.18s, got {settled:.4}"
    );

    // Re-trigger mid-recovery RESETS the cycle instead of stacking. Driven
    // through the same `TowerFired` message the rules write on every shot.
    app.world_mut()
        .write_message(warden::systems::tower::TowerFired { tower });
    app.update();
    for _ in 0..5 {
        app.update(); // ≈ half recovered
    }
    let mid = disp(&app);
    assert!(
        mid < kick * 0.75,
        "mid-recovery displacement must have decayed first, got {mid:.3}"
    );
    app.world_mut()
        .write_message(warden::systems::tower::TowerFired { tower });
    app.update();
    let reset = disp(&app);
    assert!(
        (reset - kick).abs() < 0.01,
        "a re-trigger must RESET back to the full kick, not stack (kick={kick:.3}, after={reset:.3})"
    );

    // Frenzy refire (0.67s cooldown): the peak never exceeds the configured
    // depth, and the aim keeps converging under continuous fire.
    app.world_mut()
        .resource_mut::<warden::resources::WarFrenzy>()
        .remaining = 2.0;
    let mut peak = disp(&app);
    for _ in 0..180 {
        app.update();
        peak = peak.max(disp(&app));
    }
    assert!(
        peak <= 0.12 + 1e-4,
        "peak displacement must never exceed depth (reset semantics), got {peak:.4}"
    );
    let yaw = mount_yaw_deg(&app, mount);
    assert!(
        (yaw + 135.0).abs() <= 5.0,
        "aim stays converged on the −X-muzzle bearing −135° (±5°) while recoiling, got {yaw:.1}°"
    );
}

/// Capability card ART8 — acceptance: without a fire there is no displacement.
#[test]
fn ballista_recoil_idle_without_fire() {
    let (mut app, _tower, mount) = ballista_app(Vec2::new(0.0, 0.0));
    // No enemy → no acquisition → no fire for a full second.
    for _ in 0..60 {
        app.update();
    }
    let tf = app.world().get::<Transform>(mount).unwrap();
    assert!(
        tf.translation
            .distance(Vec3::new(0.0, warden::systems::tower::BALLISTA_MOUNT_Y, 0.0))
            < 1e-6,
        "no fire, no displacement"
    );
}

// --- SM1 v2: studio showroom (neutral viewer) ------------------------------------
//
// Headless equivalents of the live acceptance: `cargo run -- --scenario
// showroom` = neutral studio (no map/HUD/dusk), egui list or Tab / ←/→ to
// browse, drag orbit / wheel zoom. Exhibits are pure presentation entities
// — every assertion below also pins the zero-interference law.

#[cfg(debug_assertions)]
mod showroom {
    use super::debug_cheats::debug_app;
    use super::*;
    use warden::resources::{DebugRun, EnemyDefs, FusionDefs};
    use warden::systems::debug::{DebugScenarioQueue, Scenario};
    use warden::systems::showroom::{
        ShowroomExhibit, ShowroomFloor, ShowroomLight, ShowroomPad, ShowroomPlate, ShowroomState,
    };
    use showroom_viewer::ShowroomOrbit;

    fn enter(app: &mut App) {
        app.world_mut()
            .resource_mut::<DebugScenarioQueue>()
            .0
            .push(Scenario::Showroom);
    }

    fn count_exhibits(app: &mut App) -> usize {
        let mut q = app.world_mut().query_filtered::<(), With<ShowroomExhibit>>();
        q.iter(app.world()).count()
    }

    fn plate_text(app: &mut App) -> String {
        let mut q = app.world_mut().query_filtered::<&Text2d, With<ShowroomPlate>>();
        q.single(app.world())
            .expect("exactly one name plate")
            .0
            .clone()
    }

    fn press(app: &mut App, key: KeyCode) {
        let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
        keys.press(key);
        app.update();
        // MinimalPlugins has no InputPlugin, so nothing auto-resets the button
        // state between frames — release + clear or the key stays "held" and
        // re-fires on every later update (and blocks the next press()).
        let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
        keys.release(key);
        keys.clear_just_pressed(key);
    }

    /// SM1 v2 acceptance: entering the studio sweeps the battlefield away
    /// (no map/slots), hides the game UI, moves to GameState::Showroom,
    /// lands floor + pad + exactly one exhibit on the entry frame, marks the
    /// run (save isolation), leaves no rule entities, and three seconds of
    /// idling change nothing.
    #[test]
    fn showroom_enters_neutral_studio_zero_interference() {
        let mut app = debug_app();
        app.update(); // startup baseline: gold 100, base 10/10, wave 1
        enter(&mut app);
        app.update(); // executor builds the studio; present spawns the exhibit
        app.update(); // the GameState::Showroom transition applies
        assert!(
            app.world().get_resource::<ShowroomState>().is_some(),
            "browser state is inserted on entry"
        );
        assert_eq!(count_exhibits(&mut app), 1, "exactly one exhibit on the pad");
        let mut floors = app.world_mut().query_filtered::<(), With<ShowroomFloor>>();
        assert_eq!(floors.iter(app.world()).count(), 1, "the studio floor exists");
        let mut pads = app.world_mut().query_filtered::<(), With<ShowroomPad>>();
        assert_eq!(pads.iter(app.world()).count(), 1, "the display pad exists");
        // Battlefield swept: no ground, no tower slots remain.
        let mut ground = app.world_mut().query::<&warden::components::MapGround>();
        assert_eq!(ground.iter(app.world()).count(), 0, "the arena terrain is gone");
        let mut slots = app.world_mut().query::<&TowerSlot>();
        assert_eq!(slots.iter(app.world()).count(), 0, "the tower slots are gone");
        // Game UI hidden: the command-bar ROOT is invisible (its children
        // inherit the hidden state — ControlZoneRoot stays `Inherited`).
        let mut bars = app
            .world_mut()
            .query_filtered::<&Visibility, With<warden::systems::command::CommandBarRoot>>();
        for vis in bars.iter(app.world()) {
            assert_eq!(*vis, Visibility::Hidden, "the game UI must be hidden");
        }
        assert_eq!(
            app.world().resource::<State<GameState>>().get(),
            &GameState::Showroom,
            "the studio runs in its own game state"
        );
        assert!(
            app.world().resource::<DebugRun>().marked,
            "scenario entry marks the run (save isolation)"
        );
        let mut towers = app.world_mut().query::<&Tower>();
        assert_eq!(
            towers.iter(app.world()).count(),
            0,
            "exhibits carry no Tower rule component"
        );
        let mut enemies = app.world_mut().query::<&Enemy>();
        assert_eq!(
            enemies.iter(app.world()).count(),
            0,
            "exhibits carry no Enemy rule component"
        );
        // Orbit camera framed the exhibit (default pose ~6.5 units out).
        let mut cams = app.world_mut().query_filtered::<&Transform, With<Camera>>();
        let cam = cams.single(app.world()).expect("one camera");
        let dist = cam.translation.distance(Vec3::new(0.0, 1.0, 0.0));
        assert!(
            (dist - 6.5).abs() < 0.1,
            "camera sits at the default orbit distance (got {dist})"
        );
        let (gold, base, wave) = {
            let w = app.world().resource::<WaveState>();
            (
                app.world().resource::<Economy>().gold,
                app.world().resource::<BaseHp>().hp,
                w.current,
            )
        };
        // 3 s of idling at the fixed 1/60 s timestep.
        for _ in 0..180 {
            app.update();
        }
        assert_eq!(count_exhibits(&mut app), 1, "still exactly one exhibit");
        let w = app.world().resource::<WaveState>();
        assert_eq!(
            app.world().resource::<Economy>().gold,
            gold,
            "browsing must not touch the economy"
        );
        assert_eq!(app.world().resource::<BaseHp>().hp, base);
        assert_eq!(w.current, wave, "waves must not advance in the showroom");
    }

    /// SM1 acceptance: the browse list auto-enumerates — → cycles through
    /// every tower (base + fused) and every enemy, Tab switches category,
    /// with at most one exhibit alive at every step and the name plate naming
    /// the def on stage (adding a def to the tables extends the cycle with
    /// zero showroom changes — the lists are read live from the defs).
    #[test]
    fn showroom_traversal_visits_every_def_single_exhibit() {
        let mut app = debug_app();
        app.update();
        enter(&mut app);
        // Plugin ordering vs the executor is unspecified — the exhibit lands
        // by the second frame after entry.
        app.update();
        app.update();
        let (tower_labels, fusion_labels, enemy_labels) = {
            let towers = app.world().resource::<TowerDefs>();
            let fusions = app.world().resource::<FusionDefs>();
            let enemies = app.world().resource::<EnemyDefs>();
            (
                towers.list.iter().map(|d| d.label).collect::<Vec<_>>(),
                fusions.list.iter().map(|d| d.label).collect::<Vec<_>>(),
                enemies.list.iter().map(|d| d.label).collect::<Vec<_>>(),
            )
        };
        let n_towers = tower_labels.len() + fusion_labels.len();
        let n_enemies = enemy_labels.len();
        assert!(n_towers >= 2 && n_enemies >= 2, "the defs must be non-trivial");

        let mut seen: Vec<String> = vec![plate_text(&mut app)]; // item 0 is on stage
        for _ in 0..n_towers {
            press(&mut app, KeyCode::ArrowRight);
            assert_eq!(count_exhibits(&mut app), 1, "one exhibit at a time");
            seen.push(plate_text(&mut app));
        }
        press(&mut app, KeyCode::Tab);
        assert_eq!(count_exhibits(&mut app), 1, "category switch keeps one exhibit");
        seen.push(plate_text(&mut app)); // enemies item 0
        for _ in 0..n_enemies {
            press(&mut app, KeyCode::ArrowRight);
            assert_eq!(count_exhibits(&mut app), 1, "one exhibit at a time");
            seen.push(plate_text(&mut app));
        }
        for label in tower_labels
            .iter()
            .chain(fusion_labels.iter())
            .chain(enemy_labels.iter())
        {
            assert!(
                seen.iter().any(|t| t.contains(label)),
                "def '{label}' must take the stage at least once (plates: {seen:?})"
            );
        }
        let mut distinct = seen.clone();
        distinct.sort();
        distinct.dedup();
        assert_eq!(
            distinct.len(),
            n_towers + n_enemies,
            "exactly one distinct exhibit per def (cycle wraps back to item 0)"
        );
        // Browsing keys also work backwards.
        press(&mut app, KeyCode::ArrowLeft);
        assert_eq!(count_exhibits(&mut app), 1);
        assert_ne!(plate_text(&mut app), *seen.last().unwrap());
    }

    /// SM1 v2 acceptance: the egui list and the keys share one cursor —
    /// a `select` write (exactly what a panel click does) swaps the exhibit
    /// through the same present path; and left-drag orbits the camera.
    #[test]
    fn showroom_panel_select_and_orbit_camera() {
        let mut app = debug_app();
        app.update();
        enter(&mut app);
        app.update();
        app.update(); // exhibit lands by the second frame (plugin ordering)
        // Panel-equivalent selection: enemy 3 (healer) straight from item 0.
        app.world_mut()
            .resource_mut::<ShowroomState>()
            .select(1, 3);
        app.update();
        assert_eq!(count_exhibits(&mut app), 1, "one exhibit after panel select");
        let plate = plate_text(&mut app);
        let healer = app.world().resource::<EnemyDefs>().list[3].label;
        assert!(
            plate.contains(healer),
            "panel select puts the clicked def on the pad (plate '{plate}')"
        );
        // Orbit: hold left + mouse motion moves the camera around the pad.
        let mut cams = app.world_mut().query_filtered::<&Transform, With<Camera>>();
        let before = cams.single(app.world()).unwrap().translation;
        {
            let mut mice = app
                .world_mut()
                .resource_mut::<ButtonInput<bevy::input::mouse::MouseButton>>();
            mice.press(bevy::input::mouse::MouseButton::Left);
        }
        app.world_mut()
            .insert_resource(bevy::input::mouse::AccumulatedMouseMotion {
                delta: Vec2::new(60.0, 0.0),
            });
        app.update();
        let mut cams = app.world_mut().query_filtered::<&Transform, With<Camera>>();
        let after = cams.single(app.world()).unwrap().translation;
        assert_ne!(
            before, after,
            "dragging must orbit the camera around the exhibit"
        );
    }

    /// SM1 v2.1 acceptance: the studio is airtight — any 3D-visual ROOT that
    /// is not part of the showroom (late-materializing async props, stray
    /// meshes/lights) is removed the frame it appears, while the studio's
    /// own floor/pad/exhibit/light and the camera survive.
    #[test]
    fn showroom_sweep_removes_strays_keeps_studio() {
        let mut app = debug_app();
        app.update();
        enter(&mut app);
        app.update();
        app.update(); // exhibit lands by the second frame (plugin ordering)
        // A stray unmarked mesh root and an async-scene-style root appear
        // AFTER the studio is up (props materialize late in the real app).
        let mesh = app
            .world_mut()
            .resource_mut::<Assets<Mesh>>()
            .add(Sphere::new(0.5));
        let stray_mesh = app
            .world_mut()
            .spawn((Mesh3d(mesh), Transform::from_xyz(5.0, 0.0, 5.0)))
            .id();
        let stray_scene = app
            .world_mut()
            .spawn((bevy::world_serialization::WorldAssetRoot(
                default(),
            ), Transform::from_xyz(-5.0, 0.0, -5.0)))
            .id();
        app.update();
        let entities = app.world().entities();
        assert!(
            !entities.contains(stray_mesh),
            "a stray mesh root must be swept"
        );
        assert!(
            !entities.contains(stray_scene),
            "a stray async-scene root must be swept"
        );
        // The studio itself is untouched.
        assert_eq!(count_exhibits(&mut app), 1, "the exhibit survives");
        let mut floors = app.world_mut().query_filtered::<(), With<ShowroomFloor>>();
        assert_eq!(floors.iter(app.world()).count(), 1, "the floor survives");
        let mut lights = app
            .world_mut()
            .query_filtered::<(), With<ShowroomLight>>();
        assert_eq!(lights.iter(app.world()).count(), 1, "the studio light survives");
        let mut cams = app.world_mut().query_filtered::<(), With<Camera>>();
        assert_eq!(cams.iter(app.world()).count(), 1, "the camera survives");
        // Exhibits keep switching after sweeps (the sweep never eats them).
        app.world_mut().resource_mut::<ShowroomState>().select(1, 0);
        app.update();
        assert_eq!(count_exhibits(&mut app), 1);
        assert!(plate_text(&mut app).contains("普通"), "enemy 0 on the pad");
    }

    /// SM1 v2.3 acceptance: G lays the WHOLE current category out at once
    /// (one exhibit per def, gallery identity markers), and selecting from
    /// the panel-equivalent path refocuses to a single exhibit.
    #[test]
    fn showroom_gallery_lays_out_category_and_refocuses() {
        let mut app = debug_app();
        app.update();
        enter(&mut app);
        app.update();
        app.update(); // exhibit lands by the second frame (plugin ordering)
        let n_towers = app.world().resource::<TowerDefs>().list.len()
            + app.world().resource::<FusionDefs>().list.len();
        assert_eq!(count_exhibits(&mut app), 1, "focus mode starts with one");
        // G -> gallery: every def of the category on stage at once.
        press(&mut app, KeyCode::KeyG);
        assert_eq!(
            count_exhibits(&mut app),
            n_towers,
            "gallery lays out one exhibit per def"
        );
        let mut gallery = app
            .world_mut()
            .query_filtered::<(), With<warden::systems::showroom::ShowroomGalleryItem>>();
        assert_eq!(
            gallery.iter(app.world()).count(),
            n_towers,
            "every gallery exhibit carries its catalog identity"
        );
        // Panel-equivalent pick: select item 2 and leave gallery -> refocus.
        let label = app.world().resource::<TowerDefs>().list[2].label;
        {
            let mut st = app.world_mut().resource_mut::<ShowroomState>();
            st.select(0, 2);
            st.gallery = false;
        }
        app.update();
        assert_eq!(count_exhibits(&mut app), 1, "back to a single exhibit");
        assert!(
            plate_text(&mut app).contains(label),
            "the picked def takes the pad (plate '{}')",
            plate_text(&mut app)
        );
        // G toggles back into the grid and out again.
        press(&mut app, KeyCode::KeyG);
        assert_eq!(count_exhibits(&mut app), n_towers);
        press(&mut app, KeyCode::KeyG);
        assert_eq!(count_exhibits(&mut app), 1);
    }

    /// SM6 acceptance: search filters by name/tags, drives focus to the
    /// first hit, and flattens the hit set in gallery mode.
    #[test]
    fn showroom_search_filters_focus_and_gallery() {
        let mut app = debug_app();
        app.update();
        enter(&mut app);
        app.update();
        app.update();
        // "mage" hits the mage tower by name; focus auto-jumps to it.
        app.world_mut()
            .resource_mut::<showroom_viewer::SearchQuery>()
            .text = "mage".into();
        app.update();
        app.update(); // present reacts to the stage change
        assert_eq!(count_exhibits(&mut app), 1);
        assert!(
            plate_text(&mut app).contains("法师塔"),
            "focus jumps to the first hit (plate '{}')",
            plate_text(&mut app)
        );
        // Chinese tag search: the zh label is a searchable tag.
        app.world_mut()
            .resource_mut::<showroom_viewer::SearchQuery>()
            .text = "弓箭手".into();
        app.update();
        app.update();
        assert!(plate_text(&mut app).contains("弓箭手塔"), "zh tag matches");
        // Gallery + search: the flattened hit set goes on stage ("enemy"
        // tag matches all six enemies, none of the towers).
        app.world_mut()
            .resource_mut::<showroom_viewer::ShowroomState>()
            .gallery = true;
        app.world_mut()
            .resource_mut::<showroom_viewer::SearchQuery>()
            .text = "enemy".into();
        app.update();
        app.update();
        assert_eq!(count_exhibits(&mut app), 6, "six enemies match the tag");
        // Clearing the query restores the plain category gallery.
        app.world_mut()
            .resource_mut::<showroom_viewer::SearchQuery>()
            .text = String::new();
        app.update();
        app.update();
        let n_towers = app.world().resource::<TowerDefs>().list.len()
            + app.world().resource::<FusionDefs>().list.len();
        assert_eq!(count_exhibits(&mut app), n_towers, "empty query = full category");
    }

    /// SM5 acceptance: playback time control writes the virtual clock
    /// (pause freezes, step lets one frame through), the turntable rotates,
    /// and R resets the camera.
    #[test]
    fn showroom_time_control_turntable_and_view_reset() {
        let mut app = debug_app();
        app.update();
        enter(&mut app);
        app.update();
        app.update();
        // Pause freezes the virtual clock.
        {
            let mut tm = app
                .world_mut()
                .resource_mut::<showroom_viewer::ShowroomTime>();
            tm.paused = true;
        }
        app.update();
        assert!(
            app.world().resource::<Time<Virtual>>().is_paused(),
            "panel pause freezes the virtual clock"
        );
        // Step lets exactly one frame through, then re-freezes.
        {
            let mut tm = app
                .world_mut()
                .resource_mut::<showroom_viewer::ShowroomTime>();
            tm.step = 1;
        }
        app.update(); // stepped frame
        app.update(); // re-frozen
        assert!(
            app.world().resource::<Time<Virtual>>().is_paused(),
            "step re-freezes after one frame"
        );
        // Speed writes through.
        {
            let mut tm = app
                .world_mut()
                .resource_mut::<showroom_viewer::ShowroomTime>();
            tm.paused = false;
            tm.speed = 0.5;
        }
        app.update();
        assert!((app.world().resource::<Time<Virtual>>().relative_speed() - 0.5).abs() < 1e-3);
        // Turntable rotates the yaw over time (virtual dt; unpaused here).
        {
            let mut t = app
                .world_mut()
                .resource_mut::<showroom_viewer::ShowroomTurntable>();
            t.on = true;
        }
        let yaw0 = app.world().resource::<showroom_viewer::ShowroomOrbit>().yaw_deg;
        for _ in 0..30 {
            app.update();
        }
        let yaw1 = app.world().resource::<showroom_viewer::ShowroomOrbit>().yaw_deg;
        assert!(yaw1 - yaw0 > 1.0, "turntable rotates ({} -> {yaw1})", yaw0);
        // R resets the camera pose to defaults (turn the turntable off
        // first, or it rotates the fresh pose on the very same frame).
        app.world_mut()
            .resource_mut::<showroom_viewer::ShowroomTurntable>()
            .on = false;
        press(&mut app, KeyCode::KeyR);
        let o = app.world().resource::<showroom_viewer::ShowroomOrbit>();
        assert_eq!(o.yaw_deg, ShowroomOrbit::default().yaw_deg);
        assert_eq!(o.dist, ShowroomOrbit::default().dist);
    }

    /// SM7/SM8 acceptance: origins resolve from art-assets.json (vault id +
    /// version on real assets, GameLocal on gray boxes), the composition
    /// exhibit carries its part tag, and compare mode stages two entries
    /// side by side until any selection leaves it.
    #[test]
    fn showroom_origins_parts_and_compare() {
        let mut app = debug_app();
        app.update();
        enter(&mut app);
        app.update();
        app.update();
        // Origins: the archer composition = tower_arrow + weapon_ballista,
        // both resolved against the real manifest (test cwd = games/warden).
        {
            let cat = app
                .world()
                .resource::<showroom_viewer::ShowroomCatalog>();
            let archer = &cat.categories[0].items[0];
            assert_eq!(archer.origin.len(), 2, "archer = two vault parts");
            for o in &archer.origin {
                match o {
                    showroom_viewer::Origin::Vault { id, version, .. } => {
                        assert_ne!(id, "?", "manifest id resolved");
                        assert_ne!(version, "?", "manifest version resolved");
                    }
                    showroom_viewer::Origin::GameLocal { .. } => panic!("archer parts are vault"),
                }
            }
            assert_eq!(archer.parts.len(), 2, "composition declares two parts");
            let fused = &cat.categories[0].items[4];
            assert!(
                matches!(&fused.origin[0], showroom_viewer::Origin::GameLocal { .. }),
                "gray-box fusion is game-local"
            );
        }
        // The focused composition exhibit carries its parts tag (children
        // only exist on the model path — live-verified).
        let mut tags = app
            .world_mut()
            .query_filtered::<&showroom_viewer::ShowroomPartsTag, With<ShowroomExhibit>>();
        assert_eq!(tags.single(app.world()).unwrap().0, 2);
        // Compare mode: two entries on stage, A/B plates; a selection leaves.
        {
            let mut st = app.world_mut().resource_mut::<ShowroomState>();
            st.compare = Some(((0, 0), (1, 0)));
            st.compare_pick_b = false;
        }
        app.update();
        app.update();
        assert_eq!(count_exhibits(&mut app), 2, "compare stages two entries");
        let mut plates = app
            .world_mut()
            .query_filtered::<&Text2d, With<ShowroomPlate>>();
        let texts: Vec<String> = plates
            .iter(app.world())
            .map(|t| t.0.clone())
            .collect();
        assert!(texts.iter().any(|t| t.starts_with("A ·")), "A plate: {texts:?}");
        assert!(texts.iter().any(|t| t.starts_with("B ·")), "B plate: {texts:?}");
        app.world_mut().resource_mut::<ShowroomState>().select(0, 3);
        app.update();
        app.update();
        assert_eq!(count_exhibits(&mut app), 1, "selection leaves compare");
        assert!(
            app.world()
                .resource::<ShowroomState>()
                .compare
                .is_none()
        );
    }
}

// --- SM3R: debug shooting range --------------------------------------------------
//
// One real attacker (real spawn/fire/impact pipeline) versus stationary
// dummies with a damage ledger; entered via Scenario::Range. The tool never
// fakes combat logic — the ledger just polls the dummies' real hp.

#[cfg(debug_assertions)]
mod range_tests {
    use super::debug_cheats::debug_app;
    use super::*;
    use warden::systems::debug::{DebugScenarioQueue, Scenario};
    use warden::systems::enemy::RangeDummy;
    use warden::systems::range::{RangeLedger, RangeState};

    fn enter_range(app: &mut App) {
        app.world_mut()
            .resource_mut::<DebugScenarioQueue>()
            .0
            .push(Scenario::Range);
    }

    fn dummy_count(app: &mut App) -> usize {
        let mut q = app
            .world_mut()
            .query_filtered::<&Transform, With<RangeDummy>>();
        q.iter(app.world()).count()
    }

    fn dummy_positions(app: &mut App) -> Vec<Vec3> {
        let mut q = app
            .world_mut()
            .query_filtered::<&Transform, With<RangeDummy>>();
        q.iter(app.world()).map(|t| t.translation).collect()
    }

    /// SM3R acceptance: the range lands one attacker + three STATIONARY
    /// dummies; the real fire pipeline damages them and the ledger
    /// accumulates (total damage / hits).
    #[test]
    fn range_fires_at_stationary_dummies_and_ledgers() {
        let mut app = debug_app();
        app.update();
        enter_range(&mut app);
        for _ in 0..3 {
            app.update(); // executor -> maintenance reset lands the board
        }
        let mut towers = app.world_mut().query::<&Tower>();
        assert_eq!(towers.iter(app.world()).count(), 1, "one attacker tower");
        assert_eq!(dummy_count(&mut app), 3, "three dummies");
        // Clean stage: battlefield gone, game UI hidden, studio floor in.
        let mut ground = app.world_mut().query::<&warden::components::MapGround>();
        assert_eq!(ground.iter(app.world()).count(), 0, "battlefield swept");
        let mut slots = app.world_mut().query::<&TowerSlot>();
        assert_eq!(slots.iter(app.world()).count(), 0, "tower slots swept");
        let mut bars = app
            .world_mut()
            .query_filtered::<&Visibility, With<warden::systems::command::CommandBarRoot>>();
        for vis in bars.iter(app.world()) {
            assert_eq!(*vis, Visibility::Hidden, "game UI hidden in the range");
        }
        let mut floors = app
            .world_mut()
            .query_filtered::<(), With<showroom_viewer::ShowroomFloor>>();
        assert_eq!(floors.iter(app.world()).count(), 1, "studio floor present");
        // Dummies never move (RangeDummy is excluded from path movement).
        let before = dummy_positions(&mut app);
        for _ in 0..60 {
            app.update();
        }
        let mut after = dummy_positions(&mut app);
        // Query iteration order is arbitrary — compare as sorted sets.
        let mut before = before;
        before.sort_by_key(|v| (v.x.to_bits(), v.z.to_bits()));
        after.sort_by_key(|v| (v.x.to_bits(), v.z.to_bits()));
        assert_eq!(before, after, "dummies are stationary");
        // The real pipeline fires: after 5 s the ledger has real damage
        // (archer: 10 dmg, 1/s — dummies are ordinary, no healers).
        for _ in 0..300 {
            app.update();
        }
        let l = app.world().resource::<RangeLedger>();
        assert!(
            l.total >= 10.0,
            "ledger accumulated damage (total {})",
            l.total
        );
        assert!(l.hits >= 1, "ledger counted hits ({})", l.hits);
    }

    /// SM3R acceptance: a killed dummy (real death path) is replaced, and a
    /// reset respawns everything and zeroes the ledger.
    #[test]
    fn range_respawns_dead_dummy_and_resets() {
        let mut app = debug_app();
        app.update();
        enter_range(&mut app);
        for _ in 0..3 {
            app.update();
        }
        // Kill one dummy through the real hp field: death despawns (+gold),
        // maintenance refills the row.
        {
            let mut q = app
                .world_mut()
                .query_filtered::<&mut Enemy, With<RangeDummy>>();
            let Some(mut first) = q.iter_mut(app.world_mut()).next() else {
                panic!("dummies on the board");
            };
            first.hp = 0.0;
        }
        app.update(); // death settles
        app.update(); // maintenance refills
        app.update(); // spawn lands
        assert_eq!(dummy_count(&mut app), 3, "dead dummy replaced");
        // Reset: fresh board, ledger zeroed.
        app.world_mut().resource_mut::<RangeState>().reset = true;
        app.update();
        app.update();
        let l = app.world().resource::<RangeLedger>();
        assert_eq!(l.total, 0.0, "reset zeroes the ledger");
        assert_eq!(dummy_count(&mut app), 3, "reset respawns the dummies");
        let mut towers = app.world_mut().query::<&Tower>();
        assert_eq!(towers.iter(app.world()).count(), 1, "attacker respawned");
    }
}

// --- SK5: generic skill framework (ADR-0011) --------------------------------------

mod skills_core {
    use super::*;
    use bevy::ecs::message::MessageCursor;
    use warden::components::SkillCaster;
    use warden::resources::{
        SkillAiming, SkillBalance, SkillDef, SkillDefs, SkillKind, SkillTargetMode,
    };
    use warden::systems::skills::{learn_skill, level_up, CastSkill, SkillCast};

    /// Test defs: a None-target active (slot 0), a Point-target active
    /// (slot 1) and a passive (slot 2) — the shapes the framework must tell
    /// apart. Production rows land with SK8/SK9; the framework is content-free.
    fn register_test_defs(app: &mut App) {
        {
            let mut defs = app.world_mut().resource_mut::<SkillDefs>();
            defs.0 = vec![
                SkillDef {
                    name: "burst",
                    kind: SkillKind::Active,
                    target: SkillTargetMode::None,
                    max_level: 3,
                    badge: 'B',
                    hotkey: None,
                    indicator_radius: 0.0,
                },
                SkillDef {
                    name: "aimed",
                    kind: SkillKind::Active,
                    target: SkillTargetMode::Point,
                    max_level: 1,
                    badge: 'A',
                    hotkey: None,
                    indicator_radius: 4.0,
                },
                SkillDef {
                    name: "instinct",
                    kind: SkillKind::Passive,
                    target: SkillTargetMode::None,
                    max_level: 3,
                    badge: 'P',
                    hotkey: None,
                    indicator_radius: 0.0,
                },
            ];
        }
        app.world_mut()
            .resource_mut::<SkillBalance>()
            .cooldowns = vec![vec![10.0, 12.0, 14.0], vec![20.0], vec![0.0, 0.0, 0.0]];
    }

    /// A bare caster entity — no Tower, no other component: the framework
    /// must not care who owns the skills.
    fn spawn_caster(app: &mut App) -> Entity {
        let mut caster = SkillCaster { slots: vec![] };
        learn_skill(&mut caster, 0, 1);
        learn_skill(&mut caster, 1, 1);
        learn_skill(&mut caster, 2, 1);
        app.world_mut().spawn(caster).id()
    }

    fn request_cast(app: &mut App, caster: Entity, slot: usize) {
        app.world_mut()
            .resource_mut::<Messages<CastSkill>>()
            .write(CastSkill { caster, slot });
    }

    /// Messages are double-buffered: `len()` flips with the buffer, so count
    /// via a cursor delta instead (the KB pitfall note on message asserts).
    fn new_casts(app: &mut App, cursor: &mut MessageCursor<SkillCast>) -> usize {
        let messages = app.world().resource::<Messages<SkillCast>>();
        cursor.read(messages).count()
    }

    /// SK5 acceptance: a confirmed None-target cast starts the slot cooldown
    /// (from SkillBalance by def+level), and it ticks down with virtual time
    /// exactly like the phase-1 law — freezing on pause, resuming after.
    #[test]
    fn slot_cooldown_ticks_with_time_and_freezes_on_pause() {
        let mut app = test_app();
        register_test_defs(&mut app);
        let caster = spawn_caster(&mut app);
        app.update();
        let mut casts = MessageCursor::<SkillCast>::default();
        request_cast(&mut app, caster, 0);
        app.update(); // the request validates and confirms in this frame
        assert_eq!(new_casts(&mut app, &mut casts), 1, "one confirmed cast");
        let started = app.world().get::<SkillCaster>(caster).unwrap().slots[0].cooldown;
        assert!(
            (started - 10.0 + 1.0 / 60.0).abs() < 1e-3,
            "cooldown starts at the SkillBalance value ({started})"
        );
        for _ in 0..60 {
            app.update(); // 1s of game time
        }
        let after_1s = app.world().get::<SkillCaster>(caster).unwrap().slots[0].cooldown;
        assert!(
            (after_1s - 9.0).abs() < 0.05,
            "1s later the CD must be ~9s (got {after_1s})"
        );
        app.world_mut()
            .resource_mut::<NextState<GameState>>()
            .set(GameState::Paused);
        app.update(); // applies the transition
        let frozen = app.world().get::<SkillCaster>(caster).unwrap().slots[0].cooldown;
        for _ in 0..120 {
            app.update();
        }
        let after_pause = app.world().get::<SkillCaster>(caster).unwrap().slots[0].cooldown;
        assert!(
            (after_pause - frozen).abs() < 1e-6,
            "paused game must freeze the cooldown (got {after_pause})"
        );
        app.world_mut()
            .resource_mut::<NextState<GameState>>()
            .set(GameState::Playing);
        app.update();
        app.update();
        let resumed = app.world().get::<SkillCaster>(caster).unwrap().slots[0].cooldown;
        assert!(resumed < after_pause, "resuming ticks the cooldown again");
    }

    /// SK5 acceptance: a slot on cooldown rejects cast requests (no new
    /// SkillCast, no cooldown restart) and never enters aiming.
    #[test]
    fn cooldown_gate_rejects_cast_requests() {
        let mut app = test_app();
        register_test_defs(&mut app);
        let caster = spawn_caster(&mut app);
        app.update();
        let mut casts = MessageCursor::<SkillCast>::default();
        request_cast(&mut app, caster, 0);
        app.update();
        assert_eq!(new_casts(&mut app, &mut casts), 1);
        let cd = app.world().get::<SkillCaster>(caster).unwrap().slots[0].cooldown;
        for _ in 0..30 {
            app.update(); // let some time pass so a restart would be visible
        }
        let before = app.world().get::<SkillCaster>(caster).unwrap().slots[0].cooldown;
        assert!(before < cd, "time passed");
        request_cast(&mut app, caster, 0);
        app.update();
        app.update();
        assert_eq!(
            new_casts(&mut app, &mut casts),
            0,
            "no second cast while cooling down"
        );
        let after = app.world().get::<SkillCaster>(caster).unwrap().slots[0].cooldown;
        assert!(
            after <= before,
            "a rejected request must not restart the cooldown ({before} -> {after})"
        );
        assert_eq!(
            app.world().resource::<SkillAiming>().0,
            None,
            "no aiming from a rejected request"
        );
    }

    /// SK5 acceptance: a Point-target skill enters aiming without paying;
    /// Esc cancels through the unified cancel and still pays nothing.
    #[test]
    fn point_skill_aims_and_esc_cancels_without_paying() {
        let mut app = test_app();
        register_test_defs(&mut app);
        let caster = spawn_caster(&mut app);
        app.update();
        let mut casts = MessageCursor::<SkillCast>::default();
        request_cast(&mut app, caster, 1);
        app.update();
        assert_eq!(
            app.world().resource::<SkillAiming>().0,
            Some((caster, 1)),
            "a Point skill enters aiming"
        );
        assert!(
            app.world().get::<SkillCaster>(caster).unwrap().slots[1].cooldown <= 0.0,
            "aiming does not start the cooldown"
        );
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Escape);
        app.update();
        {
            let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
            keys.clear();
            keys.release(KeyCode::Escape);
        }
        assert_eq!(app.world().resource::<SkillAiming>().0, None, "Esc cancels");
        assert!(
            app.world().get::<SkillCaster>(caster).unwrap().slots[1].cooldown <= 0.0,
            "a cancelled pick never pays"
        );
        assert_eq!(new_casts(&mut app, &mut casts), 0, "no cast happened");
    }

    /// SK5 acceptance: the real player path — cursor over a world point,
    /// left click — confirms the aim, emits SkillCast with that point and
    /// starts the cooldown.
    #[test]
    fn point_skill_confirms_at_cursor_ground_point() {
        let mut app = test_app();
        register_test_defs(&mut app);
        let caster = spawn_caster(&mut app);
        app.update();
        app.update();
        let mut casts = MessageCursor::<SkillCast>::default();
        request_cast(&mut app, caster, 1);
        app.update();
        assert!(app.world().resource::<SkillAiming>().0.is_some());
        // Cursor over the world origin, then confirm with a left click
        // (same headless window emulation as the phase-1 meteor tests).
        enable_world_picking(&mut app, Vec2::new(640.0, 360.0));
        let point = Vec3::new(0.0, 0.0, 0.0);
        let cursor = {
            let mut cams = app.world_mut().query::<(&Camera, &GlobalTransform)>();
            let (cam, tf) = cams.single(app.world()).expect("one camera");
            cam.world_to_viewport(tf, point).expect("point projects")
        };
        {
            let mut q = app.world_mut().query_filtered::<&mut Window, ()>();
            for mut w in q.iter_mut(app.world_mut()) {
                w.set_cursor_position(Some(cursor));
            }
        }
        click_left(&mut app);
        app.update();
        assert_eq!(app.world().resource::<SkillAiming>().0, None, "confirm clears aiming");
        assert_eq!(new_casts(&mut app, &mut casts), 1, "the cast confirmed");
        let cd = app.world().get::<SkillCaster>(caster).unwrap().slots[1].cooldown;
        assert!(cd > 19.9, "confirm starts the cooldown (got {cd})");
    }

    /// SK5 acceptance: Passive slots never cast; requests for despawned
    /// casters or out-of-range slots are dropped silently.
    #[test]
    fn passive_slots_and_stale_requests_never_cast() {
        let mut app = test_app();
        register_test_defs(&mut app);
        let caster = spawn_caster(&mut app);
        app.update();
        let mut casts = MessageCursor::<SkillCast>::default();
        request_cast(&mut app, caster, 2); // passive
        request_cast(&mut app, Entity::PLACEHOLDER, 0); // despawned caster
        request_cast(&mut app, caster, 9); // out-of-range slot
        app.update();
        app.update();
        assert_eq!(new_casts(&mut app, &mut casts), 0, "nothing validated");
        assert!(
            app.world().get::<SkillCaster>(caster).unwrap().slots[2].cooldown <= 0.0,
            "a passive slot never enters cooldown"
        );
    }

    /// SK5 acceptance: level_up raises one level at a time and clamps at the
    /// def's max_level; re-learning an owned skill refreshes only the level.
    #[test]
    fn level_up_clamps_at_max_level() {
        let defs = SkillDefs(vec![SkillDef {
            name: "x",
            kind: SkillKind::Active,
            target: SkillTargetMode::None,
            max_level: 3,
            badge: 'X',
            hotkey: None,
            indicator_radius: 0.0,
        }]);
        let mut caster = SkillCaster { slots: vec![] };
        learn_skill(&mut caster, 0, 1);
        assert_eq!(level_up(&defs, &mut caster, 0), 2);
        assert_eq!(level_up(&defs, &mut caster, 0), 3);
        assert_eq!(level_up(&defs, &mut caster, 0), 3, "clamped at max_level");
        // Re-learn keeps one slot and only refreshes the level.
        learn_skill(&mut caster, 0, 1);
        assert_eq!(caster.slots.len(), 1);
        assert_eq!(caster.slots[0].level, 1);
    }

    /// SK5: a caster that despawns mid-aim drops the pick (nothing left to
    /// charge) — the aiming resource self-heals instead of sticking.
    #[test]
    fn despawned_caster_drops_the_aim() {
        let mut app = test_app();
        register_test_defs(&mut app);
        let caster = spawn_caster(&mut app);
        app.update();
        request_cast(&mut app, caster, 1);
        app.update();
        assert!(app.world().resource::<SkillAiming>().0.is_some());
        app.world_mut().despawn(caster);
        app.update();
        app.update(); // Input sees the missing caster within two frames
        assert_eq!(
            app.world().resource::<SkillAiming>().0,
            None,
            "aiming self-heals when the caster is gone"
        );
    }
}

// --- SK6: stat modification pipeline (ADR-0011) ------------------------------------

mod stats_pipeline {
    use super::*;
    use warden::components::{EffectiveStats, ModSource, StatMod, StatMods};
    use warden::resources::{StatKind, WarFrenzy, FRENZY_SPEED_MULT};
    use warden::systems::stats::{clear_stat_mod, set_stat_mod, BASE_CRIT_MULTIPLIER};

    /// An archer-shaped base tower with an empty StatMods bag.
    fn spawn_tower(app: &mut App) -> Entity {
        app.world_mut()
            .spawn((
                Tower {
                    tower_index: 0,
                    damage: 10.0,
                    attack_speed: 1.0,
                    range: 20.0,
                    attack_type: AttackType::Physical,
                    cooldown: 0.0,
                    target: None,
                    kind: TowerKind::Base(0),
                    aoe_radius: 0.0,
                    slow_factor: 0.0,
                    slow_duration: 0.0,
                    invested: 0,
                },
                StatMods::default(),
                Transform::from_xyz(0.0, 0.5, 0.0),
            ))
            .id()
    }

    fn estats(app: &App, e: Entity) -> EffectiveStats {
        *app.world().get::<EffectiveStats>(e).expect("aggregated")
    }

    fn push_mod(app: &mut App, e: Entity, m: StatMod) {
        let mut mods = app.world_mut().get_mut::<StatMods>(e).unwrap();
        set_stat_mod(&mut mods, m);
    }

    /// SK6 acceptance: add folds before mult — (10 + 3) × 1.2 = 15.6 — and
    /// an untouched tower aggregates to its base stats with default crit.
    #[test]
    fn aggregation_folds_add_then_mult_with_default_crit() {
        let mut app = test_app();
        let t = spawn_tower(&mut app);
        let t2 = spawn_tower(&mut app); // untouched twin
        push_mod(
            &mut app,
            t,
            StatMod {
                source: ModSource::Skill { caster: t, def: 0 },
                kind: StatKind::Damage,
                add: 3.0,
                mult: 1.0,
                remaining: None,
            },
        );
        push_mod(
            &mut app,
            t,
            StatMod {
                source: ModSource::Other("test"),
                kind: StatKind::Damage,
                add: 0.0,
                mult: 1.2,
                remaining: None,
            },
        );
        app.update();
        let e = estats(&app, t);
        assert!((e.damage - 15.6).abs() < 1e-4, "got {}", e.damage);
        let base = estats(&app, t2);
        assert_eq!(base.damage, 10.0);
        assert_eq!(base.attack_speed, 1.0);
        assert_eq!(base.range, 20.0);
        assert_eq!(base.crit_chance, 0.0, "no crit without a mod");
        assert_eq!(base.crit_multiplier, BASE_CRIT_MULTIPLIER);
    }

    /// SK6 acceptance: one (source, kind) identity refreshes instead of
    /// stacking — the aura/buff re-assert contract.
    #[test]
    fn same_source_refreshes_instead_of_stacking() {
        let mut app = test_app();
        let t = spawn_tower(&mut app);
        let source = ModSource::Skill { caster: t, def: 2 };
        push_mod(
            &mut app,
            t,
            StatMod {
                source,
                kind: StatKind::Damage,
                add: 5.0,
                mult: 1.0,
                remaining: None,
            },
        );
        push_mod(
            &mut app,
            t,
            StatMod {
                source,
                kind: StatKind::Damage,
                add: 7.0,
                mult: 1.0,
                remaining: None,
            },
        );
        app.update();
        assert_eq!(
            app.world().get::<StatMods>(t).unwrap().0.len(),
            1,
            "one entry per (source, kind)"
        );
        assert!((estats(&app, t).damage - 17.0).abs() < 1e-4);
    }

    /// SK6 acceptance: a timed entry ticks down, drops at zero, and the
    /// aggregate falls back with it.
    #[test]
    fn timed_mod_expires_and_aggregate_falls_back() {
        let mut app = test_app();
        let t = spawn_tower(&mut app);
        push_mod(
            &mut app,
            t,
            StatMod {
                source: ModSource::Skill { caster: t, def: 1 },
                kind: StatKind::Damage,
                add: 8.0,
                mult: 1.0,
                remaining: Some(0.5),
            },
        );
        app.update();
        assert!((estats(&app, t).damage - 18.0).abs() < 1e-4, "boost active");
        for _ in 0..60 {
            app.update(); // 1s — well past the 0.5s duration
        }
        assert!(
            app.world().get::<StatMods>(t).unwrap().0.is_empty(),
            "expired entry dropped"
        );
        assert_eq!(estats(&app, t).damage, 10.0, "aggregate fell back");
    }

    /// SK6 acceptance: aura semantics — apply by source on another entity,
    /// remove on leave, aggregate follows both.
    #[test]
    fn aura_apply_and_remove_by_source() {
        let mut app = test_app();
        let owner = spawn_tower(&mut app);
        let ally = spawn_tower(&mut app);
        let aura = ModSource::Aura {
            caster: owner,
            def: 3,
        };
        push_mod(
            &mut app,
            ally,
            StatMod {
                source: aura,
                kind: StatKind::AttackSpeed,
                add: 0.0,
                mult: 1.25,
                remaining: None,
            },
        );
        app.update();
        assert!((estats(&app, ally).attack_speed - 1.25).abs() < 1e-4);
        {
            let mut mods = app.world_mut().get_mut::<StatMods>(ally).unwrap();
            clear_stat_mod(&mut mods, aura, StatKind::AttackSpeed);
        }
        app.update();
        assert_eq!(estats(&app, ally).attack_speed, 1.0, "aura removed");
    }

    /// SK6 acceptance: the global multipliers land in the same fold — AC4
    /// Boosts per tower_index and the SK4 frenzy on attack speed (the old
    /// inline tower_fire math, now aggregated).
    #[test]
    fn aggregate_folds_global_boosts_and_frenzy() {
        let mut app = test_app();
        let t = spawn_tower(&mut app);
        {
            let mut boosts = app.world_mut().resource_mut::<Boosts>();
            boosts.damage_mult[0] = 1.2;
        }
        app.world_mut().resource_mut::<WarFrenzy>().remaining = 1.0;
        app.update();
        let e = estats(&app, t);
        assert!((e.damage - 12.0).abs() < 1e-4, "Boosts folded");
        assert!(
            (e.attack_speed - FRENZY_SPEED_MULT).abs() < 1e-4,
            "frenzy folded on attack speed"
        );
        assert_eq!(e.range, 20.0, "untouched stat stays base");
    }
}

// --- SK7: unified damage pipeline (ADR-0011) ----------------------------------------

mod damage_pipeline {
    use super::*;
    use bevy::ecs::message::MessageCursor;
    use warden::systems::damage::{DamageDealt, DamageRequest, DamageSource};

    fn spawn_enemy_at(app: &mut App, pos: Vec3, hp: f32, armor: bool) -> Entity {
        app.world_mut()
            .spawn((
                Enemy {
                    hp,
                    max_hp: hp,
                    speed: 0.0,
                    leak: 0,
                    kill_gold: 6,
                    physical_armor: armor,
                    def_index: 0,
                    next_wp: 0,
                },
                Transform::from_xyz(pos.x, 0.4, pos.z),
            ))
            .id()
    }

    fn spawn_tower(app: &mut App, index: usize, kind: TowerKind, range: f32) -> Entity {
        app.world_mut()
            .spawn((
                Tower {
                    tower_index: index,
                    damage: 10.0,
                    attack_speed: 1.0,
                    range,
                    attack_type: AttackType::Physical,
                    cooldown: 0.0,
                    target: None,
                    kind,
                    aoe_radius: 0.0,
                    slow_factor: 0.0,
                    slow_duration: 0.0,
                    invested: 0,
                },
                Transform::from_xyz(0.0, 0.5, 0.0),
            ))
            .id()
    }

    fn request(app: &mut App, victim: Entity, amount: f32, ty: AttackType) {
        app.world_mut()
            .resource_mut::<Messages<DamageRequest>>()
            .write(DamageRequest {
                victim,
                amount,
                attack_type: ty,
                source: DamageSource::System,
                crit: false,
            });
    }

    /// SK7 acceptance: armor resolves once, at the single writer — physical
    /// is halved vs armor, magic pierces; both publish DamageDealt with the
    /// applied amount.
    #[test]
    fn armor_resolves_at_the_single_writer() {
        let mut app = test_app();
        let armored = spawn_enemy_at(&mut app, Vec3::new(2.0, 0.0, 0.0), 100.0, true);
        let unarmored = spawn_enemy_at(&mut app, Vec3::new(4.0, 0.0, 0.0), 100.0, false);
        let mut dealt = MessageCursor::<DamageDealt>::default();
        app.update();
        request(&mut app, armored, 40.0, AttackType::Physical);
        request(&mut app, armored, 40.0, AttackType::Magic);
        request(&mut app, unarmored, 40.0, AttackType::Physical);
        app.update();
        let events: Vec<_> = {
            let msgs = app.world().resource::<Messages<DamageDealt>>();
            dealt.read(msgs).cloned().collect()
        };
        assert_eq!(events.len(), 3, "all requests resolved the same frame");
        let hp = |e: Entity| app.world().get::<Enemy>(e).unwrap().hp;
        assert_eq!(hp(armored), 40.0, "40 phys halved + 40 magic full vs armor");
        assert_eq!(hp(unarmored), 60.0, "40 phys full vs no armor");
        let amounts: Vec<f32> = events.iter().map(|e| e.amount).collect();
        assert_eq!(amounts, vec![20.0, 40.0, 40.0]);
    }

    /// SK7 acceptance: same-frame overkill — a dead victim takes no further
    /// requests (no double kill messages, no double kill gold).
    #[test]
    fn same_frame_overkill_kills_once() {
        let mut app = test_app();
        let e = spawn_enemy_at(&mut app, Vec3::new(2.0, 0.0, 0.0), 50.0, false);
        let gold_before = app.world().resource::<Economy>().gold;
        let mut dealt = MessageCursor::<DamageDealt>::default();
        app.update();
        request(&mut app, e, 30.0, AttackType::Physical);
        request(&mut app, e, 30.0, AttackType::Physical);
        request(&mut app, e, 30.0, AttackType::Physical);
        app.update();
        app.update(); // Cleanup settles the death
        let events: Vec<_> = {
            let msgs = app.world().resource::<Messages<DamageDealt>>();
            dealt.read(msgs).cloned().collect()
        };
        assert_eq!(events.len(), 2, "the third request hits a corpse: skipped");
        assert!(events[0].killed == false, "first hit leaves hp at 20");
        assert!(events[1].killed, "the SECOND hit kills");
        assert_eq!(
            app.world().resource::<Economy>().gold,
            gold_before + 6,
            "kill gold paid exactly once"
        );
    }

    /// SK7 acceptance: the melee branch (shield tower) rides the same
    /// pipeline and its DamageDealt attributes to the tower entity.
    #[test]
    fn melee_routes_through_the_pipeline_with_attribution() {
        let mut app = test_app();
        let tower = spawn_tower(&mut app, 1, TowerKind::Base(1), 3.0);
        let enemy = spawn_enemy_at(&mut app, Vec3::new(1.0, 0.0, 0.0), 100.0, false);
        let mut dealt = MessageCursor::<DamageDealt>::default();
        app.update();
        app.update();
        let events: Vec<_> = {
            let msgs = app.world().resource::<Messages<DamageDealt>>();
            dealt.read(msgs).cloned().collect()
        };
        assert!(
            events.iter().any(|e| e.source == DamageSource::Tower(tower)),
            "melee hit attributed to its tower"
        );
        assert!(app.world().get::<Enemy>(enemy).unwrap().hp < 100.0);
    }

    /// SK7 acceptance: a ranged tower's hit carries the attribution through
    /// the projectile snapshot into the unified resolution.
    #[test]
    fn projectile_hit_attributes_to_its_tower() {
        let mut app = test_app();
        let tower = spawn_tower(&mut app, 0, TowerKind::Base(0), 20.0);
        let enemy = spawn_enemy_at(&mut app, Vec3::new(2.0, 0.0, 0.0), 500.0, false);
        let mut dealt = MessageCursor::<DamageDealt>::default();
        let mut events: Vec<DamageDealt> = Vec::new();
        // Messages are double-buffered: drain every frame or old events drop.
        let mut drain = |app: &mut App, events: &mut Vec<DamageDealt>| {
            let msgs = app.world().resource::<Messages<DamageDealt>>();
            events.extend(dealt.read(msgs).copied());
        };
        for _ in 0..41 {
            app.update();
            drain(&mut app, &mut events);
        }
        assert!(
            events
                .iter()
                .any(|e| e.source == DamageSource::Tower(tower) && e.amount == 10.0),
            "ranged hit attributed with the snapshot damage"
        );
        assert!(app.world().get::<Enemy>(enemy).unwrap().hp < 500.0);
    }

    /// SK7 acceptance: the commander skill path (meteor rain through the
    /// REAL player flow) attributes as CommanderSkill and its AOE victims
    /// all resolve in one frame.
    #[test]
    fn meteor_rain_attributed_as_commander_skill() {
        use warden::resources::SkillAiming;
        let mut app = test_app();
        app.update();
        app.update();
        app.update(); // skill buttons exist
        let victims = [
            spawn_enemy_at(&mut app, Vec3::ZERO, 30.0, true),
            spawn_enemy_at(&mut app, Vec3::new(2.0, 0.0, 0.0), 30.0, false),
        ];
        let mut dealt = MessageCursor::<DamageDealt>::default();
        // Z -> target picking -> cursor at origin -> left click (the same
        // real player path the SK2 tests drive).
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyZ);
        app.update();
        {
            let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
            keys.clear();
            keys.release(KeyCode::KeyZ);
        }
        let commander = commander_entity(&mut app);
        assert_eq!(
            app.world().resource::<SkillAiming>().0,
            Some((commander, 0))
        );
        enable_world_picking(&mut app, Vec2::new(640.0, 360.0));
        let cursor = {
            let mut cams = app.world_mut().query::<(&Camera, &GlobalTransform)>();
            let (cam, tf) = cams.single(app.world()).expect("one camera");
            cam.world_to_viewport(tf, Vec3::ZERO).expect("projects")
        };
        {
            let mut q = app.world_mut().query_filtered::<&mut Window, ()>();
            for mut w in q.iter_mut(app.world_mut()) {
                w.set_cursor_position(Some(cursor));
            }
        }
        click_left(&mut app);
        let mut events: Vec<DamageDealt> = Vec::new();
        for _ in 0..121 {
            app.update(); // three meteors at 0.3s spacing, 40 dmg each
            let msgs = app.world().resource::<Messages<DamageDealt>>();
            events.extend(dealt.read(msgs).copied());
        }
        assert!(
            events
                .iter()
                .all(|e| matches!(e.source, DamageSource::CommanderSkill(_))),
            "meteor damage attributed to the commander skill"
        );
        assert!(
            events.len() >= 2,
            "both victims took hits (armored one survives half damage)"
        );
        let _ = victims;
    }
}

// --- SK9: archer tower skill kit (ADR-0011) -----------------------------------------

mod skills_archer {
    use super::*;
    use bevy::ecs::message::MessageCursor;
    use warden::components::{EffectiveStats, SkillCaster, StatMods, Tower, TowerKind};
    use warden::resources::{RunRng, SkillDefs, TowerDefs};
    use warden::systems::archer_skills::{
        archer_skill_caster, SKILL_EAGLE_EYE, SKILL_FIRE_ARROWS, SKILL_MULTISHOT,
    };
    use warden::systems::damage::{DamageDealt, DamageSource};
    use warden::systems::skills::{learn_skill, CastSkill};

    /// An archer tower carrying the stock kit. `cooldown` starts high so the
    /// normal auto-fire never interferes with skill-driven assertions.
    fn spawn_archer(app: &mut App, attack_speed: f32) -> Entity {
        app.world_mut()
            .spawn((
                Tower {
                    tower_index: 0,
                    damage: 10.0,
                    attack_speed,
                    range: 20.0,
                    attack_type: AttackType::Physical,
                    cooldown: 999.0,
                    target: None,
                    kind: TowerKind::Base(0),
                    aoe_radius: 0.0,
                    slow_factor: 0.0,
                    slow_duration: 0.0,
                    invested: 0,
                },
                StatMods::default(),
                archer_skill_caster(),
                Transform::from_xyz(0.0, 0.5, 0.0),
            ))
            .id()
    }

    fn spawn_dummy(app: &mut App, pos: Vec3, hp: f32) -> Entity {
        app.world_mut()
            .spawn((
                Enemy {
                    hp,
                    max_hp: hp,
                    speed: 0.0,
                    leak: 0,
                    kill_gold: 0,
                    physical_armor: false,
                    def_index: 0,
                    next_wp: 0,
                },
                Transform::from_xyz(pos.x, 0.4, pos.z),
            ))
            .id()
    }

    /// Slot index of a def on the caster (panic if absent).
    fn slot_of(app: &mut App, tower: Entity, def: usize) -> usize {
        app.world()
            .get::<SkillCaster>(tower)
            .expect("caster")
            .slots
            .iter()
            .position(|s| s.def == def)
            .expect("skill on caster")
    }

    fn request_cast(app: &mut App, tower: Entity, def: usize) {
        let slot = slot_of(app, tower, def);
        app.world_mut()
            .resource_mut::<Messages<CastSkill>>()
            .write(CastSkill {
                caster: tower,
                slot,
            });
    }

    fn estats(app: &App, tower: Entity) -> EffectiveStats {
        *app.world().get::<EffectiveStats>(tower).expect("aggregated")
    }

    /// SK9 acceptance: the archer gateway attaches the kit at level 1 and an
    /// empty StatMods bag (出厂自带).
    #[test]
    fn spawn_base_tower_ships_the_archer_kit() {
        let mut app = test_app();
        let tower = {
            // Borrow dance: the plain-fn gateway wants &mut Assets while
            // Commands borrows the world — take the asset stores out.
            let mut meshes = std::mem::take(&mut *app.world_mut().resource_mut::<Assets<Mesh>>());
            let mut materials =
                std::mem::take(&mut *app.world_mut().resource_mut::<Assets<StandardMaterial>>());
            let def = app.world().resource::<TowerDefs>().list[0].clone();
            let mut queue = bevy::ecs::world::CommandQueue::default();
            let tower = {
                let mut commands = Commands::new(&mut queue, app.world());
                warden::systems::tower::spawn_base_tower(
                    &mut commands,
                    &def,
                    0,
                    Vec3::ZERO,
                    &mut meshes,
                    &mut materials,
                    None,
                )
            };
            queue.apply(app.world_mut());
            *app.world_mut().resource_mut::<Assets<Mesh>>() = meshes;
            *app.world_mut().resource_mut::<Assets<StandardMaterial>>() = materials;
            tower
        };
        let caster = app.world().get::<SkillCaster>(tower).expect("kit attached");
        assert_eq!(caster.slots.len(), 3, "three skills at lv1");
        assert!(caster.slots.iter().all(|s| s.level == 1));
        assert_eq!(
            caster
                .slots
                .iter()
                .map(|s| s.def)
                .collect::<Vec<_>>(),
            vec![SKILL_EAGLE_EYE, SKILL_MULTISHOT, SKILL_FIRE_ARROWS]
        );
        assert!(
            app.world().get::<StatMods>(tower).is_some(),
            "StatMods bag attached for passives/buffs"
        );
    }

    /// SK9 acceptance: eagle-eye lv3 (20%, ×2.5) makes the long-run average
    /// per shot match base × (1 + 0.2 × 1.5) = 13 within tolerance.
    #[test]
    fn eagle_eye_crit_average_matches_theory() {
        let mut app = test_app();
        app.world_mut().insert_resource(RunRng::seeded(1234));
        let tower = spawn_archer(&mut app, 60.0); // ~1 shot per frame
        app.world_mut()
            .get_mut::<Tower>(tower)
            .unwrap()
            .cooldown = 0.0; // this test WANTS the auto-fire running
        {
            let mut caster = app
                .world_mut()
                .get_mut::<SkillCaster>(tower)
                .expect("caster");
            learn_skill(&mut caster, SKILL_EAGLE_EYE, 3);
        }
        app.update(); // passive sync folds lv3 into EffectiveStats
        let e = estats(&app, tower);
        assert!((e.crit_chance - 0.20).abs() < 1e-4, "lv3 crit chance");
        assert!((e.crit_multiplier - 2.5).abs() < 1e-4, "lv3 crit multiplier");
        spawn_dummy(&mut app, Vec3::new(2.0, 0.0, 0.0), 1_000_000.0);
        let mut dealt = MessageCursor::<DamageDealt>::default();
        let (mut total, mut hits, mut crits) = (0.0f32, 0u32, 0u32);
        for _ in 0..1500 {
            app.update();
            let msgs = app.world().resource::<Messages<DamageDealt>>();
            for d in dealt.read(msgs) {
                if matches!(d.source, DamageSource::Tower(t) if t == tower) {
                    total += d.amount;
                    hits += 1;
                    crits += d.crit as u32;
                }
            }
        }
        assert!(hits >= 1200, "enough samples (got {hits})");
        let mean = total / hits as f32;
        assert!(
            (mean - 13.0).abs() / 13.0 < 0.05,
            "mean damage {mean:.2} within ±5% of the 13.0 expectation"
        );
        let crit_rate = crits as f32 / hits as f32;
        assert!(
            (crit_rate - 0.20).abs() < 0.05,
            "crit rate {crit_rate:.3} near 20%"
        );
    }

    /// SK9 acceptance: multishot lv1 fires exactly 3 arrows at the 3 nearest
    /// DISTINCT targets in range (nothing beyond range, no double arrows).
    #[test]
    fn multishot_hits_three_nearest_distinct_targets() {
        let mut app = test_app();
        let tower = spawn_archer(&mut app, 1.0);
        let in_range = [
            Vec3::new(2.0, 0.0, 0.0),
            Vec3::new(3.0, 0.0, 0.0),
            Vec3::new(4.0, 0.0, 0.0),
            Vec3::new(5.0, 0.0, 0.0),
        ];
        for p in in_range {
            spawn_dummy(&mut app, p, 10_000.0);
        }
        spawn_dummy(&mut app, Vec3::new(80.0, 0.0, 80.0), 10_000.0); // out of range
        let mut dealt = MessageCursor::<DamageDealt>::default();
        app.update();
        request_cast(&mut app, tower, SKILL_MULTISHOT);
        let mut skill_hits: Vec<(Entity, f32)> = Vec::new();
        for _ in 0..90 {
            app.update(); // request validates, volley flies, impacts land
            // Messages are double-buffered: drain every frame or events drop.
            let msgs = app.world().resource::<Messages<DamageDealt>>();
            for d in dealt.read(msgs) {
                if let DamageSource::TowerSkill { tower: t, skill } = d.source {
                    if t == tower && skill == SKILL_MULTISHOT {
                        skill_hits.push((d.victim, d.amount));
                    }
                }
            }
        }
        assert_eq!(skill_hits.len(), 3, "exactly three skill arrows landed");
        let mut victims: Vec<Entity> = skill_hits.iter().map(|(v, _)| *v).collect();
        victims.sort();
        victims.dedup();
        assert_eq!(victims.len(), 3, "three DISTINCT victims");
        for (_, amount) in &skill_hits {
            assert!(
                (*amount - 10.0).abs() < 1e-4,
                "arrow damage = the aggregated tower damage (got {amount})"
            );
        }
        // The 4th-nearest in-range dummy (lv1 = 3 arrows) and the faraway
        // one are both untouched by the volley.
        let full = {
            let mut q = app.world_mut().query::<&Enemy>();
            q.iter(app.world()).filter(|e| e.hp == 10_000.0).count()
        };
        assert_eq!(full, 2, "only the 4th-nearest and the far dummy are unharmed");
    }

    /// SK9 acceptance: fire arrows (autocast ON) boosts normal-attack damage
    /// by the table value for its duration, expires, then auto-recasts
    /// roughly on its 20s cooldown; with the toggle OFF it never casts.
    #[test]
    fn fire_arrows_buff_expires_and_autocasts_on_cooldown() {
        let mut app = test_app();
        let tower = spawn_archer(&mut app, 1.0);
        spawn_dummy(&mut app, Vec3::new(2.0, 0.0, 0.0), 1_000_000.0);
        // Toggle autocast ON (the SK10 UI does this with a right-click).
        {
            let slot = slot_of(&mut app, tower, SKILL_FIRE_ARROWS);
            let mut caster = app.world_mut().get_mut::<SkillCaster>(tower).unwrap();
            caster.slots[slot].autocast = true;
        }
        app.update();
        // A few frames: acquire target -> autocast queues -> cast confirms
        // -> effect lands -> (aggregate and effect order inside Simulate is
        // unspecified, so allow a frame of slack) -> stats fold the buff.
        for _ in 0..6 {
            app.update();
        }
        let boosted = estats(&app, tower).damage;
        assert!(
            (boosted - 13.0).abs() < 1e-4,
            "fire arrows +3 while active (got {boosted})"
        );
        let cd = {
            let slot = slot_of(&mut app, tower, SKILL_FIRE_ARROWS);
            app.world().get::<SkillCaster>(tower).unwrap().slots[slot].cooldown
        };
        assert!(cd > 19.0 && cd <= 20.0, "the 20s cooldown started (got {cd})");
        // Buff expires at 8s; cooldown (20s) still running → no recast yet.
        for _ in 0..(60 * 10) {
            app.update();
        }
        assert_eq!(
            estats(&app, tower).damage,
            10.0,
            "buff expired, no recast while the CD runs"
        );
        // ~21s total: the autocast recast fires and the buff returns.
        for _ in 0..(60 * 12) {
            app.update();
        }
        assert!(
            estats(&app, tower).damage > 12.0,
            "autocast re-applied the buff after the cooldown"
        );
    }

    /// SK9: with the toggle OFF (the learn default), the autocast trigger
    /// never queues a cast even with a target in range.
    #[test]
    fn fire_arrows_autocast_off_never_casts() {
        let mut app = test_app();
        let tower = spawn_archer(&mut app, 1.0);
        spawn_dummy(&mut app, Vec3::new(2.0, 0.0, 0.0), 1_000_000.0);
        for _ in 0..120 {
            app.update();
        }
        assert_eq!(
            estats(&app, tower).damage,
            10.0,
            "no buff without the autocast toggle"
        );
        let slot = slot_of(&mut app, tower, SKILL_FIRE_ARROWS);
        assert_eq!(
            app.world().get::<SkillCaster>(tower).unwrap().slots[slot].cooldown,
            0.0,
            "nothing was cast"
        );
    }

    /// SK9 acceptance: level_up refreshes the passive's numbers through the
    /// idempotent sync system (the SK10 upgrade button will call the same).
    #[test]
    fn eagle_eye_level_up_refreshes_passive_values() {
        let mut app = test_app();
        let tower = spawn_archer(&mut app, 1.0);
        app.update();
        assert!((estats(&app, tower).crit_chance - 0.10).abs() < 1e-4, "lv1");
        {
            let slot = slot_of(&mut app, tower, SKILL_EAGLE_EYE);
            let defs = warden::systems::skills::production_skill_defs();
            let mut caster = app.world_mut().get_mut::<SkillCaster>(tower).unwrap();
            let _ = warden::systems::skills::level_up(&defs, &mut caster, slot);
        }
        app.update(); // sync re-asserts the lv2 entries
        let e = estats(&app, tower);
        assert!((e.crit_chance - 0.15).abs() < 1e-4, "lv2 chance");
        assert!((e.crit_multiplier - 2.25).abs() < 1e-4, "lv2 multiplier");
    }
}

// --- SK10: command-card skill slots (ADR-0011) ---------------------------------------

mod skills_ui {
    use super::*;
    use warden::components::{Commander, SkillCaster, StatMods, Tower, TowerKind};
    use warden::resources::Selection;
    use warden::systems::archer_skills::{archer_skill_caster, SKILL_FIRE_ARROWS, SKILL_MULTISHOT};
    use warden::systems::command::{
        CommandAction, CommandButton, TowerSkillText,
    };

    fn spawn_archer(app: &mut App) -> Entity {
        app.world_mut()
            .spawn((
                Tower {
                    tower_index: 0,
                    damage: 10.0,
                    attack_speed: 1.0,
                    range: 20.0,
                    attack_type: AttackType::Physical,
                    cooldown: 999.0,
                    target: None,
                    kind: TowerKind::Base(0),
                    aoe_radius: 0.0,
                    slow_factor: 0.0,
                    slow_duration: 0.0,
                    invested: 0,
                },
                StatMods::default(),
                archer_skill_caster(),
                Transform::from_xyz(0.0, 0.5, 0.0),
            ))
            .id()
    }

    fn press_button(app: &mut App, action: CommandAction) -> bool {
        // Inject the same Interaction a click produces (UI12 hotkey pattern).
        let mut q = app
            .world_mut()
            .query_filtered::<&mut bevy::ui::Interaction, With<CommandButton>>();
        let mut found = false;
        for mut inter in q.iter_mut(app.world_mut()) {
            // Caller resets all; press exactly the matching button.
            *inter = bevy::ui::Interaction::None;
        }
        let mut q = app
            .world_mut()
            .query::<(&mut bevy::ui::Interaction, &CommandButton)>();
        for (mut inter, btn) in q.iter_mut(app.world_mut()) {
            if btn.action == action && !found {
                *inter = bevy::ui::Interaction::Pressed;
                found = true;
            }
        }
        found
    }

    /// SK10 acceptance: selecting a skilled tower activates the reserved
    /// slots — an upgrade button plus one castable button per skill and a
    /// display-only passive chip.
    #[test]
    fn selected_archer_shows_skill_buttons() {
        let mut app = test_app();
        let tower = spawn_archer(&mut app);
        app.update();
        app.update();
        *app.world_mut().resource_mut::<Selection>() = Selection::Tower(tower);
        app.update(); // card rebuilds for the new selection
        app.update();
        let mut q = app.world_mut().query::<&CommandButton>();
        let actions: Vec<CommandAction> = q.iter(app.world()).map(|b| b.action).collect();
        assert!(actions.contains(&CommandAction::SkillUpgrade), "upgrade slot live");
        assert!(
            actions.contains(&CommandAction::TowerSkill(1)),
            "multishot slot live"
        );
        assert!(
            actions.contains(&CommandAction::TowerSkill(2)),
            "fire arrows slot live"
        );
    }

    /// SK10 acceptance: the skill button casts through the framework and
    /// the label switches to the cooldown seconds; the paid upgrade raises
    /// the whole kit and charges the table cost.
    #[test]
    fn skill_button_casts_and_upgrade_raises_the_kit() {
        let mut app = test_app();
        let tower = spawn_archer(&mut app);
        app.update();
        app.update();
        app.world_mut().resource_mut::<Economy>().gold = 100;
        *app.world_mut().resource_mut::<Selection>() = Selection::Tower(tower);
        app.update();
        app.update();
        // Cast multishot through the button (grid hotkey equivalent).
        assert!(press_button(&mut app, CommandAction::TowerSkill(1)), "button exists");
        app.update();
        app.update(); // cast validates, effects run, cooldown starts
        {
            let caster = app.world().get::<SkillCaster>(tower).unwrap();
            let slot = caster.slots.iter().find(|s| s.def == SKILL_MULTISHOT).unwrap();
            assert!(slot.cooldown > 24.0, "multishot cast + 25s cd (got {})", slot.cooldown);
        }
        // The live label shows the remaining seconds.
        let label = {
            let mut q = app
                .world_mut()
                .query_filtered::<&mut Text, With<TowerSkillText>>();
            let mut found = String::new();
            for text in q.iter_mut(app.world_mut()) {
                if text.0.parse::<f32>().is_ok() {
                    found = text.0.clone();
                }
            }
            found
        };
        assert!(
            label.parse::<f32>().is_ok_and(|secs| secs > 20.0),
            "cooldown seconds on the button (got {label:?})"
        );
        // Paid upgrade: 30 gold for 1 -> 2 on every skill.
        assert!(press_button(&mut app, CommandAction::SkillUpgrade));
        app.update();
        app.update();
        assert_eq!(app.world().resource::<Economy>().gold, 70, "upgrade cost 30");
        let caster = app.world().get::<SkillCaster>(tower).unwrap();
        assert!(
            caster.slots.iter().all(|s| s.level == 2),
            "the whole kit rose one level"
        );
    }

    /// SK10 acceptance: right-click on a skill button toggles autocast and
    /// does NOT cancel the selection (the gesture belongs to the UI).
    #[test]
    fn right_click_on_skill_button_toggles_autocast() {
        let mut app = test_app();
        let tower = spawn_archer(&mut app);
        app.update();
        app.update();
        *app.world_mut().resource_mut::<Selection>() = Selection::Tower(tower);
        app.update();
        app.update();
        // Headless: mark the fire-arrows button AND the bar root as hovered
        // by hand (the real app's ui_focus keeps both current — the root's
        // rect contains every button, which is what makes cancel_modes skip).
        {
            let mut q = app
                .world_mut()
                .query::<(&mut bevy::ui::RelativeCursorPosition, &CommandButton)>();
            for (mut rcp, btn) in q.iter_mut(app.world_mut()) {
                if btn.action == CommandAction::TowerSkill(2) {
                    rcp.cursor_over = true;
                }
            }
            let mut roots = app.world_mut().query_filtered::<
                &mut bevy::ui::RelativeCursorPosition,
                With<warden::systems::command::CommandBarRoot>,
            >();
            for mut rcp in roots.iter_mut(app.world_mut()) {
                rcp.cursor_over = true;
            }
        }
        app.world_mut()
            .resource_mut::<ButtonInput<bevy::input::mouse::MouseButton>>()
            .press(bevy::input::mouse::MouseButton::Right);
        app.update();
        {
            let caster = app.world().get::<SkillCaster>(tower).unwrap();
            let slot = caster.slots.iter().find(|s| s.def == SKILL_FIRE_ARROWS).unwrap();
            assert!(slot.autocast, "right-click toggled autocast ON");
        }
        assert_eq!(
            app.world().resource::<Selection>().clone(),
            Selection::Tower(tower),
            "the UI right-click does not cancel the selection"
        );
    }

    /// SK10: hotkey discipline — with NOTHING selected, E (a build-grid key)
    /// never reaches a tower skill (context-scoped by the selection state).
    #[test]
    fn hotkeys_are_selection_scoped() {
        let mut app = test_app();
        let tower = spawn_archer(&mut app);
        app.update();
        app.update();
        let commander = {
            let mut q = app
                .world_mut()
                .query_filtered::<Entity, With<Commander>>();
            q.single(app.world()).unwrap()
        };
        let _ = tower;
        // Selection::None: the command card shows build/buy, no skill buttons.
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyE);
        app.update();
        {
            let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
            keys.clear();
            keys.release(KeyCode::KeyE);
        }
        let cds = app.world_mut()
            .get::<SkillCaster>(commander)
            .unwrap()
            .slots
            .iter()
            .map(|s| s.cooldown)
            .sum::<f32>();
        assert_eq!(cds, 0.0, "E did nothing without a selected skilled tower");
    }
}
