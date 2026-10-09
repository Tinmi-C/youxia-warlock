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
    world.insert_resource(SelectedTower("arrow".into()));
    info!("[loop-td] map loaded: {}", path.display());
}

/// Delegate to the base (kept as a game-side name for tests/ callers).
pub fn build_grid_from(bundle: &MapBundle) -> BuildGrid {
    warbase::bundle::build_grid(&bundle.world)
}

/// Update: keyboard cursor + selection + place/sell. Runs only with real
/// input (headless tests call try_place/try_sell directly). Exclusive
/// system: input snapshot is cloned out of the world first.
pub fn build_input(world: &mut World) {
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

    for (key, id) in [
        (KeyCode::Digit1, "arrow"),
        (KeyCode::Digit2, "cannon"),
        (KeyCode::Digit3, "mage"),
        (KeyCode::Digit4, "sentry"),
        (KeyCode::Digit5, "ice"),
        (KeyCode::Digit6, "drum"),
        (KeyCode::Digit7, "corrode"),
        (KeyCode::Digit8, "hunter"),
    ] {
        if keys.just_pressed(key) {
            world.resource_mut::<SelectedTower>().0 = id.into();
        }
    }

    let cell = world.resource::<BuildCursor>().0;
    if keys.just_pressed(KeyCode::Space) || keys.just_pressed(KeyCode::Enter) {
        let id = world.resource::<SelectedTower>().0.clone();
        if let Err(e) = try_place(world, &id, cell) {
            info!("[loop-td] place failed: {e}");
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
