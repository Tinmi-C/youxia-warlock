//! Gameplay core: map load + assemble. Thin-game rule: mechanisms are
//! warbase's; this crate wires them to the map file and the thin RTS layer.

use bevy::prelude::*;
use warbase::bundle::{assemble, load_bundle};

use crate::resources::CurrentMap;

/// Default map: the crate's own maps/arena.ron. Override with `--map <path>`
/// or env `RTS_LAB_MAP`.
pub fn default_map_path() -> std::path::PathBuf {
    if let Ok(p) = std::env::var("RTS_LAB_MAP") {
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
    std::path::PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/maps/arena.ron"))
}

/// Exclusive startup: load + validate + assemble + build grid.
pub fn load_map(world: &mut World) {
    let path = default_map_path();
    let bundle = match load_bundle(&path) {
        Ok(b) => b,
        Err(e) => panic!("[rts-lab] failed to load map {}: {e}", path.display()),
    };
    assemble(world, &bundle);
    let grid = warbase::bundle::build_grid(&bundle.world);
    world.insert_resource(grid);
    world.insert_resource(CurrentMap(bundle));
    info!("[rts-lab] map loaded: {}", path.display());
}
