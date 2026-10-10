//! Gameplay core (map load, input). Thin-game rule: everything here
//! composes warbase mechanisms — placement/selling/grid construction live in
//! the base (genre-generic), this crate only wires them to input/UI and the
//! map file.

use bevy::prelude::*;
use warbase::bundle::{assemble, load_bundle, MapBundle};
use warbase::prelude::*;

// Base-level placement (hoisted during BX10 prep — the harness drives the
// same path real input does).
pub use warbase::build::{try_place, try_sell, try_upgrade, Buildable};

/// The loaded map (kept around for rebuilds / UI).
#[derive(Resource)]
pub struct CurrentMap(pub MapBundle);

/// Keyboard-driven build cursor (grid cell).
#[derive(Resource, Debug)]
pub struct BuildCursor(pub Cell);

/// Currently selected tower def id.
#[derive(Resource, Debug)]
pub struct SelectedTower(pub String);

/// Default map: the crate's own maps/loop.ron. Override with `--map <path>`
/// or env `LOOP_TD_MAP`.
pub fn default_map_path() -> std::path::PathBuf {
    if let Ok(p) = std::env::var("LOOP_TD_MAP") {
        return p.into();
    }
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        if a == "--map" {
            if let Some(p) = args.next() {
                return p.into();
            }
        }
    }
    std::path::PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/maps/loop.ron"))
}

/// Exclusive startup: load + validate + assemble + build grid + cursor.
pub fn load_map(world: &mut World) {
    let path = default_map_path();
    let bundle = match load_bundle(&path) {
        Ok(b) => b,
        Err(e) => panic!("[loop-td] failed to load map {}: {e}", path.display()),
    };
    assemble(world, &bundle);
    let grid = warbase::bundle::build_grid(&bundle.world);
    world.insert_resource(grid);
    world.insert_resource(CurrentMap(bundle));
    world.insert_resource(BuildCursor(Cell(10, 8)));
    // Command card (BX4b): auto-generated build menu from the map's defs.
    {
        let card = warbase::ui::build_card_from_registry(
            world.resource::<UnitRegistry>(),
        );
        world.resource_mut::<warbase::ui::CommandCards>()
            .cards
            .insert(warbase::ui::BUILD_CARD.to_string(), card);
    }
    world.insert_resource(warbase::ui::Selection {
        mode: warbase::ui::SelectionMode::Build,
    });
    // Hero kit (HR2): hunter's signature autocast — the game-side effect
    // system dispatches the SkillCast by id.
    {
        let mut defs = SkillDefs::default();
        defs.defs.insert(
            "snipe".into(),
            SkillDef {
                id: "snipe".into(),
                kind: SkillKind::Autocast,
                targeting: SkillTargeting::None,
                max_level: 3,
                cooldown: 6.0,
            },
        );
        world.insert_resource(defs);
    }
    info!("[loop-td] map loaded: {}", path.display());
}

/// Delegate to the base (kept as a game-side name for tests/ callers).
pub fn build_grid_from(bundle: &MapBundle) -> BuildGrid {
    warbase::bundle::build_grid(&bundle.world)
}

/// Update: keyboard cursor + selection mode toggle. Build/sell/upgrade now
/// flow through the base command card (command_input -> CommandCardAction
/// -> command_dispatch below); tests can still call try_place directly.
pub fn build_input(world: &mut World) {
    if !world.contains_resource::<ButtonInput<KeyCode>>() {
        return;
    }
    let keys: ButtonInput<KeyCode> = world.resource::<ButtonInput<KeyCode>>().clone();
    // Cursor movement (clamped to the grid).
    let (cols, rows) = {
        let grid = world.resource::<BuildGrid>();
        (grid.cols, grid.rows)
    };
    let mut cursor = world.resource_mut::<BuildCursor>();
    let (dx, dy) = [
        (KeyCode::KeyA, -1, 0),
        (KeyCode::ArrowLeft, -1, 0),
        (KeyCode::KeyD, 1, 0),
        (KeyCode::ArrowRight, 1, 0),
        (KeyCode::KeyW, 0, 1),
        (KeyCode::ArrowUp, 0, 1),
        (KeyCode::KeyS, 0, -1),
        (KeyCode::ArrowDown, 0, -1),
    ]
    .into_iter()
    .filter(|(k, _, _)| keys.just_pressed(*k))
    .fold((0, 0), |acc, (_, dx, dy)| (acc.0 + dx, acc.1 + dy));
    if dx != 0 || dy != 0 {
        let Cell(cx, cy) = cursor.0;
        cursor.0 = Cell((cx + dx).clamp(0, cols - 1), (cy + dy).clamp(0, rows - 1));
    }
    drop(cursor);
    if keys.just_pressed(KeyCode::KeyB) {
        let mut sel = world.resource_mut::<warbase::ui::Selection>();
        sel.mode = match sel.mode {
            warbase::ui::SelectionMode::Build => warbase::ui::SelectionMode::None,
            _ => warbase::ui::SelectionMode::Build,
        };
    }
    // Legacy at-cursor keys (kept until unit selection lands).
    let cell = world.resource::<BuildCursor>().0;
    if keys.just_pressed(KeyCode::KeyG) {
        // HR2: spend a banked point on the hero at the cursor.
        if let Some(hero) = world.resource::<BuildGrid>().occupant(cell) {
            if let Err(e) = spend_skill_point(world, hero, 0) {
                info!("[loop-td] skill point: {e}");
            }
        }
    }
    if keys.just_pressed(KeyCode::KeyX) {
        if let Err(e) = try_sell(world, cell) {
            info!("[loop-td] sell failed: {e}");
        }
    }
    if keys.just_pressed(KeyCode::KeyU) {
        if let Err(e) = try_upgrade(world, cell) {
            info!("[loop-td] upgrade failed: {e}");
        }
    }
}

/// HR2 effect: hunter's signature autocast — dispatch SkillCast("snipe")
/// into a big hit on the caster's current target.
pub fn snipe_effect(world: &mut World) {
    let casts: Vec<SkillCast> = {
        let msgs = world.resource::<bevy::ecs::message::Messages<SkillCast>>();
        let mut cursor = bevy::ecs::message::MessageCursor::default();
        cursor.read(msgs).filter(|c| c.skill == "snipe").cloned().collect()
    };
    for cast in casts {
        let Some(target) = world
            .get::<CurrentTarget>(cast.caster)
            .and_then(|t| t.0)
        else {
            continue;
        };
        world
            .resource_mut::<bevy::ecs::message::Messages<DamageRequest>>()
            .write(DamageRequest {
                target,
                source: cast.caster,
                amount: 60.0 * cast.level as f32,
                attack_type: AttackType::Piercing,
                crit: false,
            });
    }
}

/// HR3: milestone talent choice (levels 5/10/15/20 — every 5). While a
/// choice is pending, keys 1/2/3 pick one of three permanent growth picks.
#[derive(Resource, Debug, Clone, Copy)]
pub struct PendingTalent {
    pub hero: Entity,
}

pub fn hero_talents(world: &mut World) {
    // Offer: any hero whose level crossed the next multiple of 5.
    let mut offer: Option<Entity> = None;
    {
        let mut q = world.query_filtered::<Entity, With<XpHolder>>();
        for e in q.iter(world) {
            let xp = world.get::<XpHolder>(e).unwrap();
            if xp.level / 5 > xp.talents_taken {
                offer = Some(e);
                break;
            }
        }
    }
    if let Some(hero) = offer {
        if world.get_resource::<PendingTalent>().is_none() {
            world.insert_resource(PendingTalent { hero });
        }
    }
    let Some(pending) = world.get_resource::<PendingTalent>().cloned() else { return };
    if !world.contains_resource::<ButtonInput<KeyCode>>() {
        return;
    }
    let keys = world.resource::<ButtonInput<KeyCode>>().clone();
    let choice = [
        (KeyCode::Digit1, 0u8),
        (KeyCode::Digit2, 1),
        (KeyCode::Digit3, 2),
    ]
    .into_iter()
    .find(|(k, _)| keys.just_pressed(*k))
    .map(|(_, c)| c);
    let Some(choice) = choice else { return };

    // Three permanent picks (numbers are map-side tuning territory later).
    let picks: [(StatKind, f32); 3] = [
        (StatKind::DamageMult, 0.12),            // 力：+12% 伤害
        (StatKind::AttackIntervalMult, -0.12),   // 敏：-12% 攻击间隔
        (StatKind::DamageMult, 0.06),            // 均：+6% 伤害 & -6% 间隔
    ];
    let mut grants = vec![picks[choice as usize]];
    if choice == 2 {
        grants.push((StatKind::AttackIntervalMult, -0.06));
    }
    for (kind, amount) in grants {
        // Unique source per grant so multiple talents of the same kind stack.
        let src = world.spawn_empty().id();
        let Ok(mut holder) = world.get_entity_mut(pending.hero) else { continue };
        let mut mods = holder.entry::<StatMods>().or_default();
        mods.into_mut().upsert(ModEntry {
            source: src,
            kind,
            amount,
            expires_at: None,
            from_aura: false,
        });
    }
    if let Ok(mut holder) = world.get_entity_mut(pending.hero) {
        if let Some(mut xp) = holder.get_mut::<XpHolder>() {
            xp.talents_taken += 1;
        }
    }
    world.remove_resource::<PendingTalent>();
}

// ---------------------------------------------------------------------------
// MT1: cross-run meta. Points bank from cleared waves; the end screen buys
// persistent start-gold ranks; R restarts the match with bonuses applied.
// ---------------------------------------------------------------------------

#[derive(Resource, Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct MetaState {
    pub points: u32,
    pub start_gold_bonus: u32,
}

impl Default for MetaState {
    fn default() -> Self {
        Self { points: 0, start_gold_bonus: 0 }
    }
}

pub const META_UPGRADE_COST: u32 = 10;
pub const META_UPGRADE_GOLD: u32 = 20;

pub fn meta_path() -> std::path::PathBuf {
    std::path::PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/meta_save.txt"))
}

/// Load is pure — never deletes (a stray delete here would wipe real saves).
/// Tests inject MetaState directly instead of touching the file.
pub fn load_meta(path: &std::path::Path) -> MetaState {
    match std::fs::read_to_string(path) {
        Ok(s) => serde_json::from_str(&s).unwrap_or_default(),
        Err(_) => MetaState::default(),
    }
}

/// Startup: real game loads the save; tests inject their own MetaState.
pub fn load_meta_startup(world: &mut World) {
    if world.contains_resource::<MetaState>() {
        return;
    }
    let meta = load_meta(&meta_path());
    world.insert_resource(meta);
}

pub fn save_meta(path: &std::path::Path, meta: &MetaState) {
    if let Ok(json) = serde_json::to_string(meta) {
        let _ = std::fs::write(path, json);
    }
}

/// Bank the run's cleared waves into points exactly once (idempotent via
/// the marker resource).
#[derive(Resource)]
pub struct RunBanked;

pub fn bank_run(world: &mut World) {
    if *world.resource::<MatchFlow>() == MatchFlow::Running {
        return;
    }
    if world.get_resource::<RunBanked>().is_some() {
        return;
    }
    let cleared = world.resource::<WaveDirector>().next_wave as u32;
    world.resource_mut::<MetaState>().points += cleared;
    world.insert_resource(RunBanked);
    let path = meta_path();
    let meta = world.resource::<MetaState>().clone();
    save_meta(&path, &meta);
}

/// End-screen input: M buys a start-gold rank, R restarts with bonuses.
pub fn end_screen_input(world: &mut World) {
    if *world.resource::<MatchFlow>() == MatchFlow::Running {
        return;
    }
    if !world.contains_resource::<ButtonInput<KeyCode>>() {
        return;
    }
    let keys = world.resource::<ButtonInput<KeyCode>>().clone();
    if keys.just_pressed(KeyCode::KeyM) {
        let mut meta = world.resource_mut::<MetaState>();
        if meta.points >= META_UPGRADE_COST {
            meta.points -= META_UPGRADE_COST;
            meta.start_gold_bonus += META_UPGRADE_GOLD;
            let snapshot = meta.clone();
            drop(meta);
            save_meta(&meta_path(), &snapshot);
            info!("[loop-td] meta: start gold +{}/rank", META_UPGRADE_GOLD);
        }
    }
    if keys.just_pressed(KeyCode::KeyE) {
        world.insert_resource(EndlessNext);
        restart_run(world);
    }
    if keys.just_pressed(KeyCode::KeyR) {
        world.remove_resource::<EndlessNext>();
        restart_run(world);
    }
}

/// Fresh match from the loaded map, meta bonus applied (assemble resets
/// director/flow/econ/rules; towers/creeps/paths despawned).
pub fn restart_run(world: &mut World) {
    let mut bundle = world.resource::<CurrentMap>().0.clone();
    if world.get_resource::<EndlessNext>().is_some() {
        bundle.rules.win = WinCondition::Endless;
        info!("[loop-td] endless mode: next run cycles with +15%/pass");
    }
    // Despawn units and path entities (panel/HUD/camera stay).
    let mut to_despawn: Vec<Entity> = Vec::new();
    {
        let mut q = world.query_filtered::<Entity, With<UnitInstance>>();
        to_despawn.extend(q.iter(world));
    }
    if let Some(paths) = world.get_resource::<Paths>() {
        to_despawn.extend(paths.0.iter().copied());
    }
    for e in to_despawn {
        let _ = world.get_entity_mut(e).map(|mut em| em.despawn());
    }
    warbase::bundle::assemble(world, &bundle);
    let grid = warbase::bundle::build_grid(&bundle.world);
    world.insert_resource(grid);
    world.insert_resource(BuildCursor(Cell(10, 8)));
    world.remove_resource::<RunBanked>();
    let bonus = world.resource::<MetaState>().start_gold_bonus;
    world.resource_mut::<Resources>().gold += bonus;
    info!("[loop-td] run restarted (+{bonus} meta gold)");
}

// ---------------------------------------------------------------------------
// ST1: run stats (kills / stock peak / stock curve), score, end screen,
// and the endless entry (E at the end screen -> next run is Endless).
// ---------------------------------------------------------------------------

#[derive(Resource, Debug, Default)]
pub struct RunStats {
    pub kills: u32,
    pub stock_peak: u32,
    /// StockCount sampled every 30 frames (0.5s) — the settlement curve.
    pub stock_history: Vec<u32>,
    samples: u32,
}

#[derive(Resource, Default)]
struct DeathCursor(bevy::ecs::message::MessageCursor<UnitDied>);

pub fn track_stats(world: &mut World) {
    world.get_resource_or_insert_with(RunStats::default);
    world.get_resource_or_insert_with(DeathCursor::default);
    let deaths: Vec<UnitDied> = world.resource_scope(|w, mut c: Mut<DeathCursor>| {
        let msgs = w.resource::<bevy::ecs::message::Messages<UnitDied>>();
        c.0.read(msgs).cloned().collect()
    });
    let mut stats = world.resource_mut::<RunStats>();
    stats.kills += deaths.len() as u32;
    stats.samples += 1;
    if stats.samples % 30 == 0 {
        let stock = {
            drop(&mut stats);
            let s = world.resource::<StockCount>().0;
            s
        };
        let mut stats = world.resource_mut::<RunStats>();
        stats.stock_peak = stats.stock_peak.max(stock);
        stats.stock_history.push(stock);
    }
}

/// Settlement score: kills + waves + stock control + hero levels.
pub fn settlement_score(world: &mut World) -> u32 {
    let (kills, peak) = {
        let stats = world.resource::<RunStats>();
        (stats.kills, stats.stock_peak)
    };
    let waves = world.resource::<WaveDirector>().next_wave as u32;
    let cap = match &world.resource::<MatchRules>().leak {
        LeakPolicy::Stockpile { cap } => *cap,
        _ => 50,
    };
    let heroes: u32 = {
        let mut q = world.query_filtered::<&XpHolder, ()>();
        q.iter(world).map(|x| x.level as u32).sum()
    };
    kills * 10 + waves * 100 + cap.saturating_sub(peak) * 2 + heroes * 50
}

#[derive(Component)]
pub struct EndScreen;

#[derive(Resource, Default)]
struct EndScreenText(String);

pub fn refresh_end_screen(world: &mut World) {
    let flow = world.resource::<MatchFlow>().clone();
    if flow == MatchFlow::Running {
        if let Ok(e) = world.query_filtered::<Entity, With<EndScreen>>().single(world) {
            world.despawn(e);
            world.resource_mut::<EndScreenText>().0.clear();
        }
        return;
    }
    let next = {
        let waves = world.resource::<WaveDirector>().next_wave;
        let meta = world.resource::<MetaState>().clone();
        let score = settlement_score(world);
        let (kills, peak, history) = {
            let stats = world.resource::<RunStats>();
            (stats.kills, stats.stock_peak, stats.stock_history.clone())
        };
        // Compact stock curve: 20 buckets, '#' scaled to the cap.
        let cap = match &world.resource::<MatchRules>().leak {
            LeakPolicy::Stockpile { cap } => *cap,
            _ => 50,
        } as f32;
        let curve: String = {
            let h = &history;
            let step = (h.len() / 20).max(1);
            h.iter().step_by(step).take(20).map(|s| {
                let h_chars = ((s.to_owned() as f32 / cap) * 8.0).round() as usize;
                std::iter::repeat('#').take(h_chars.min(8)).collect::<String>()
                    + &std::iter::repeat('.').take((8 - h_chars.min(8))).collect::<String>()
                    + " "
            }).collect()
        };
        let outcome = match &flow {
            MatchFlow::Won(_) => format!("胜利！{}", score),
            MatchFlow::Lost(r) => format!("失败（{r}）得分 {score}"),
            MatchFlow::Running => unreachable!(),
        };
        format!(
            "{outcome}\n波次 {}  击杀 {}  存量峰值 {}\n存量曲线 {}\nMeta 点数 {}（加成 +{}金）  M 买开局+20金(10点)  E 无尽重开  R 普通重开",
            waves, kills, peak, curve.trim_end(), meta.points, meta.start_gold_bonus,
        )
    };
    world.get_resource_or_insert_with(EndScreenText::default);
    let mut key = world.resource_mut::<EndScreenText>();
    if key.0 == next {
        return;
    }
    key.0 = next.clone();
    match world.query_filtered::<Entity, With<EndScreen>>().single(world) {
        Ok(e) => {
            if let Some(mut txt) = world.get_mut::<Text>(e) {
                *txt = Text::new(next);
            }
        }
        Err(_) => {
            world.spawn((
                EndScreen,
                Text::new(next),
                TextFont { font_size: FontSize::Px(22.0), ..default() },
                TextColor(Color::WHITE),
                Node {
                    position_type: PositionType::Absolute,
                    top: Val::Px(60.0),
                    left: Val::Px(120.0),
                    ..default()
                },
            ));
        }
    }
}

/// Marker: next restart runs Endless (E at the end screen).
#[derive(Resource, Default)]
pub struct EndlessNext;

/// Dispatch base command-card actions onto the map (BX4b consumer side).
pub fn command_dispatch(world: &mut World) {    let events: Vec<warbase::ui::CommandCardAction> = {
        let msgs = world.resource::<bevy::ecs::message::Messages<warbase::ui::CommandCardAction>>();
        let mut cursor = bevy::ecs::message::MessageCursor::default();
        cursor.read(msgs).cloned().collect()
    };
    let cell = world.resource::<BuildCursor>().0;
    for event in events {
        match event.action {
            warbase::ui::CommandAction::Build(def) => {
                if let Err(e) = try_place(world, &def, cell) {
                    info!("[loop-td] place {def} failed: {e}");
                }
            }
            warbase::ui::CommandAction::Upgrade => {
                if let Err(e) = try_upgrade(world, cell) {
                    info!("[loop-td] upgrade failed: {e}");
                }
            }
            warbase::ui::CommandAction::Sell => {
                if let Err(e) = try_sell(world, cell) {
                    info!("[loop-td] sell failed: {e}");
                }
            }
            warbase::ui::CommandAction::Cast(_) => {}
        }
    }
}
