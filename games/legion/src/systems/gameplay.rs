//! Gameplay core: map load + per-player wallets + the thin legion kit layer.
//! Thin-game rule: mechanisms are warbase's; this crate wires them to the map
//! file and composes them into the round loop.

use bevy::prelude::*;
use warbase::bundle::{assemble, load_bundle};
use warbase::prelude::*;

use crate::resources::{CurrentMap, Purse};

/// Default map: the crate's own maps/arena.ron. Override with `--map <path>`
/// or env `LEGION_MAP`.
pub fn default_map_path() -> std::path::PathBuf {
    if let Ok(p) = std::env::var("LEGION_MAP") {
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
        Err(e) => panic!("[legion] failed to load map {}: {e}", path.display()),
    };
    assemble(world, &bundle);
    let grid = warbase::bundle::build_grid(&bundle.world);
    world.insert_resource(grid);
    world.insert_resource(CurrentMap(bundle));
    info!("[legion] map loaded: {}", path.display());
}

/// Exclusive startup (after load_map): seed the four wallets from the map's
/// `econ_start.gold` (per-player view of the base's single-player field —
/// the base `Resources` global stays untouched/unused by legion play).
pub fn init_purses(world: &mut World) {
    let start = world.resource::<CurrentMap>().0.econ_start.gold;
    world.insert_resource(Purse([start; 4]));
    info!("[legion] purses seeded: {start} gold x4");
}

// ---------------------------------------------------------------------------
// Legion kit attachment (every spawn path funnels here)
// ---------------------------------------------------------------------------

/// Def ids the game-side wiring references (must exist in maps/arena.ron).
pub const KEEP_DEF: &str = "keep";
pub const MERCENARY_DEFS: [&str; 6] =
    ["merc_cheap", "merc_swarm", "merc_tank", "merc_ranged", "merc_aoe", "merc_support"];

/// Attach the free-agent kit to freshly spawned units (Added<UnitInstance>
/// observer — covers starting_forces keeps and hired mercenaries alike):
/// - `Selectable` on every instance (keeps included, WC3 habit);
/// - `MoveAgent` + `AgentOrderState` when the def moves (speed 0 keeps stay
///   put — no agent, they are scenery anchors, not LastTeamStanding fighters).
/// Visual hierarchy note: base spawn gives units a bare Transform; mesh
/// children (visualize_units) want the parent to carry Visibility (bevy
/// B0004) — inserted once here, inert headless.
pub fn attach_legion_kit(
    added: Query<(Entity, &UnitInstance), Added<UnitInstance>>,
    registry: Res<UnitRegistry>,
    mut commands: Commands,
) {
    for (entity, instance) in &added {
        let Some(def) = registry.get(&instance.def) else { continue };
        let mut ec = commands.entity(entity);
        ec.insert(Selectable);
        if def.attrs.move_speed > 0.0 {
            ec.insert((
                MoveAgent {
                    speed: def.attrs.move_speed,
                    arrival_radius: 0.1,
                    waypoints: Vec::new(),
                    velocity: Vec3::ZERO,
                    body_radius: def.attrs.body_radius,
                    order: None,
                },
                AgentOrderState::Idle,
            ));
        }
        // Visibility comes last so a mover's kit insert and the render marker
        // land in one command batch.
        ec.insert(Visibility::default());
    }
}
