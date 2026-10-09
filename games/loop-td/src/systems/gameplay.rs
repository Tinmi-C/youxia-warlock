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
