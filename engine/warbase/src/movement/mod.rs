//! Movement (WC3 alignment, ADR-0016): free-roaming agents for commanded
//! units — the counterpart of `path` (track followers for wave creeps).
//!
//! Landed here (MOV1/MOV2):
//! - `move_agents` (Director): constant-speed locomotion toward the current
//!   waypoint with arrival braking; the free-agent twin of
//!   `path::advance_followers`.
//! - `astar` + `smooth_path` (pure): 8-way grid A* with pinned determinism
//!   (fixed direction order, total-order open-set key, grid-indexed arrays,
//!   no HashMap anywhere) and a best-effort unreachable semantic.
//! - `process_path_requests` (Tick): frame-budgeted path queue (WC3-style
//!   throttling), writing world-space waypoints into `MoveAgent`.
//! - `dirty_recompute` (Director): rebuilds the nav grid when the BuildGrid
//!   changes (change-detection observation — the build domain itself stays
//!   untouched) and re-enqueues affected agents through the same budgeted
//!   queue.
//!
//! Later increments (plan §5): MOV3 separation, MOV4 `tick_orders` (order
//! consumption in Tick), MOV5 formation scatter. `MoveAgent` and
//! `PathFollower` are mutually exclusive on an entity (debug_assert in
//! `move_agents`) — track creeps and free agents are two worlds.
//!
//! Determinism (plan D6, replay's foundation): the A* open set is a std
//! `BinaryHeap` over a hand-written total order (f asc, h asc, insertion
//! seq asc); successors expand in the pinned direction order below;
//! g/closed/came-from state lives in grid-indexed `Vec`s; the request queue
//! drains in (enqueue frame, entity index) order. No HashMap iteration
//! drives logic.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::cmp::{Ordering, Reverse};
use std::collections::{BTreeSet, BinaryHeap};

use crate::build::{BuildGrid, BuildMode, Cell};
use crate::buff::EffectiveStats;
use crate::path::PathFollower;

/// A commanded unit's current order (WC3 command set; `Smart` is resolved
/// into a concrete variant by selection::smart_resolve before it reaches
/// movement). Shared vocabulary between SEL4 and MOV — pinned here so both
/// domains build against it independently.
#[derive(Debug, Clone, PartialEq)]
pub enum Order {
    /// Walk to a point; ignore everything.
    Move { to: Vec3 },
    /// Walk to a point, engaging any hostile in weapon range on the way
    /// (A-key). Resume the original route after the target dies.
    AttackMove { to: Vec3 },
    /// Chase one entity into range and fire (right-click on a hostile).
    AttackTarget { target: Entity },
    /// Stand ground: acquire targets in range but never chase (H-key).
    HoldPosition,
    /// Clear the order and stop in place (S-key).
    Stop,
    /// Patrol between two points with attack-move semantics (P-key).
    Patrol { a: Vec3, b: Vec3, toward_b: bool },
    /// Walk onto a ground item and pick it up (right-click on an item).
    Pickup { item: Entity },
    /// Keep a friendly entity's flank (right-click on a friendly).
    Follow { target: Entity },
}

/// Player-facing command broadcast: one order for a set of selected units
/// (formation offsets are assigned by MOV5 at consumption). Consumed in Tick
/// by MOV4 `tick_orders` (not shipped yet).
#[derive(Message, Debug)]
pub struct UnitCommand {
    pub order: Order,
    pub subjects: Vec<Entity>,
}

/// Free-roaming locomotion component (NOT for wave creeps on tracks).
/// `speed` is the unit's base speed; `move_agents` multiplies it by
/// `EffectiveStats.move_mult` each frame (no StatMods on the entity => bare
/// speed, mult 1.0). `waypoints` is the pending route in world space,
/// consumed front-first — written by MOV2 `process_path_requests`, or seeded
/// directly for straight-line orders. `velocity` is derived state written by
/// `move_agents` (zero when idle) for readers (facing, separation, sfx).
/// Spawn together with `AgentOrderState`.
#[derive(Component, Debug)]
pub struct MoveAgent {
    pub speed: f32,
    pub arrival_radius: f32,
    pub waypoints: Vec<Vec3>,
    pub velocity: Vec3,
}

/// Execution sub-state of the current order. MOV1 maintains Idle/Moving;
/// Engaging/Chasing belong to MOV4 (engaged units hold position in
/// `move_agents` — the seam MOV4 will drive).
#[derive(Component, Debug, Default, PartialEq)]
pub enum AgentOrderState {
    #[default]
    Idle,
    Moving,
    /// Attack-move pause: engaged with a target, route resumes after.
    Engaging,
    Chasing,
}

/// Navigation grid: same cell size as the build grid; blockers = authored
/// scenery (bundle `nav`/`world.blocked`) + build-grid scenery + placed
/// buildables. `blocked` is the live union, rebuilt by `dirty_recompute`;
/// `authored` is the bundle-only base that survives rebuilds. A grid without
/// geometry (FixedSlots maps, default resource) means "nothing blocks" —
/// path requests fall back to a straight line.
#[derive(Resource, Debug, Clone)]
pub struct NavGrid {
    pub cell: f32,
    pub cols: i32,
    pub rows: i32,
    /// World corner of cell (0, 0); cell center = origin + (c + 0.5) * cell.
    pub origin: Vec2,
    /// Live blocked set in grid coordinates (deterministic: BTreeSet).
    pub blocked: BTreeSet<(i32, i32)>,
    /// Bundle-authored blockers only (rebuild base; occupancy-free).
    pub authored: BTreeSet<(i32, i32)>,
    /// A* calls allowed per frame (WC3-style throttling; plan default 8).
    pub budget_per_frame: usize,
}

impl Default for NavGrid {
    fn default() -> Self {
        Self {
            cell: 0.0,
            cols: 0,
            rows: 0,
            origin: Vec2::ZERO,
            blocked: BTreeSet::new(),
            authored: BTreeSet::new(),
            budget_per_frame: 8,
        }
    }
}

impl NavGrid {
    /// A grid with geometry to path on (False for default/FixedSlots maps).
    pub fn has_grid(&self) -> bool {
        self.cell > 0.0 && self.cols > 0 && self.rows > 0
    }

    pub fn in_bounds(&self, c: (i32, i32)) -> bool {
        c.0 >= 0 && c.1 >= 0 && c.0 < self.cols && c.1 < self.rows
    }

    /// World position -> grid cell (only meaningful when `has_grid`).
    pub fn world_to_cell(&self, world: Vec2) -> (i32, i32) {
        let local = world - self.origin;
        (
            (local.x / self.cell).floor() as i32,
            (local.y / self.cell).floor() as i32,
        )
    }

    /// Grid cell -> world center = origin + (c + 0.5) * cell.
    pub fn cell_center(&self, c: (i32, i32)) -> Vec2 {
        self.origin
            + Vec2::new(
                (c.0 as f32 + 0.5) * self.cell,
                (c.1 as f32 + 0.5) * self.cell,
            )
    }
}

/// Map-bundle nav section (SEAM0d; optional, old maps default empty).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NavDef {
    #[serde(default)]
    pub blocked: Vec<[i32; 2]>,
    /// A* calls allowed per frame (MOV2 budget; WC3-style throttling).
    #[serde(default = "default_budget")]
    pub budget_per_frame: usize,
}

fn default_budget() -> usize {
    8
}

// --------------------------------------------------------------------------
// MOV2: A* (pure functions)
// --------------------------------------------------------------------------

/// PINNED neighbor order — the expansion order of equal-cost successors and
/// the push order into the open set. N, E, S, W, NE, SE, SW, NW (+y = north
/// on the grid plane). Never reorder: replay determinism (plan D6).
const DIRS: [(i32, i32); 8] = [
    (0, 1),   // N
    (1, 0),   // E
    (0, -1),  // S
    (-1, 0),  // W
    (1, 1),   // NE
    (1, -1),  // SE
    (-1, -1), // SW
    (-1, 1),  // NW
];

/// Open-set key. Total order (no f32 partiality): f asc, then h asc — the
/// pinned tie-break "equal f prefers the smaller h" — then insertion seq asc
/// (FIFO among full ties, so the pinned direction order above decides).
/// Wrapped in `Reverse` for the max-heap `BinaryHeap` => pops the best key.
#[derive(PartialEq)]
struct OpenKey {
    f: f32,
    h: f32,
    seq: u64,
    cell: (i32, i32),
}
impl Eq for OpenKey {}
impl Ord for OpenKey {
    fn cmp(&self, other: &Self) -> Ordering {
        self.f
            .total_cmp(&other.f)
            .then(self.h.total_cmp(&other.h))
            .then(self.seq.cmp(&other.seq))
    }
}
impl PartialOrd for OpenKey {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Octile distance (admissible + consistent for 8-way unit costs).
fn octile(a: (i32, i32), b: (i32, i32)) -> f32 {
    let dx = (a.0 - b.0).abs() as f32;
    let dy = (a.1 - b.1).abs() as f32;
    let (mx, mn) = if dx > dy { (dx, dy) } else { (dy, dx) };
    mx + (std::f32::consts::SQRT_2 - 1.0) * mn
}

/// 8-way grid A*, raw grid path (start and goal cells included). Pure and
/// deterministic (see module header). Rules, pinned:
/// - Cardinal steps cost 1.0, diagonals SQRT_2.
/// - NO CORNER CUTTING: a diagonal step requires BOTH orthogonal neighbors
///   passable — walls never leak diagonally.
/// - The start cell may itself be blocked (a tower was just placed under
///   the unit); the search escapes it normally.
/// - Unreachable / off-grid goal: returns the path to the explored cell
///   with the smallest h (best-effort, WC3-style "walk as close as
///   possible"); on h ties the first explored cell wins (pop order, itself
///   deterministic). `None` only when the nav grid has no geometry or the
///   start is out of bounds.
pub fn astar(nav: &NavGrid, from: (i32, i32), to: (i32, i32)) -> Option<Vec<(i32, i32)>> {
    if !nav.has_grid() || !nav.in_bounds(from) {
        return None;
    }
    let cols = nav.cols;
    let n = (cols * nav.rows) as usize;
    let flat = |c: (i32, i32)| (c.1 * cols + c.0) as usize;
    let mut g = vec![f32::INFINITY; n];
    let mut came_from = vec![-1i64; n];
    let mut closed = vec![false; n];
    let mut heap: BinaryHeap<Reverse<OpenKey>> = BinaryHeap::new();
    let mut seq: u64 = 0;

    let h0 = octile(from, to);
    g[flat(from)] = 0.0;
    heap.push(Reverse(OpenKey { f: h0, h: h0, seq, cell: from }));
    seq += 1;

    // Best-effort target: lowest-h explored cell (strict < keeps the first
    // explored cell on ties — deterministic).
    let mut best = from;
    let mut best_h = h0;

    while let Some(Reverse(key)) = heap.pop() {
        let cur = key.cell;
        let ci = flat(cur);
        if closed[ci] {
            continue; // stale heap entry
        }
        closed[ci] = true;
        let h = octile(cur, to);
        if h < best_h {
            best_h = h;
            best = cur;
        }
        if cur == to {
            return Some(reconstruct(&came_from, cols, cur));
        }
        let gc = g[ci];
        for &(dx, dy) in DIRS.iter() {
            let nxt = (cur.0 + dx, cur.1 + dy);
            if !nav.in_bounds(nxt) || nav.blocked.contains(&nxt) {
                continue;
            }
            let step = if dx != 0 && dy != 0 {
                // Corner-cut guard: both orthogonal cells must be passable.
                let a = (cur.0 + dx, cur.1);
                let b = (cur.0, cur.1 + dy);
                if nav.blocked.contains(&a) || nav.blocked.contains(&b) {
                    continue;
                }
                std::f32::consts::SQRT_2
            } else {
                1.0
            };
            let ng = gc + step;
            let ni = flat(nxt);
            if ng < g[ni] {
                g[ni] = ng;
                came_from[ni] = ci as i64;
                let nh = octile(nxt, to);
                heap.push(Reverse(OpenKey { f: ng + nh, h: nh, seq, cell: nxt }));
                seq += 1;
            }
        }
    }
    // Exhausted without reaching the goal: best-effort partial path.
    Some(reconstruct(&came_from, cols, best))
}

fn reconstruct(came_from: &[i64], cols: i32, mut c: (i32, i32)) -> Vec<(i32, i32)> {
    let mut path = vec![c];
    loop {
        let f = came_from[(c.1 * cols + c.0) as usize];
        if f < 0 {
            break;
        }
        c = ((f % cols as i64) as i32, (f / cols as i64) as i32);
        path.push(c);
    }
    path.reverse();
    path
}

/// Line-of-sight string pulling: greedily keep the furthest cell whose
/// center is visible from the last kept one. Visibility = sampling the
/// center-to-center segment at half-cell steps, strictly between the
/// endpoints (endpoints may legally sit inside blocked cells — e.g. the
/// start when a tower just landed there). Conservative by construction:
/// corner grazes stay un-pulled, walls are never cut.
pub fn smooth_path(nav: &NavGrid, path: &[(i32, i32)]) -> Vec<(i32, i32)> {
    let n = path.len();
    if n <= 2 {
        return path.to_vec();
    }
    let mut out = vec![path[0]];
    let mut k = 0;
    for j in 2..n {
        if !los(nav, path[k], path[j]) {
            out.push(path[j - 1]);
            k = j - 1;
        }
    }
    out.push(path[n - 1]);
    out
}

fn los(nav: &NavGrid, a: (i32, i32), b: (i32, i32)) -> bool {
    if a == b {
        return true;
    }
    let pa = nav.cell_center(a);
    let pb = nav.cell_center(b);
    los_world(nav, Vec3::new(pa.x, 0.0, pa.y), Vec3::new(pb.x, 0.0, pb.y))
}

/// World-space segment visibility (see `smooth_path`). No grid geometry =>
/// nothing blocks.
fn los_world(nav: &NavGrid, a: Vec3, b: Vec3) -> bool {
    if !nav.has_grid() {
        return true;
    }
    let delta = b - a;
    let dist = delta.length();
    if dist <= 0.0 {
        return true;
    }
    let steps = ((dist / (nav.cell * 0.5)).ceil() as usize).max(1);
    for i in 1..steps {
        let p = a + delta * (i as f32 / steps as f32);
        let c = nav.world_to_cell(Vec2::new(p.x, p.z));
        if !nav.in_bounds(c) || nav.blocked.contains(&c) {
            return false;
        }
    }
    true
}

// --------------------------------------------------------------------------
// MOV2: request queue + budgeted processing
// --------------------------------------------------------------------------

/// One pending path request.
#[derive(Debug, Clone, Copy)]
pub struct PathRequest {
    pub entity: Entity,
    /// World-space target point.
    pub target: Vec3,
    /// Frame counter value at enqueue time (deterministic drain order).
    pub frame: u64,
}

/// Pending A* requests, drained `NavGrid::budget_per_frame` at a time by
/// `process_path_requests` (Tick). Deterministic drain order: sorted by
/// (enqueue frame, entity index); re-requesting for an entity replaces its
/// older entry (latest target wins).
#[derive(Resource, Debug, Default)]
pub struct PathRequestQueue(pub Vec<PathRequest>);

impl PathRequestQueue {
    pub fn push(&mut self, entity: Entity, target: Vec3, frame: u64) {
        self.0.retain(|r| r.entity != entity);
        self.0.push(PathRequest { entity, target, frame });
    }
}

/// Monotonic frame counter for enqueue ordering (incremented once per
/// `process_path_requests` run).
#[derive(Resource, Debug, Default)]
pub struct NavFrame(pub u64);

/// A* call statistics (throttle proof + test observability). `calls` counts
/// the current frame's calls (reset at the start of each run); `total`
/// never resets.
#[derive(Resource, Debug, Default)]
pub struct AstarCount {
    pub calls: usize,
    pub total: usize,
}

/// Manual dirty flag: force a nav rebuild on the next `dirty_recompute` run
/// even if the BuildGrid did not change (normally unnecessary — change
/// detection already catches try_place/try_sell; for out-of-band blockers).
#[derive(Resource, Debug, Default)]
pub struct NavForceDirty(pub bool);

/// Enqueue a path request for a MoveAgent (the MOV2 entry point for games;
/// MOV4 `tick_orders` will land UnitCommand::Move here, tests call it
/// directly).
pub fn request_path(world: &mut World, entity: Entity, target: Vec3) {
    let frame = world.get_resource_or_insert_with(NavFrame::default).0;
    world
        .get_resource_or_insert_with(PathRequestQueue::default)
        .push(entity, target, frame);
}

/// Force a nav rebuild + affected re-path check on the next frame (see
/// `NavForceDirty`).
pub fn mark_nav_dirty(world: &mut World) {
    world.get_resource_or_insert_with(NavForceDirty::default).0 = true;
}

/// Tick: drain up to `NavGrid::budget_per_frame` requests per frame, run
/// `astar` (pure), and write world-space waypoints into
/// `MoveAgent::waypoints` — cell centers via `smooth_path`, with the final
/// waypoint replaced by the exact requested point when its cell is the
/// path's goal. No grid geometry (FixedSlots maps / default NavGrid) =>
/// straight-line fallback waypoint, no A* call, no budget use.
pub fn process_path_requests(
    mut queue: ResMut<PathRequestQueue>,
    mut frame: ResMut<NavFrame>,
    nav: Res<NavGrid>,
    mut count: ResMut<AstarCount>,
    mut agents: Query<(&mut MoveAgent, &mut AgentOrderState, &Transform)>,
) {
    count.calls = 0;
    frame.0 += 1;
    if queue.0.is_empty() {
        return;
    }
    // Deterministic drain order: (enqueue frame, entity index).
    queue
        .0
        .sort_by(|a, b| (a.frame, a.entity.index_u32()).cmp(&(b.frame, b.entity.index_u32())));
    let take = queue.0.len().min(nav.budget_per_frame.max(1));
    let batch: Vec<PathRequest> = queue.0.drain(..take).collect();
    for req in batch {
        let Ok((mut agent, mut state, tf)) = agents.get_mut(req.entity) else {
            continue; // despawned or lost its MoveAgent since enqueue
        };
        let target_cell = if nav.has_grid() {
            Some(nav.world_to_cell(Vec2::new(req.target.x, req.target.z)))
        } else {
            None
        };
        let route = target_cell.and_then(|to| {
            let from = nav.world_to_cell(Vec2::new(tf.translation.x, tf.translation.z));
            count.calls += 1;
            count.total += 1;
            astar(&nav, from, to)
        });
        match route {
            // Same cell (or degenerate path): walk to the exact point.
            Some(path) if path.len() <= 1 => agent.waypoints = vec![req.target],
            Some(path) => {
                let pulled = smooth_path(&nav, &path);
                let mut waypoints: Vec<Vec3> = pulled[1..]
                    .iter()
                    .map(|&c| {
                        let p = nav.cell_center(c);
                        Vec3::new(p.x, 0.0, p.y)
                    })
                    .collect();
                // Exact click point when the goal cell matches (unreachable
                // best-effort endings keep their cell center).
                if target_cell == Some(*pulled.last().unwrap()) {
                    if let Some(last) = waypoints.last_mut() {
                        *last = req.target;
                    }
                }
                agent.waypoints = waypoints;
            }
            // No grid geometry: straight-line best-effort.
            None => agent.waypoints = vec![req.target],
        }
        // MOV1 only owns Idle/Moving; Engaging/Chasing are MOV4's (a path
        // reply while engaged simply waits in `waypoints`).
        *state = if agent.waypoints.is_empty() {
            AgentOrderState::Idle
        } else {
            AgentOrderState::Moving
        };
    }
}

// --------------------------------------------------------------------------
// MOV2: dirty recompute (BuildGrid observation)
// --------------------------------------------------------------------------

/// Director (before `move_agents`): rebuild `NavGrid` when the BuildGrid
/// changed this frame (change-detection observation — try_place / try_sell /
/// any `resource_mut::<BuildGrid>()` bumps the resource's change tick; the
/// build domain itself is never touched) or when `mark_nav_dirty` forced
/// it. After a real blocker change, every agent whose remaining route
/// crosses a now-blocked cell is re-enqueued through the PathRequestQueue
/// (frame-budgeted like any other request — that is what bounds the
/// re-path inside the acceptance window). Occupied buildable cells come
/// from `BuildGrid::occupant` probes (bounds scan; the occupancy map itself
/// is private to the build domain).
pub fn dirty_recompute(
    build: Option<Res<BuildGrid>>,
    mut nav: ResMut<NavGrid>,
    mut force: ResMut<NavForceDirty>,
    mut queue: ResMut<PathRequestQueue>,
    frame: Res<NavFrame>,
    agents: Query<(Entity, &MoveAgent, &Transform)>,
) {
    let forced = std::mem::take(&mut force.0);
    let changed = build.as_ref().is_some_and(|b| b.is_changed());
    if !forced && !changed {
        return;
    }
    // Rebuild the live blocked set. Geometry syncs from the live build grid
    // (FreeGrid only; FixedSlots maps keep the empty default grid — warden
    // zero drift).
    let mut next = nav.authored.clone();
    if let Some(build) = &build {
        if build.mode == BuildMode::FreeGrid {
            nav.cell = build.cell_size;
            nav.cols = build.cols;
            nav.rows = build.rows;
            nav.origin = build.origin;
            for col in 0..build.cols {
                for row in 0..build.rows {
                    let cell = Cell(col, row);
                    if build.blocked.contains(&cell) || build.occupant(cell).is_some() {
                        next.insert((col, row));
                    }
                }
            }
        }
    }
    if next == nav.blocked {
        return; // flagged but nothing actually moved — spare the re-path scan
    }
    nav.blocked = next;
    // Re-path agents whose remaining route crosses the new blockers; the
    // re-target is the last pending waypoint (the current destination).
    for (entity, agent, tf) in &agents {
        if agent.waypoints.is_empty() {
            continue;
        }
        let dest = *agent.waypoints.last().unwrap();
        let mut crosses = false;
        let mut prev = tf.translation;
        for w in &agent.waypoints {
            if !los_world(&nav, prev, *w) {
                crosses = true;
                break;
            }
            prev = *w;
        }
        if crosses {
            queue.push(entity, dest, frame.0);
        }
    }
}

// --------------------------------------------------------------------------
// MOV1: locomotion
// --------------------------------------------------------------------------

/// Director: advance free agents toward the current waypoint at constant
/// speed `speed * EffectiveStats.move_mult` (no StatMods => bare speed,
/// mult 1.0; mult clamped at >= 0). Intermediate waypoints are consumed
/// exactly (snap on reach, leftover budget continues); inside
/// `arrival_radius` of the FINAL waypoint the agent brakes to a stop
/// (waypoints cleared, Idle).
///
/// Ordering note: components are disjoint from `path::advance_followers`
/// by the MoveAgent/PathFollower mutual exclusion, so running in parallel
/// would be safe — chained `after` it anyway to keep the Director segment a
/// stable fixed order (replay hashing) and to anchor MOV3
/// `separate_agents` after all locomotion.
pub fn move_agents(
    time: Res<Time>,
    mut agents: Query<(
        Entity,
        &mut MoveAgent,
        &mut AgentOrderState,
        &mut Transform,
        Option<&EffectiveStats>,
    )>,
    followers: Query<(Entity, &PathFollower)>,
) {
    let dt = time.delta_secs();
    for (entity, mut agent, mut state, mut tf, eff) in &mut agents {
        if followers.get(entity).is_ok() {
            // Mutually exclusive by contract: track creeps vs free agents.
            // debug builds halt, release builds skip the entity.
            debug_assert!(
                false,
                "MoveAgent and PathFollower are mutually exclusive (entity {entity:?})"
            );
            continue;
        }
        // Engaging/Chasing units hold position (MOV4 owns those states).
        if *state == AgentOrderState::Engaging || *state == AgentOrderState::Chasing {
            agent.velocity = Vec3::ZERO;
            continue;
        }
        let mult = eff.map(|s| s.move_mult.max(0.0)).unwrap_or(1.0);
        let speed = agent.speed * mult;
        let mut budget = speed * dt;
        let mut vel = Vec3::ZERO;
        while budget > 0.0 && !agent.waypoints.is_empty() {
            let target = agent.waypoints[0];
            let delta = target - tf.translation;
            let dist = delta.length();
            let is_last = agent.waypoints.len() == 1;
            if is_last && dist <= agent.arrival_radius {
                // Arrival brake: stop inside the radius, don't walk to the
                // exact point.
                agent.waypoints.clear();
                break;
            }
            if dist <= budget {
                tf.translation = target;
                agent.waypoints.remove(0);
                budget -= dist;
            } else {
                let dir = delta / dist;
                tf.translation += dir * budget;
                vel = dir * speed;
                budget = 0.0;
            }
        }
        agent.velocity = vel;
        *state = if agent.waypoints.is_empty() {
            AgentOrderState::Idle
        } else {
            AgentOrderState::Moving
        };
    }
}

// --------------------------------------------------------------------------
// Bundle dispatch (SEAM0d stub filled in by MOV2; never edit bundle/mod.rs)
// --------------------------------------------------------------------------

/// Seed the NavGrid from the map bundle: geometry and scenery blockers from
/// `bundle::build_grid(&bundle.world)` (FreeGrid only — FixedSlots maps keep
/// the empty default grid), authored blockers = build-grid scenery ∪
/// `world.blocked` ∪ `nav.blocked`, budget from `nav.budget_per_frame`.
/// HashSet iteration feeds a BTreeSet, so the result is order-independent.
pub fn apply_nav(world: &mut World, bundle: &crate::bundle::MapBundle) {
    let build = crate::bundle::build_grid(&bundle.world);
    let mut nav = NavGrid {
        budget_per_frame: bundle.nav.budget_per_frame.max(1),
        ..Default::default()
    };
    if build.mode == BuildMode::FreeGrid {
        nav.cell = build.cell_size;
        nav.cols = build.cols;
        nav.rows = build.rows;
        nav.origin = build.origin;
        for c in &build.blocked {
            nav.authored.insert((c.0, c.1));
        }
        for c in &bundle.world.blocked {
            nav.authored.insert((c[0], c[1]));
        }
        for c in &bundle.nav.blocked {
            nav.authored.insert((c[0], c[1]));
        }
        nav.blocked = nav.authored.clone();
    }
    world.insert_resource(nav);
}

/// Domain systems (registered once by WarbasePlugin; increments add systems
/// HERE, never in lib.rs). MOV4 `tick_orders` will join the Tick slot.
pub struct MovementPlugin;

impl Plugin for MovementPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PathRequestQueue>()
            .init_resource::<NavFrame>()
            .init_resource::<AstarCount>()
            .init_resource::<NavForceDirty>()
            .add_systems(
                Update,
                process_path_requests.in_set(crate::WarbaseSet::Tick),
            )
            .add_systems(
                Update,
                (dirty_recompute, move_agents)
                    .chain()
                    .after(crate::path::advance_followers)
                    .in_set(crate::WarbaseSet::Director),
            );
    }
}

pub mod prelude {
    pub use super::{
        astar, dirty_recompute, mark_nav_dirty, move_agents, process_path_requests, request_path,
        smooth_path, AgentOrderState, AstarCount, MoveAgent, MovementPlugin, NavDef, NavForceDirty,
        NavFrame, NavGrid, Order, PathRequest, PathRequestQueue, UnitCommand,
    };
}
