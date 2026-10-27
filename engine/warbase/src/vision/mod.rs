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
//! HIGHLAND (round-2 plan §4 line 1): authored high ground occludes the
//! grid itself — see [`TerrainGrid`] and the D5 note on
//! [`line_of_sight`]. This is PHYSICAL line of sight, not a player-fog
//! rule: it changes which cells [`VisionGrid`] lights, while tower
//! auto-targeting keeps ignoring fog entirely (D5 unchanged — targeting
//! only ever looks at weapon range).
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

/// Map-bundle terrain section (SEAM2d; consumed by the Phase-2 HIGHLAND
/// increment): the cells raised to high ground plus the high-ground attack
/// bonus. Empty section = flat map, zero drift (no occlusion, no bonus).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TerrainDef {
    /// Grid cells on high ground (same lattice as `world.blocked` /
    /// build-grid coordinates).
    #[serde(default)]
    pub high_cells: Vec<[i32; 2]>,
    /// HIGHLAND attack bonus: a basic attack FROM a high cell onto a low
    /// cell deals `damage * (1 + bonus)`. Default 0 = no bonus (zero drift
    /// for every existing map). The number ships per map, like all content.
    #[serde(default)]
    pub attack_bonus: f32,
}

/// Runtime high-ground lookup (HIGHLAND, Phase-2): the authored high-cell
/// set plus the build-lattice geometry needed to map world points onto it.
///
/// DESIGN CHOICE (task left it open): a resource SEPARATE from
/// [`VisionGrid`], not a field inside it. Rationale: (a) terrain is
/// assemble-time static while the vision grid recomputes whenever the
/// source set changes — different lifetimes; (b) combat reads terrain for
/// the attack bonus and must NOT depend on fog state (D5: auto-targeting
/// ignores fog; only the vision system consumes the occlusion); (c) both
/// consumers read it without mutation, so one shared resource is enough.
///
/// Zero drift: an empty `high` set makes every probe answer "low ground"
/// and [`TerrainGrid::attack_multiplier`] return 1.0 — flat maps keep the
/// exact pre-HIGHLAND behavior (the occlusion fast-path in
/// `update_vision_grid` skips the line walk entirely).
#[derive(Resource, Debug, Default, Clone)]
pub struct TerrainGrid {
    /// Authored high cells on the BUILD lattice (`TerrainDef::high_cells`).
    pub high: std::collections::BTreeSet<(i32, i32)>,
    /// High-ground attack bonus (`TerrainDef::attack_bonus`).
    pub attack_bonus: f32,
    /// Build-lattice cell size (0.0 on FixedSlots maps => nothing is high).
    pub cell: f32,
    /// World corner of build cell (0, 0) — same convention as
    /// [`VisionGrid::origin`].
    pub origin: Vec2,
}

impl TerrainGrid {
    /// Is this world point (logic plane: `x` = world x, `y` = world z) on
    /// authored high ground? Empty terrain / degenerate lattice => false.
    pub fn is_high(&self, world: Vec2) -> bool {
        if self.cell <= 0.0 || self.high.is_empty() {
            return false;
        }
        let cx = ((world.x - self.origin.x) / self.cell).floor() as i32;
        let cy = ((world.y - self.origin.y) / self.cell).floor() as i32;
        self.high.contains(&(cx, cy))
    }

    /// HIGHLAND attack multiplier for a basic attack fired at `from` (the
    /// attacker's translation) against a target at `to`: `1 + bonus` when
    /// the attacker stands on high ground and the target does not, 1.0
    /// otherwise (low->high, same height, either flat, or bonus 0). Called
    /// from `combat::fire_attacks` at the damage-computation point.
    pub fn attack_multiplier(&self, from: Vec3, to: Vec3) -> f32 {
        if self.attack_bonus == 0.0 {
            return 1.0; // authored flat combat: identity, zero drift
        }
        let from_high = self.is_high(Vec2::new(from.x, from.z));
        let to_high = self.is_high(Vec2::new(to.x, to.z));
        if from_high && !to_high {
            1.0 + self.attack_bonus
        } else {
            1.0
        }
    }

    /// Is this VISION-grid cell high ground? Maps the cell center back to a
    /// world point, then onto the build lattice — correct for any ratio of
    /// fog cell to build cell (equal cells, the default, are bit-identical).
    fn cell_high(&self, grid: &VisionGrid, cell: (i32, i32)) -> bool {
        let world = grid.origin
            + Vec2::new(
                (cell.0 as f32 + 0.5) * grid.cell,
                (cell.1 as f32 + 0.5) * grid.cell,
            );
        self.is_high(world)
    }
}

/// Bundle dispatch for the terrain section: install the [`TerrainGrid`]
/// from `TerrainDef` + the build-lattice geometry (`bundle::build_grid`
/// derives it; FixedSlots maps carry no cell size, so nothing is high
/// there — their vision grids are degenerate anyway). Empty section =>
/// empty grid resource, zero drift.
pub fn apply_terrain(world: &mut World, bundle: &crate::bundle::MapBundle) {
    let build = crate::bundle::build_grid(&bundle.world);
    world.insert_resource(TerrainGrid {
        high: bundle.terrain.high_cells.iter().map(|&[x, y]| (x, y)).collect(),
        attack_bonus: bundle.terrain.attack_bonus,
        cell: build.cell_size,
        origin: build.origin,
    });
}

/// HIGHLAND line-of-sight probe: can a viewer at cell `from` light cell
/// `to`? Pinned rule (WC3 cliff semantics):
/// - a viewer on HIGH ground is never occluded (high sees high AND low);
/// - a viewer on LOW ground is blocked by the FIRST high cell its ray
///   touches — the destination INCLUDED: "low cannot light high" holds even
///   for an adjacent high cell (the cliff face blocks), and a low->low ray
///   crossing a high ridge is blocked midway.
/// The ray is the integer Bresenham line between the two cell centers
/// (diagonal steps visit one cell per step — the pinned approximation).
///
/// D5 BOUNDARY (pinned here and in the tests): this occlusion is PHYSICS,
/// not player fog — it changes what [`VisionGrid`] lights (a low tower
/// genuinely cannot SEE a creep on the cliff above it). Tower
/// AUTO-TARGETING still ignores it completely: `combat::pick_targets`
/// never reads the grid, only weapon range — a low tower shoots a
/// high-ground creep standing in a dark cell exactly as before.
fn line_of_sight(
    terrain: &TerrainGrid,
    grid: &VisionGrid,
    from: (i32, i32),
    to: (i32, i32),
) -> bool {
    if from == to {
        return true; // a viewer always lights its own cell
    }
    if terrain.cell_high(grid, from) {
        return true; // high ground overlooks everything below and beside it
    }
    let (mut x, mut y) = from;
    let (dx, dy) = ((to.0 - x).abs(), (to.1 - y).abs());
    let sx = if to.0 > x { 1 } else { -1 };
    let sy = if to.1 > y { 1 } else { -1 };
    let mut err = dx - dy;
    loop {
        // One Bresenham step; the source cell itself is never re-tested.
        let e2 = 2 * err;
        if e2 > -dy {
            err -= dy;
            x += sx;
        }
        if e2 < dx {
            err += dx;
            y += sy;
        }
        if terrain.cell_high(grid, (x, y)) {
            return false; // first high cell on the ray blocks a low viewer
        }
        if x == to.0 && y == to.1 {
            return true;
        }
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
    // HIGHLAND: high ground occludes the grid itself (physical LOS — see
    // `line_of_sight`; D5 auto-targeting still never consults it).
    terrain: Res<TerrainGrid>,
    // MAP (round-2 plan §3): night vision factor — daynight-owned resource,
    // 1.0 unless a Night cycle is authored (zero drift for default maps).
    night_vision: Option<Res<crate::daynight::NightVisionFactor>>,
    sources: Query<(&VisionSource, &Transform, Option<&Owner>, Option<&Allegiance>)>,
) {
    // Degenerate geometry (app never assembled a bundle, or a FixedSlots map
    // with no build extent): an empty grid has nothing to recompute.
    if grid.cell <= 0.0 || grid.cols <= 0 || grid.rows <= 0 {
        return;
    }
    let night_f = night_vision.map(|f| f.0).unwrap_or(1.0);

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
                (v.radius * night_f).to_bits(),
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
        let radius = vision.radius * night_f; // MAP: night vision factor
        let r_cells = radius / grid.cell;
        // Bounding box in cell coords, widened by 1 against floor/ceil edge
        // rounding and clamped to the grid; membership is decided exactly by
        // the center-distance test below, so the box only has to be a
        // superset of the circle.
        let src_x = (pos.x - grid.origin.x) / grid.cell;
        let src_y = (pos.y - grid.origin.y) / grid.cell;
        // HIGHLAND: the occlusion probe walks from this source CELL; the
        // empty-terrain fast path below keeps flat maps bit-identical.
        let src_cell = (src_x.floor() as i32, src_y.floor() as i32);
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
                // HIGHLAND: in-radius is not enough on authored high ground —
                // a low viewer's ray dies on the first high cell it touches
                // (destination included); a high viewer is never occluded.
                // Empty terrain skips the walk entirely (zero drift).
                if pos.distance(center) <= radius
                    && (terrain.high.is_empty() || line_of_sight(&terrain, &grid, src_cell, (cx, cy)))
                {
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
            .init_resource::<MinimapModel>()
            // HIGHLAND: registered by the plugin (not assemble) so combat's
            // Res<TerrainGrid> is always satisfied; assemble overwrites.
            .init_resource::<TerrainGrid>();
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
        point_visible, apply_terrain, InFog, MinimapBlip, MinimapModel, TerrainDef, TerrainGrid,
        VisionDef, VisionGrid, VisionPerspective, VisionPlugin, VisionSource, VisionStats,
    };
}
