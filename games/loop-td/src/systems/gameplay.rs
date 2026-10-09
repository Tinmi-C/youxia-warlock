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

/// Dispatch base command-card actions onto the map (BX4b consumer side).
pub fn command_dispatch(world: &mut World) {
    let events: Vec<warbase::ui::CommandCardAction> = {
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
