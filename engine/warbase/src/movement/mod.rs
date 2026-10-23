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
//! Also landed here (MOV3/4/5, plan §5):
//! - `separate_agents` (Director, after `move_agents`): soft-collision push
//!   via a hardcoded spatial-grid bucketing (see the constants below);
//!   track creeps (`PathFollower`) never participate — pinned rule.
//! - `tick_orders` (Tick): consumes `UnitCommand` messages (D4) — order
//!   assignment, MOV5 formation scatter, and the per-frame order state
//!   machine (Move / AttackMove / AttackTarget / HoldPosition / Patrol /
//!   Stop).
//! - `acquire_engagement` (Intent, after combat's `pick_targets`): the
//!   attack-move/patrol/hold engagement seam — scans the nearest hostile in
//!   weapon range (combat's own air/ground filter) and writes `CurrentTarget`
//!   + the Engaging/Moving label. combat/ itself is never touched (MOV4 hard
//!   constraint): `fire_attacks` already fires at any live `CurrentTarget`.
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
use std::collections::{BTreeMap, BTreeSet, BinaryHeap};

use crate::build::{BuildGrid, BuildMode, Cell};
use crate::buff::EffectiveStats;
use crate::combat::{AttackKit, CurrentTarget, Dead};
use crate::path::PathFollower;
use crate::unit::{AirUnit, Allegiance, Hostility, Hp, TargetDomain};

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

/// Documented default `MoveAgent::body_radius` (MOV3/MOV5): games that do
/// not carry a body size on their defs spawn agents with this. Same value
/// the plan's acceptance numbers assume (0.3-radius units).
pub const DEFAULT_BODY_RADIUS: f32 = 0.3;

/// Free-roaming locomotion component (NOT for wave creeps on tracks).
/// `speed` is the unit's base speed; `move_agents` multiplies it by
/// `EffectiveStats.move_mult` each frame (no StatMods on the entity => bare
/// speed, mult 1.0). `waypoints` is the pending route in world space,
/// consumed front-first — written by MOV2 `process_path_requests`, or seeded
/// directly for straight-line orders. `velocity` is derived state written by
/// `move_agents` (zero when idle) for readers (facing, separation, sfx).
/// `body_radius` (MOV3 soft collision, MOV5 slot spacing) is filled by the
/// game at spawn from its def (BaseAttributes stay in the registry; nothing
/// on the entity carries a radius) — default `DEFAULT_BODY_RADIUS`.
/// `order` (MOV4) is the currently assigned command; `None` = manual /
/// legacy agents (warden, loop-td) that `move_agents` alone drives.
/// Spawn together with `AgentOrderState`.
#[derive(Component, Debug)]
pub struct MoveAgent {
    pub speed: f32,
    pub arrival_radius: f32,
    pub waypoints: Vec<Vec3>,
    pub velocity: Vec3,
    pub body_radius: f32,
    pub order: Option<Order>,
}

/// Execution sub-state of the current order. Idle/Moving are maintained by
/// `move_agents` for order-less agents; Engaging/Chasing are MOV4's (MOV4's
/// `tick_orders`/`acquire_engagement` own the transitions): Engaging units
/// hold position in `move_agents` (attack-move pause / chase in range);
/// Chasing units WALK their MOV4-refreshed pursuit waypoints — the label
/// just says who steers.
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
        // MOV1 owns Idle/Moving; Engaging/Chasing are MOV4's — a path reply
        // landing mid-engagement updates the waiting route but never the
        // label (the engagement logic re-evaluates it next frame).
        if *state != AgentOrderState::Engaging && *state != AgentOrderState::Chasing {
            *state = if agent.waypoints.is_empty() {
                AgentOrderState::Idle
            } else {
                AgentOrderState::Moving
            };
        }
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
        // Engaging units hold position (MOV4's attack-move pause / chase in
        // range). Chasing units walk their MOV4-refreshed waypoints like any
        // mover — only the label differs (MOV4 owns the transitions, so the
        // trailing Idle/Moving rewrite is skipped for Chasing).
        if *state == AgentOrderState::Engaging {
            agent.velocity = Vec3::ZERO;
            continue;
        }
        let chasing = *state == AgentOrderState::Chasing;
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
        if !chasing {
            *state = if agent.waypoints.is_empty() {
                AgentOrderState::Idle
            } else {
                AgentOrderState::Moving
            };
        }
    }
}

// --------------------------------------------------------------------------
// MOV3: soft-collision separation
// --------------------------------------------------------------------------

/// Largest `body_radius` the bucketing below GUARANTEES to catch (plan §5
/// MOV3: bucket edge = 2 * max radius). HARDCODED by design — a constant
/// bucket grid keeps the pair set frame-stable and deterministic; 1.0 is
/// generous against a 1.0 nav cell (WC3-scale units sit near 0.3). Agents
/// with a larger radius simply degrade to neighbor-bucket coverage.
const SEPARATION_MAX_BODY_RADIUS: f32 = 1.0;

/// Spatial bucket edge = 2 * `SEPARATION_MAX_BODY_RADIUS`: two overlapping
/// agents (both radii <= max) are at most in adjacent buckets, so a
/// half-neighborhood scan sees every overlapping pair exactly once.
const SEPARATION_BUCKET_EDGE: f32 = 2.0 * SEPARATION_MAX_BODY_RADIUS;

/// Pair correction clamp per frame = 0.5 x nav cell. FixedSlots maps (and
/// the default NavGrid) have no geometry (cell == 0); they fall back to this
/// constant, which equals 0.5 x cell on the standard 1.0 grid.
const SEPARATION_FALLBACK_MAX_PUSH: f32 = 0.5;

/// Positions closer than this count as EXACTLY stacked (the spawn-pile
/// case): the separation axis is undefined, so a pinned axis is used.
const SEPARATION_COINCIDENT_EPS: f32 = 1e-6;

/// Half-neighborhood bucket offsets: together with the in-bucket i<j scan
/// each unordered bucket pair is visited exactly once per frame.
const SEPARATION_NEIGHBORS: [(i32, i32); 4] = [(1, 0), (1, 1), (0, 1), (-1, 1)];

/// One soft-collision correction between entries `i` (lower entity index)
/// and `j`: if the body circles (radii from `MoveAgent::body_radius`)
/// overlap by less than 50%, both sides retreat half the overlap each along
/// the center axis (per-pair clamp `max_push`, see constants above).
/// Pinned rules:
/// - No overlap (dist >= ri + rj): nothing.
/// - Deep penetration (0 < dist < 0.5 x (ri + rj)): DO NOT push — the
///   anti-jitter rule (plan §5 MOV3 "穿透>50% 不推"): teleport-style pileups
///   are resolved by the game, not by a violent pop.
/// - Exactly stacked (dist ~ 0): push along the pinned +/-x axis by half
///   the wanted distance — the deep-penetration skip above deliberately
///   does not apply, or coincident spawn piles could never separate.
fn separation_push(
    entries: &[(Entity, f32, Vec2)],
    i: usize,
    j: usize,
    max_push: f32,
    pushes: &mut [Vec2],
) {
    let want = entries[i].1 + entries[j].1;
    if want <= 0.0 {
        return;
    }
    let delta = entries[j].2 - entries[i].2;
    let dist = delta.length();
    if dist >= want {
        return; // not overlapping
    }
    let (dir, overlap) = if dist < SEPARATION_COINCIDENT_EPS {
        (Vec2::new(1.0, 0.0), want)
    } else {
        if dist < want * 0.5 {
            return; // deep penetration: hold (anti-jitter)
        }
        (delta / dist, want - dist)
    };
    let step = (overlap * 0.5).min(max_push);
    pushes[i] -= dir * step;
    pushes[j] += dir * step;
}

/// Director (after `move_agents`): soft-collision separation for free
/// agents — mutually overlapping `MoveAgent`s push each other apart by
/// position correction (never velocity; `move_agents` rewrites velocity).
/// Track creeps (`PathFollower`) NEVER participate — pinned rule (plan §5
/// MOV3; the TD games' creep flow stays exactly as it was).
///
/// Determinism (D6): agents are snapshotted in entity-index order, buckets
/// live in a `BTreeMap` (key order), bucket members are index-ascending,
/// and the pinned half-neighborhood offsets fix the pair enumeration order —
/// the accumulated f32 pushes are a pure function of the snapshot.
pub fn separate_agents(
    nav: Res<NavGrid>,
    mut agents: Query<(Entity, &MoveAgent, &mut Transform), Without<PathFollower>>,
) {
    if agents.is_empty() {
        return;
    }
    // Snapshot (Entity, body_radius, xz-position) in entity-index order.
    let mut entries: Vec<(Entity, f32, Vec2)> = agents
        .iter()
        .map(|(e, agent, tf)| {
            (e, agent.body_radius.max(0.0), Vec2::new(tf.translation.x, tf.translation.z))
        })
        .collect();
    entries.sort_by_key(|&(e, _, _)| e.index_u32());
    let mut pushes: Vec<Vec2> = vec![Vec2::ZERO; entries.len()];
    let max_push = if nav.cell > 0.0 {
        nav.cell * 0.5
    } else {
        SEPARATION_FALLBACK_MAX_PUSH
    };

    // Spatial buckets (BTreeMap: deterministic key order; members are pushed
    // in index order, so each bucket's Vec is index-ascending).
    let mut buckets: BTreeMap<(i32, i32), Vec<usize>> = BTreeMap::new();
    for (i, &(_, _, pos)) in entries.iter().enumerate() {
        let key = (
            (pos.x / SEPARATION_BUCKET_EDGE).floor() as i32,
            (pos.y / SEPARATION_BUCKET_EDGE).floor() as i32,
        );
        buckets.entry(key).or_default().push(i);
    }
    for (&key, members) in &buckets {
        for a in 0..members.len() {
            for b in (a + 1)..members.len() {
                separation_push(&entries, members[a], members[b], max_push, &mut pushes);
            }
        }
        for &(dx, dy) in SEPARATION_NEIGHBORS.iter() {
            let Some(nbr) = buckets.get(&(key.0 + dx, key.1 + dy)) else {
                continue;
            };
            for &a in members {
                for &b in nbr {
                    separation_push(&entries, a, b, max_push, &mut pushes);
                }
            }
        }
    }

    // Apply the accumulated corrections (index order; y untouched).
    for (i, &(entity, _, _)) in entries.iter().enumerate() {
        if pushes[i] == Vec2::ZERO {
            continue;
        }
        if let Ok((_, _, mut tf)) = agents.get_mut(entity) {
            tf.translation.x += pushes[i].x;
            tf.translation.z += pushes[i].y;
        }
    }
}

// --------------------------------------------------------------------------
// MOV5: formation scatter (consumed by MOV4 `tick_orders`)
// --------------------------------------------------------------------------

/// Deterministic hex-ring slot table around a command point (MOV5, plan §5):
/// slot 0 IS the order point (single-unit commands go straight to it); ring
/// k >= 1 adds 6k slots on a hexagon of radius k * spacing, slot angles
/// 2*pi*j/(6k) counted from +x. `spacing` = 2 x max subject body radius, so
/// every slot pair (and each slot vs the center) is at least `spacing`
/// apart — comfortably above the plan's 0.9 x (ri + rj) scatter bound.
/// Fixed algorithm, no RNG (D6); subjects are assigned slots in entity-index
/// order by `tick_orders`.
fn formation_slots(center: Vec3, count: usize, spacing: f32) -> Vec<Vec3> {
    let mut slots = vec![center];
    let mut ring: usize = 1;
    while slots.len() < count {
        let n = 6 * ring;
        for j in 0..n {
            let angle = (j as f32 / n as f32) * std::f32::consts::TAU;
            slots.push(Vec3::new(
                center.x + ring as f32 * spacing * angle.cos(),
                0.0,
                center.z + ring as f32 * spacing * angle.sin(),
            ));
        }
        ring += 1;
    }
    slots
}

// --------------------------------------------------------------------------
// MOV4: order consumption + state machine
// --------------------------------------------------------------------------

/// Tick (before `process_path_requests`): the WC3 command state machine.
///
/// Phase 1 — consume `UnitCommand` messages (arrival order, D4):
/// - Move / AttackMove: MOV5 scatter (subjects sorted by entity index, slot
///   i for the i-th subject; single subject = slot 0 = the order point),
///   seed `waypoints = [slot]` (instant straight-line best-effort, MOV2's
///   contract) and enqueue the same target through the budgeted
///   PathRequestQueue — the A* reply overwrites the seed with the true
///   route.
/// - AttackTarget: assign the order; phase 2 evaluates range this frame.
/// - HoldPosition: clear waypoints, stand.
/// - Patrol: seed the first leg (toward_b ? b : a) through the same queue.
/// - Stop: clear waypoints and the order.
/// - Pickup / Follow: stored only — executed by the future INV/Follow
///   increments (no behavior yet, pinned as parked).
///
/// Phase 2 — per-frame order advancement over every ordered agent:
/// - Move / AttackMove: arrival (empty waypoints + Idle) spends the order
///   (back to order-less manual locomotion).
/// - AttackTarget: target gone => order ends (Idle, waypoints cleared);
///   in weapon range (kit range; arrival_radius when kit-less) => Engaging
///   (holds, combat fires via CurrentTarget); out of range => Chasing with
///   `waypoints = [target position]` refreshed EVERY frame.
/// - Patrol: reaching an endpoint (empty waypoints + Idle) flips toward_b
///   and seeds the other leg through the queue. Engagement during a leg is
///   `acquire_engagement`'s business (AttackMove semantics), not here.
///
/// Zero drift: order-less agents (warden, loop-td) never enter phase 2.
#[allow(clippy::type_complexity)]
pub fn tick_orders(
    mut commands: MessageReader<UnitCommand>,
    mut queue: ResMut<PathRequestQueue>,
    frame: Res<NavFrame>,
    mut agents: Query<(
        Entity,
        &mut MoveAgent,
        &mut AgentOrderState,
        &Transform,
        Option<&AttackKit>,
    )>,
    others: Query<(&Transform, &Hp), Without<Dead>>,
) {
    // ---- Phase 1: command consumption ----
    for cmd in commands.read() {
        match &cmd.order {
            Order::Move { to } | Order::AttackMove { to } => {
                let mut subjects: Vec<Entity> = cmd
                    .subjects
                    .iter()
                    .copied()
                    .filter(|e| agents.get(*e).is_ok())
                    .collect();
                if subjects.is_empty() {
                    continue;
                }
                subjects.sort_by_key(|e| e.index_u32());
                subjects.dedup();
                // Slot spacing from the subjects' own bodies (MOV5): two radii
                // must fit between neighbours; floor at the documented default.
                let max_radius = subjects
                    .iter()
                    .filter_map(|e| agents.get(*e).ok().map(|a| a.1.body_radius))
                    .fold(0.0f32, f32::max)
                    .max(DEFAULT_BODY_RADIUS);
                let slots = formation_slots(*to, subjects.len(), 2.0 * max_radius);
                for (i, &entity) in subjects.iter().enumerate() {
                    let Ok((_, mut agent, mut state, _, _)) = agents.get_mut(entity) else {
                        continue;
                    };
                    agent.order = Some(cmd.order.clone());
                    agent.waypoints = vec![slots[i]];
                    agent.velocity = Vec3::ZERO;
                    *state = AgentOrderState::Moving;
                    queue.push(entity, slots[i], frame.0);
                }
            }
            Order::AttackTarget { .. } => {
                for &entity in &cmd.subjects {
                    let Ok((_, mut agent, _, _, _)) = agents.get_mut(entity) else {
                        continue;
                    };
                    agent.order = Some(cmd.order.clone());
                    // Range evaluated by phase 2 this same frame.
                }
            }
            Order::HoldPosition => {
                for &entity in &cmd.subjects {
                    let Ok((_, mut agent, mut state, _, _)) = agents.get_mut(entity) else {
                        continue;
                    };
                    agent.waypoints.clear();
                    agent.velocity = Vec3::ZERO;
                    agent.order = Some(Order::HoldPosition);
                    *state = AgentOrderState::Idle;
                }
            }
            Order::Stop => {
                for &entity in &cmd.subjects {
                    let Ok((_, mut agent, mut state, _, _)) = agents.get_mut(entity) else {
                        continue;
                    };
                    agent.waypoints.clear();
                    agent.velocity = Vec3::ZERO;
                    agent.order = None;
                    *state = AgentOrderState::Idle;
                }
            }
            Order::Patrol { a, b, toward_b } => {
                for &entity in &cmd.subjects {
                    let Ok((_, mut agent, mut state, _, _)) = agents.get_mut(entity) else {
                        continue;
                    };
                    let dest = if *toward_b { *b } else { *a };
                    agent.order = Some(cmd.order.clone());
                    agent.waypoints = vec![dest];
                    agent.velocity = Vec3::ZERO;
                    *state = AgentOrderState::Moving;
                    queue.push(entity, dest, frame.0);
                }
            }
            // Parked orders: stored for the future INV/Follow increments.
            Order::Pickup { .. } | Order::Follow { .. } => {
                for &entity in &cmd.subjects {
                    let Ok((_, mut agent, mut state, _, _)) = agents.get_mut(entity) else {
                        continue;
                    };
                    agent.waypoints.clear();
                    agent.order = Some(cmd.order.clone());
                    *state = AgentOrderState::Idle;
                }
            }
        }
    }

    // ---- Phase 2: per-frame order advancement ----
    for (entity, mut agent, mut state, tf, kit) in &mut agents {
        let Some(order) = agent.order.clone() else { continue };
        match order {
            Order::Move { .. } | Order::AttackMove { .. } => {
                // Arrival brake emptied the route: the order is spent.
                if agent.waypoints.is_empty() && *state == AgentOrderState::Idle {
                    agent.order = None;
                }
            }
            Order::AttackTarget { target } => {
                // Order ends when the target is gone (despawned, or dead
                // pending this frame's Cleanup).
                let Ok((ttpos, hp)) = others.get(target) else {
                    agent.order = None;
                    agent.waypoints.clear();
                    agent.velocity = Vec3::ZERO;
                    *state = AgentOrderState::Idle;
                    continue;
                };
                if hp.0 <= 0.0 {
                    agent.order = None;
                    agent.waypoints.clear();
                    agent.velocity = Vec3::ZERO;
                    *state = AgentOrderState::Idle;
                    continue;
                }
                let range = kit.map(|k| k.def.range).unwrap_or(agent.arrival_radius);
                if tf.translation.distance(ttpos.translation) <= range {
                    // In weapon range: stand and fire (combat supplies the
                    // shots — pick_targets/acquire write CurrentTarget).
                    agent.waypoints.clear();
                    agent.velocity = Vec3::ZERO;
                    *state = AgentOrderState::Engaging;
                } else {
                    // Chase: pursuit waypoint refreshed every frame.
                    agent.waypoints = vec![ttpos.translation];
                    *state = AgentOrderState::Chasing;
                }
            }
            Order::Patrol { a, b, toward_b } => {
                if agent.waypoints.is_empty() && *state == AgentOrderState::Idle {
                    let toward_b = !toward_b;
                    let dest = if toward_b { b } else { a };
                    agent.order = Some(Order::Patrol { a, b, toward_b });
                    agent.waypoints = vec![dest];
                    agent.velocity = Vec3::ZERO;
                    *state = AgentOrderState::Moving;
                    queue.push(entity, dest, frame.0);
                }
            }
            // HoldPosition stands until another order replaces it; Stop was
            // fully handled in phase 1 (defensive clear here); Pickup/Follow
            // are parked.
            Order::HoldPosition => {}
            Order::Stop => agent.order = None,
            Order::Pickup { .. } | Order::Follow { .. } => {}
        }
    }
}

/// Intent (after combat's `pick_targets`): the engagement seam for
/// ordered movers (plan §5 MOV4 — combat itself is never modified; this
/// system writes `CurrentTarget` for its agents and lets `fire_attacks` do
/// what it already does).
///
/// - AttackMove / Patrol, state Moving: nearest hostile inside weapon range
///   (combat's own air/ground filter, Hostility oracle) => write
///   CurrentTarget, state -> Engaging (move_agents then holds the unit;
///   waypoints are PRESERVED — the route resumes after the fight).
/// - AttackMove / Patrol, state Engaging: re-scan each frame — no hostile
///   left in range (killed, fled, despawned) => clear CurrentTarget,
///   state -> Moving (the paused route continues).
/// - HoldPosition: same nearest-in-range scan into CurrentTarget, but the
///   state NEVER changes (out of range => no target, no chase, no shot).
/// - AttackTarget / order-less agents: not here (tick_orders owns the
///   chase; pick_targets alone serves everyone).
///
/// Running AFTER `pick_targets` makes this the last CurrentTarget writer in
/// Intent for engaged movers, so "nearest" (the engagement rule) wins over
/// the kit's own priority; every other attacker keeps pick_targets' pick.
#[allow(clippy::type_complexity)]
pub fn acquire_engagement(
    hostility: Res<Hostility>,
    mut agents: Query<(
        &mut MoveAgent,
        &mut AgentOrderState,
        &Transform,
        &Allegiance,
        &AttackKit,
        &mut CurrentTarget,
    )>,
    enemies: Query<
        (
            Entity,
            &Transform,
            &Hp,
            &Allegiance,
            Has<AirUnit>,
            Option<&PathFollower>,
        ),
        Without<Dead>,
    >,
) {
    for (agent, mut state, tf, side, kit, mut target) in &mut agents {
        let mover = matches!(
            agent.order.as_ref(),
            Some(Order::AttackMove { .. }) | Some(Order::Patrol { .. })
        );
        let holding = matches!(agent.order.as_ref(), Some(Order::HoldPosition));
        if !mover && !holding {
            continue;
        }
        // Engaging is meaningful only for movers; HoldPosition never relabels.
        if mover
            && !matches!(
                *state,
                AgentOrderState::Moving | AgentOrderState::Engaging
            )
        {
            continue;
        }
        // Nearest hostile in weapon range; total-order tie-break on entity
        // index (D6 — query iteration order never decides logic).
        let mut best: Option<(f32, u32, Entity)> = None;
        for (e, etf, hp, eside, air, _) in &enemies {
            if hp.0 <= 0.0 || !hostility.hostile(*side, *eside) {
                continue;
            }
            if kit.def.targets == TargetDomain::Ground && air {
                continue;
            }
            if kit.def.targets == TargetDomain::Air && !air {
                continue;
            }
            let dist = tf.translation.distance(etf.translation);
            if dist > kit.def.range {
                continue;
            }
            let idx = e.index_u32();
            let better = match best {
                None => true,
                Some((bd, bi, _)) => {
                    dist.total_cmp(&bd).then(idx.cmp(&bi)) == Ordering::Less
                }
            };
            if better {
                best = Some((dist, idx, e));
            }
        }
        match best {
            Some((_, _, e)) => {
                target.0 = Some(e);
                if mover {
                    *state = AgentOrderState::Engaging;
                }
            }
            None => {
                target.0 = None;
                if mover && *state == AgentOrderState::Engaging {
                    *state = AgentOrderState::Moving; // resume the preserved route
                }
            }
        }
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
/// HERE, never in lib.rs). Tick: `tick_orders` (command consumption + order
/// state machine) before `process_path_requests` (same-frame routing of
/// fresh orders). Director: (dirty_recompute, move_agents, separate_agents)
/// — locomotion then soft-collision separation. Intent:
/// `acquire_engagement` after combat's `pick_targets` (last CurrentTarget
/// writer for engaged movers — see its doc).
pub struct MovementPlugin;

impl Plugin for MovementPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PathRequestQueue>()
            .init_resource::<NavFrame>()
            .init_resource::<AstarCount>()
            .init_resource::<NavForceDirty>()
            .add_systems(
                Update,
                (tick_orders, process_path_requests)
                    .chain()
                    .in_set(crate::WarbaseSet::Tick),
            )
            .add_systems(
                Update,
                (dirty_recompute, move_agents, separate_agents)
                    .chain()
                    .after(crate::path::advance_followers)
                    .in_set(crate::WarbaseSet::Director),
            )
            .add_systems(
                Update,
                acquire_engagement
                    .after(crate::combat::pick_targets)
                    .in_set(crate::WarbaseSet::Intent),
            );
    }
}

pub mod prelude {
    pub use super::{
        acquire_engagement, astar, dirty_recompute, mark_nav_dirty, move_agents,
        process_path_requests, request_path, separate_agents, smooth_path, tick_orders,
        AgentOrderState, AstarCount, DEFAULT_BODY_RADIUS, MoveAgent, MovementPlugin, NavDef,
        NavForceDirty, NavFrame, NavGrid, Order, PathRequest, PathRequestQueue, UnitCommand,
    };
}
