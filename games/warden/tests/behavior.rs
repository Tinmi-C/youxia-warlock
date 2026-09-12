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
        game::GamePlugin, map::MapPlugin, meta::MetaPlugin, towers::TowersPlugin, ui::UiPlugin,
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
            GamePlugin,
            MapPlugin,
            TowersPlugin,
            EnemiesPlugin,
            WavesPlugin,
            EconomyPlugin,
            AcquisitionPlugin,
            MetaPlugin,
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
    app.update();
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
    app.update();
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
    app.update();
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
    app.update();
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
    app.update();
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

    // Start the wave: the preview must disappear during combat.
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
