//! Vision & fog of war (WC3 alignment, ADR-0016): two-layer grid (visible /
//! explored) fed by VisionSource carriers. Rule D5: player fog constrains
//! the interaction layer only (manual cast points, selection, rendering) —
//! towers and autocast are unaffected (they carry their own VisionSource).
//!
//! FOG1: circular unobstructed vision; the grid update runs in Tick.
//! FOG2: shared vision — only sources on the observer team
//! ([`VisionPerspective`]) light the grid (team mapping pinned to MP1's
//! Allegiance rule, see [`source_team`]); `InFog` marking of units runs in
//! Cleanup; manual Point casts are gated by [`point_visible`] (hook lives in
//! skill::process_casts).
//! FOG3: [`MinimapModel`] — the data layer a grey-box minimap reads
//! (blips = visible units, deterministic order; rendering is the ui
//! domain's business, the base ships data only).
//!
//! Zero drift: maps without fog in play (no grid geometry — warden's
//! FixedSlots; or no lit cells — loop-td's grid before any source) get no
//! InFog marks, empty blips, and pass-through visibility probes
//! ([`fog_active`]).

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::unit::{Allegiance, MatchDiplomacy, Owner, TeamId, UnitInstance, XpHolder};

/// Grants vision of `radius` world units around the carrier (towers, free
/// agents, buildings). Creeps may carry it for team-shared vision (MP1+).
#[derive(Component, Debug)]
pub struct VisionSource {
    pub radius: f32,
}

/// Present on units currently outside the observer team's vision (FOG2).
/// Consumption is downstream and interaction-layer only, per D5: the fx/ui
/// layers dim or hide carriers (art comes later; the base ships no
/// visuals), manual interactions reject, and combat/autocast NEVER read it
/// (towers carry their own VisionSource and keep firing into fog).
#[derive(Component, Debug)]
pub struct InFog;

/// Which team's vision the single [`VisionGrid`] computes — the observer
/// seat (FOG2 shared vision: all sources on this team light the grid
/// together, unowned Defender-side scenery included via [`source_team`]).
/// Default = the home team `TeamId(0)`, matching the single-player default
/// diplomacy, so warden's owner-less towers keep FOG1 behavior exactly.
///
/// One grid = one observer. Multi-view rendering (a grid per seat, e.g.
/// splitscreen or replays) is a game-side concern — swap or instantiate
/// this resource per view and tick the grid per seat; recorded as the
/// known simplification for the RL RTS acceptance maps.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub struct VisionPerspective(pub TeamId);

impl Default for VisionPerspective {
    fn default() -> Self {
        Self(TeamId(0))
    }
}

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

/// Map-bundle terrain section (SEAM2d, Phase-2 highland placeholder): the
/// cells raised to high ground. Vision/attack-bonus consumption lands with
/// the Phase-2 highland increment (VisionGrid extension); the section is
/// pinned now so map authors and the bundle schema are stable.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TerrainDef {
    /// Grid cells on high ground (same lattice as `world.blocked` /
    /// build-grid coordinates).
    #[serde(default)]
    pub high_cells: Vec<[i32; 2]>,
}

/// Bundle dispatch for the terrain section (SEAM2d placeholder): the
/// Phase-2 highland increment owns the consumption; empty = flat map.
pub fn apply_terrain(world: &mut World, bundle: &crate::bundle::MapBundle) {
    let _ = (world, &bundle.terrain.high_cells);
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

/// One minimap blip (FOG3 data layer): a unit currently visible to the
/// observer, projected on the logic plane (`Vec2.x` = world x, `Vec2.y` =
/// world z). `side` drives the tint, `is_hero` the bigger dot — both are
/// ui-domain concerns; the base ships semantics only.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MinimapBlip {
    pub pos: Vec2,
    pub side: Allegiance,
    pub is_hero: bool,
}

/// Minimap data model (FOG3): everything a grey-box minimap needs, nothing
/// it doesn't. Refreshed in Cleanup (after InFog marking); blips are sorted
/// by entity index, so the order is deterministic (D6). Rendering, event
/// pings, and click-to-camera live in the game-side ui domain.
#[derive(Resource, Debug, Default, Clone)]
pub struct MinimapModel {
    /// Visible units only (fog-filtered), ascending entity-index order.
    pub blips: Vec<MinimapBlip>,
    /// World rect the vision grid covers (the extent a minimap draws).
    /// `Rect::EMPTY` when the map has no vision geometry.
    pub map_bounds: Rect,
}

/// Fog is "in play" only when the grid has geometry AND something is lit.
/// FixedSlots maps (warden) never grow a grid; maps with a build extent but
/// zero lit cells (loop-td today: no VisionSource carriers) light nothing.
/// Both must stay zero-drift: no InFog, empty blips, pass-through
/// visibility probes. Pinned simplification: a real match whose sources all
/// die/sell out also reads as "no fog" — revisit when a map actually plays
/// fog that way (RL maps).
fn fog_active(grid: &VisionGrid) -> bool {
    grid.cell > 0.0 && grid.cols > 0 && grid.rows > 0 && !grid.visible.is_empty()
}

/// D5 visibility probe for the interaction layer (FOG2): is this world
/// point (logic plane: `Vec2.x` = world x, `Vec2.y` = world z) inside the
/// observer team's current visible set? Outside the grid or with fog
/// inactive the answer is true — fog constrains only when it exists (the
/// zero-drift contract for maps that never opted into vision).
///
/// Consumer today: skill::process_casts' Point-cast gate (manual casts must
/// land in vision). Selection ("invisible enemies unselectable") joins here
/// when its increment lands.
pub fn point_visible(grid: &VisionGrid, world_pos: Vec2) -> bool {
    if !fog_active(grid) {
        return true;
    }
    let cx = ((world_pos.x - grid.origin.x) / grid.cell).floor() as i32;
    let cy = ((world_pos.y - grid.origin.y) / grid.cell).floor() as i32;
    grid.visible.contains(&(cx, cy))
}

/// Team of a vision source for perspective filtering (FOG2 shared vision).
/// Mapping is pinned to MP1's Allegiance<->team rule (unit/mod.rs,
/// `Hostility`):
/// - owned: the owner's team; owners missing from the match table fold to
///   the home team (team 0), exactly like `MatchDiplomacy::allegiance_of`;
///   `Owner` is the source of truth and wins over any stale Allegiance.
/// - unowned Defender: team 0 (warden's owner-less towers live here).
/// - unowned Attacker: the environment team (smallest team id hostile to
///   team 0) — creeps' own vision serves THEIR seat, not the default one.
/// - neither marker (bare scenery sources, e.g. map dressing or test
///   rigs): team 0 — preserves FOG1's "every source is friendly" behavior
///   for carriers that predate the MP1 markers (zero drift).
fn source_team(
    diplomacy: &MatchDiplomacy,
    owner: Option<&Owner>,
    side: Option<&Allegiance>,
) -> TeamId {
    match (owner, side) {
        (Some(o), _) => diplomacy.team_of(o.0).unwrap_or(TeamId(0)),
        (None, Some(Allegiance::Attacker)) => diplomacy.environment_team(),
        (None, _) => TeamId(0),
    }
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

/// FNV-1a over the sorted source entries + their count + the observer team.
/// Hand-rolled (not RandomState/DefaultHasher) so determinism is structural,
/// not documented — the D6 discipline for anything replay-visible. The
/// perspective rides along so switching the observer seat always relights
/// the grid even with an unchanged source set.
fn source_fingerprint(entries: &[(i32, i32, u32)], perspective: TeamId) -> u64 {
    const BASIS: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;
    let mut h = BASIS;
    for v in [perspective.0 as u64, entries.len() as u64] {
        h ^= v;
        h = h.wrapping_mul(PRIME);
    }
    for &(cx, cy, radius_bits) in entries {
        for v in [cx as i64 as u64, cy as i64 as u64, radius_bits as u64] {
            h ^= v;
            h = h.wrapping_mul(PRIME);
        }
    }
    h.wrapping_mul(PRIME)
}

/// FOG1/FOG2 grid update (Tick): full recompute of `visible` from every
/// VisionSource on the observer team ([`VisionPerspective`] — shared vision
/// by construction: same-team sources union, everything else is filtered by
/// [`source_team`]), then `explored |= visible` (explored only grows).
fn update_vision_grid(
    mut grid: ResMut<VisionGrid>,
    mut stats: ResMut<VisionStats>,
    perspective: Res<VisionPerspective>,
    diplomacy: Res<MatchDiplomacy>,
    sources: Query<(&VisionSource, &Transform, Option<&Owner>, Option<&Allegiance>)>,
) {
    // Degenerate geometry (app never assembled a bundle, or a FixedSlots map
    // with no build extent): an empty grid has nothing to recompute.
    if grid.cell <= 0.0 || grid.cols <= 0 || grid.rows <= 0 {
        return;
    }

    // Source-set fingerprint over PERSPECTIVE sources only: count + sorted
    // (cell, radius-bits) entries. Radius rides along so a mid-match radius
    // change (a buff) re-lights the grid, not just movement. Off-seat
    // sources are invisible here, so their movement correctly cannot
    // trigger a recompute. While the fingerprint holds the visible set
    // cannot have changed — skip the whole recompute. This is headroom
    // against large maps (thousands of cells x dozens of sources); at the
    // current scale a full recompute every tick would be fine.
    let mut entries: Vec<(i32, i32, u32)> = sources
        .iter()
        .filter(|(_, _, owner, side)| source_team(&diplomacy, *owner, *side) == perspective.0)
        .map(|(v, t, _, _)| {
            let pos = Vec2::new(t.translation.x, t.translation.z);
            (
                ((pos.x - grid.origin.x) / grid.cell).floor() as i32,
                ((pos.y - grid.origin.y) / grid.cell).floor() as i32,
                v.radius.to_bits(),
            )
        })
        .collect();
    entries.sort_unstable();
    let fingerprint = source_fingerprint(&entries, perspective.0);
    // `recomputes > 0` makes the first tick after assemble structurally
    // always compute once, whatever the zero-value fingerprint hashes to.
    if stats.recomputes > 0 && fingerprint == stats.fingerprint {
        return;
    }
    stats.fingerprint = fingerprint;
    stats.recomputes += 1;

    grid.visible.clear();
    for (vision, transform, owner, side) in &sources {
        // Shared vision: only the observer team's sources light this grid.
        if source_team(&diplomacy, owner, side) != perspective.0 {
            continue;
        }
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

/// Cleanup: mark units outside the observer's vision with `InFog` (FOG2).
/// Plan-pinned simplification: EVERY `UnitInstance` carrier is a mark
/// candidate (buildings included — no Transform/VisionSource
/// discrimination); grid membership decides, nothing else. A
/// perspective-team tower keeps its own cell lit by its own VisionSource
/// (any radius >= ~0.71 cells does), so real towers never fog over.
/// Rendering consumes the mark downstream (fx/ui layer, D9 grey-box; art
/// later — the base ships no visuals). Fog inactive => the mark set is
/// emptied (zero drift: warden/loop-td never see a mark).
fn mark_in_fog(
    mut commands: Commands,
    grid: Res<VisionGrid>,
    units: Query<(Entity, &Transform, Has<InFog>), With<UnitInstance>>,
) {
    if !fog_active(&grid) {
        for (entity, _, fog) in &units {
            if fog {
                commands.entity(entity).remove::<InFog>();
            }
        }
        return;
    }
    for (entity, transform, fog) in &units {
        let pos = Vec2::new(transform.translation.x, transform.translation.z);
        if point_visible(&grid, pos) {
            if fog {
                commands.entity(entity).remove::<InFog>();
            }
        } else if !fog {
            commands.entity(entity).insert(InFog);
        }
    }
}

/// Cleanup (after `mark_in_fog`): rebuild [`MinimapModel`] (FOG3) — every
/// unit standing in a visible cell projects to one blip, ordered by Entity
/// (D6 determinism: query order never reaches the model). Invisible units
/// leave no blip; heroes flag thicker dots via `XpHolder`. Bounds come from
/// the grid geometry whenever it exists, blips need fog in play.
fn refresh_minimap(
    mut model: ResMut<MinimapModel>,
    grid: Res<VisionGrid>,
    units: Query<(Entity, &Transform, &Allegiance, Has<XpHolder>), With<UnitInstance>>,
) {
    model.blips.clear();
    if grid.cell <= 0.0 || grid.cols <= 0 || grid.rows <= 0 {
        model.map_bounds = Rect::EMPTY;
        return;
    }
    model.map_bounds = Rect {
        min: grid.origin,
        max: grid.origin + Vec2::new(grid.cols as f32 * grid.cell, grid.rows as f32 * grid.cell),
    };
    if !fog_active(&grid) {
        return; // fog not in play: no blips (zero drift for loop-td)
    }
    let mut rows: Vec<(Entity, MinimapBlip)> = Vec::new();
    for (entity, transform, side, hero) in &units {
        let pos = Vec2::new(transform.translation.x, transform.translation.z);
        if point_visible(&grid, pos) {
            rows.push((
                entity,
                MinimapBlip { pos, side: *side, is_hero: hero },
            ));
        }
    }
    // D6 determinism: sort by the REAL entity index (`index_u32`). Bevy
    // 0.19's `Entity: Ord` compares raw bits, and the `nonmax` storage
    // bit-inverts the index, so ordering by `Entity` directly yields
    // DESCENDING spawn order within a generation; `index_u32` is the
    // stable, spawn-ascending key the task pins.
    rows.sort_by_key(|(entity, _)| entity.index_u32());
    model.blips = rows.into_iter().map(|(_, blip)| blip).collect();
}

/// Domain systems (registered once by WarbasePlugin; the grid update and
/// its FOG2/FOG3 consumers live here, never in lib.rs).
pub struct VisionPlugin;

impl Plugin for VisionPlugin {
    fn build(&self, app: &mut App) {
        // Resources exist even before assemble so apps that never load a
        // bundle see an empty degenerate grid instead of a panic.
        app.init_resource::<VisionGrid>()
            .init_resource::<VisionStats>()
            .init_resource::<VisionPerspective>()
            .init_resource::<MinimapModel>();
        app.add_systems(Update, update_vision_grid.in_set(crate::WarbaseSet::Tick));
        // FOG2/FOG3 consumers run late Cleanup, after combat::cleanup_dead
        // (its deferred despawns land first — no InFog writes onto dying
        // entities), chained so marking settles before the minimap reads.
        app.add_systems(
            Update,
            (mark_in_fog, refresh_minimap)
                .chain()
                .after(crate::combat::cleanup_dead)
                .in_set(crate::WarbaseSet::Cleanup),
        );
    }
}

pub mod prelude {
    pub use super::{
        point_visible, apply_terrain, InFog, MinimapBlip, MinimapModel, TerrainDef, VisionDef,
        VisionGrid, VisionPerspective, VisionPlugin, VisionSource, VisionStats,
    };
}
