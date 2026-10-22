//! Vision & fog of war (WC3 alignment, ADR-0016): two-layer grid (visible /
//! explored) fed by VisionSource carriers. Rule D5: player fog constrains
//! the interaction layer only (manual cast targets, selection, rendering) —
//! towers and autocast are unaffected (they carry their own VisionSource).
//!
//! FOG1 scope: circular unobstructed vision, all sources treated friendly
//! (team filtering lands with MP1/FOG2). The grid update runs in Tick;
//! `InFog` marking and rendering are FOG2 and do not exist here yet.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// Grants vision of `radius` world units around the carrier (towers, free
/// agents, buildings). Creeps may carry it for team-shared vision (MP1+).
#[derive(Component, Debug)]
pub struct VisionSource {
    pub radius: f32,
}

/// Present on entities currently outside every friendly vision source
/// (rendering dims/hides via the fx layer; interaction layers reject).
/// FOG2 (P3) — not populated by FOG1.
#[derive(Component, Debug)]
pub struct InFog;

/// Two-layer fog grid over the map (grid coords; explored only grows).
/// Cell layout matches the build grid: same origin and world extent. A cell
/// (cx, cy) covers the world square whose CENTER is
/// `origin + ((cx + 0.5) * cell, (cy + 0.5) * cell)`; coords are on the
/// logic plane (Vec2.x = world x, Vec2.y = world z), like BuildGrid.
#[derive(Resource, Debug, Default, Clone)]
pub struct VisionGrid {
    pub cell: f32,
    pub cols: i32,
    pub rows: i32,
    pub origin: Vec2,
    pub visible: std::collections::BTreeSet<(i32, i32)>,
    pub explored: std::collections::BTreeSet<(i32, i32)>,
}

/// Map-bundle vision section (SEAM0d; optional, old maps default).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VisionDef {
    /// Fog grid cell size in world units.
    #[serde(default = "default_cell")]
    pub cell: f32,
}

fn default_cell() -> f32 {
    1.0
}

/// Struct-level serde default (a whole missing section) must agree with the
/// field-level default above — both mean "one world unit per fog cell".
impl Default for VisionDef {
    fn default() -> Self {
        Self { cell: default_cell() }
    }
}

/// Recompute bookkeeping for the grid update: the source-set fingerprint and
/// how many full recomputes have run. Tests assert the skip path through
/// `recomputes`; it doubles as a perf readout on large maps.
#[derive(Resource, Debug, Default, Clone)]
pub struct VisionStats {
    /// Fingerprint of the last full recompute's source set (0 = never).
    pub fingerprint: u64,
    /// Full recomputes since assemble; flat while the fingerprint holds.
    pub recomputes: u64,
}

/// Bundle dispatch (SEAM0d): size VisionGrid from the build grid extent +
/// VisionDef. Never add vision loading logic to bundle/mod.rs.
pub fn apply_vision(world: &mut World, bundle: &crate::bundle::MapBundle) {
    let build = crate::bundle::build_grid(&bundle.world);

    // Fog cell choice (one rule for every map kind): the vision section owns
    // the fog resolution (`VisionDef.cell`, default 1.0); the build grid pins
    // the map EXTENT (origin + cols/rows in build cells), re-tiled at the fog
    // cell so both grids cover the same world area. Why not the build cell
    // directly: FixedSlots build grids carry no cell size (0.0), so the build
    // cell cannot be the universal source, while VisionDef.cell always
    // exists; and with equal cells (the default) the layout is bit-identical
    // to the build grid, honoring "cell layout matches the build grid".
    let cell = bundle.vision.cell;
    let (cols, rows) = if cell > 0.0 && build.cell_size > 0.0 {
        (
            ((build.cols as f32 * build.cell_size) / cell).ceil() as i32,
            ((build.rows as f32 * build.cell_size) / cell).ceil() as i32,
        )
    } else {
        // FixedSlots maps (warden) have no build extent: an empty fog grid.
        // Harmless by design — no sources, no consumers until FOG2.
        (0, 0)
    };

    // Fresh grid per assemble (re-matches never inherit stale fog memory).
    world.insert_resource(VisionGrid {
        cell,
        cols,
        rows,
        origin: build.origin,
        visible: Default::default(),
        explored: Default::default(),
    });
    world.insert_resource(VisionStats::default());
}

/// FNV-1a over the sorted source entries + their count. Hand-rolled (not
/// RandomState/DefaultHasher) so determinism is structural, not documented —
/// the D6 discipline for anything replay-visible.
fn source_fingerprint(entries: &[(i32, i32, u32)]) -> u64 {
    const BASIS: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;
    let mut h = BASIS;
    for &(cx, cy, radius_bits) in entries {
        for v in [cx as i64 as u64, cy as i64 as u64, radius_bits as u64] {
            h ^= v;
            h = h.wrapping_mul(PRIME);
        }
    }
    h ^= entries.len() as u64;
    h.wrapping_mul(PRIME)
}

/// FOG1 grid update (Tick): full recompute of `visible` from every
/// VisionSource (phase 1 treats all carriers as friendly — team filtering is
/// MP1/FOG2 per D5), then `explored |= visible` (explored only grows).
fn update_vision_grid(
    mut grid: ResMut<VisionGrid>,
    mut stats: ResMut<VisionStats>,
    sources: Query<(&VisionSource, &Transform)>,
) {
    // Degenerate geometry (app never assembled a bundle, or a FixedSlots map
    // with no build extent): an empty grid has nothing to recompute.
    if grid.cell <= 0.0 || grid.cols <= 0 || grid.rows <= 0 {
        return;
    }

    // Source-set fingerprint: source count + sorted (cell, radius-bits)
    // entries. Radius rides along so a mid-match radius change (a buff)
    // re-lights the grid, not just movement. While the fingerprint holds the
    // visible set cannot have changed — skip the whole recompute. This is
    // headroom against large maps (thousands of cells x dozens of sources);
    // at the current scale a full recompute every tick would be fine.
    let mut entries: Vec<(i32, i32, u32)> = sources
        .iter()
        .map(|(v, t)| {
            let pos = Vec2::new(t.translation.x, t.translation.z);
            (
                ((pos.x - grid.origin.x) / grid.cell).floor() as i32,
                ((pos.y - grid.origin.y) / grid.cell).floor() as i32,
                v.radius.to_bits(),
            )
        })
        .collect();
    entries.sort_unstable();
    let fingerprint = source_fingerprint(&entries);
    // `recomputes > 0` makes the first tick after assemble structurally
    // always compute once, whatever the zero-value fingerprint hashes to.
    if stats.recomputes > 0 && fingerprint == stats.fingerprint {
        return;
    }
    stats.fingerprint = fingerprint;
    stats.recomputes += 1;

    grid.visible.clear();
    for (vision, transform) in sources.iter() {
        if !(vision.radius > 0.0) {
            continue; // zero / negative / NaN radius lights nothing
        }
        let pos = Vec2::new(transform.translation.x, transform.translation.z);
        let r_cells = vision.radius / grid.cell;
        // Bounding box in cell coords, widened by 1 against floor/ceil edge
        // rounding and clamped to the grid; membership is decided exactly by
        // the center-distance test below, so the box only has to be a
        // superset of the circle.
        let src_x = (pos.x - grid.origin.x) / grid.cell;
        let src_y = (pos.y - grid.origin.y) / grid.cell;
        let min_x = ((src_x - r_cells).floor() as i32 - 1).max(0);
        let max_x = ((src_x + r_cells).ceil() as i32 + 1).min(grid.cols - 1);
        let min_y = ((src_y - r_cells).floor() as i32 - 1).max(0);
        let max_y = ((src_y + r_cells).ceil() as i32 + 1).min(grid.rows - 1);
        for cy in min_y..=max_y {
            for cx in min_x..=max_x {
                let center = grid.origin
                    + Vec2::new(
                        (cx as f32 + 0.5) * grid.cell,
                        (cy as f32 + 0.5) * grid.cell,
                    );
                if pos.distance(center) <= vision.radius {
                    grid.visible.insert((cx, cy));
                }
            }
        }
    }

    // Explored only grows (WC3 memory layer). BTreeSet union is
    // order-independent, so query iteration order cannot leak into state.
    let VisionGrid { visible, explored, .. } = &mut *grid;
    explored.extend(visible.iter().copied());
}

/// Domain systems (registered once by WarbasePlugin; the FOG1 grid update
/// lives here, never in lib.rs).
pub struct VisionPlugin;

impl Plugin for VisionPlugin {
    fn build(&self, app: &mut App) {
        // Resources exist even before assemble so apps that never load a
        // bundle see an empty degenerate grid instead of a panic.
        app.init_resource::<VisionGrid>()
            .init_resource::<VisionStats>();
        app.add_systems(Update, update_vision_grid.in_set(crate::WarbaseSet::Tick));
    }
}

pub mod prelude {
    pub use super::{InFog, VisionDef, VisionGrid, VisionPlugin, VisionSource, VisionStats};
}
