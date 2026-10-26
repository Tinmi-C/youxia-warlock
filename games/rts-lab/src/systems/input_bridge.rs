//! Game-side pointer bridge (channel-C patch, hole #4 — see the RL1 report).
//!
//! The base pick chain assumes the gameplay plane is **z = 0** with world
//! Vec2 = (x, y):
//! - `input::OrthoTopDownPicker` intersects the pointer ray with z = 0
//!   (`-ray.origin.z / ray.direction.z`) — a straight-down camera has
//!   direction.z ~ 0 and gets `None` for every pixel;
//! - `selection::resolve_picks` / `apply_select_commands` hit bodies via
//!   `translation.truncate()` = (x, y) — XZ-plane units (y = 0) collapse
//!   onto the x axis;
//! - `selection::smart_resolve` / `issue_targeted_orders` build orders with
//!   `pick.world.extend(0.0)` = (x, y, 0) — off the ground.
//! The base movement/nav/build domains (and every real map) play on the
//! **y = 0** plane with Vec2 = (x, z) (`movement` world_to_cell takes
//! (tf.x, tf.z), waypoints are Vec3::new(p.x, 0.0, p.y)). No single plane
//! satisfies both halves. Until the base unifies, rts-lab silences the base
//! pointer layer (via the documented `ActiveWorldPicker` swap seam) and
//! bridges pointer input itself with XZ geometry, driving the base's
//! plane-free command messages (`SelectCommand` / `UnitCommand` — D4).
//! The base KEYBOARD bridge stays: H/S/Esc/Tab/digits are plane-free. (The
//! A/P targeted-order path inherits the plane bug — RL2 scope.)

use bevy::picking::hover::HoverMap;
use bevy::picking::pointer::PointerId;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
// The input module's prelude is not wired into the crate prelude (plan D3
// froze lib.rs before input landed) — reach it by path.
use warbase::input::prelude::*;
use warbase::prelude::*;

use crate::resources::RtsDrag;

/// Drag threshold in pixels (same pinned constant as the base input layer).
const DRAG_THRESHOLD_PX: f32 = 4.0;

/// Swap-in picker that maps nothing: keeps the base `pointer_bridge` silent
/// so it cannot double-drive picks through the broken-geometry chain.
pub struct NullPicker;

impl WorldPicker for NullPicker {
    fn screen_to_world(&self, _world: &World, _screen: Vec2) -> Option<Vec2> {
        None
    }
}

/// Startup: take over the `ActiveWorldPicker` seam (documented swap point).
pub fn silence_base_picker(world: &mut World) {
    world.insert_resource(ActiveWorldPicker(Box::new(NullPicker)));
}

/// Screen point -> gameplay-plane Vec2 = (x, z) via the topmost orthographic
/// camera. The base `OrthoTopDownPicker`'s math, plane-corrected to y = 0.
fn xz_at(world: &World, screen: Vec2) -> Option<Vec2> {
    let mut best: Option<(isize, &Camera, &GlobalTransform)> = None;
    for entity in world.iter_entities() {
        let (Some(cam), Some(transform), Some(projection)) = (
            entity.get::<Camera>(),
            entity.get::<GlobalTransform>(),
            entity.get::<Projection>(),
        ) else {
            continue;
        };
        if !matches!(projection, Projection::Orthographic(_)) {
            continue;
        }
        if best.map_or(true, |(order, _, _)| cam.order > order) {
            best = Some((cam.order, cam, transform));
        }
    }
    let (_, cam, transform) = best?;
    // Reject off-viewport pointers (viewport_to_world extrapolates instead
    // of failing — same guard as the base picker).
    let rect = cam.logical_viewport_rect()?;
    if screen.x < rect.min.x
        || screen.x > rect.max.x
        || screen.y < rect.min.y
        || screen.y > rect.max.y
    {
        return None;
    }
    let ray = cam.viewport_to_world(transform, screen).ok()?;
    if ray.direction.y.abs() < 1e-6 {
        return None; // view direction parallel to the ground plane
    }
    let t = -ray.origin.y / ray.direction.y;
    let point = ray.origin + ray.direction * t;
    Some(Vec2::new(point.x, point.z))
}

fn primary_cursor(world: &World) -> Option<Vec2> {
    world.iter_entities().find_map(|entity| {
        if entity.get::<PrimaryWindow>().is_none() {
            return None;
        }
        entity.get::<Window>()?.cursor_position()
    })
}

/// True when the primary pointer hovers a UI node — panels swallow world
/// picks (same oracle as the base input layer; absent picking = open field).
fn ui_blocks_world_pick(world: &World) -> bool {
    let Some(hover) = world.get_resource::<HoverMap>() else {
        return false;
    };
    hover
        .get(&PointerId::Mouse)
        .is_some_and(|hits| hits.keys().any(|e| world.get::<Node>(*e).is_some()))
}

/// Selectable bodies whose disc contains the point, nearest first with
/// Entity-index tie-break — the pinned rules of base `selection::resolve_picks`,
/// geometry corrected to (x, z).
fn pick_bodies(world: &mut World, point: Vec2) -> Vec<Entity> {
    let registry = world.resource::<UnitRegistry>().clone();
    let mut q = world
        .query_filtered::<(Entity, &Transform, Option<&UnitInstance>), With<Selectable>>();
    let mut hits: Vec<(f32, Entity)> = q
        .iter(world)
        .filter_map(|(entity, tf, instance)| {
            let radius = instance
                .and_then(|i| registry.get(&i.def))
                .map_or(0.0, |d| d.attrs.body_radius);
            let center = Vec2::new(tf.translation.x, tf.translation.z);
            let dist = center.distance(point);
            (dist <= radius + AABB_EDGE_EPSILON).then_some((dist, entity))
        })
        .collect();
    hits.sort_by(|a, b| {
        a.0.total_cmp(&b.0).then_with(|| a.1.index().cmp(&b.1.index()))
    });
    hits.into_iter().map(|(_, e)| e).collect()
}

/// Pure decision (unit-tested headlessly): a right-click resolves to which
/// order? Hostile first-hit -> AttackTarget for the whole selection; open
/// ground -> Move ON the gameplay plane (x, 0, z); friendly hits issue
/// nothing (WC3 Follow is a parked base order).
pub fn right_click_order(hit: Option<Entity>, hostile: bool, ground: Vec2) -> Option<Order> {
    match hit {
        Some(target) if hostile => Some(Order::AttackTarget { target }),
        Some(_) => None,
        None => Some(Order::Move { to: Vec3::new(ground.x, 0.0, ground.y) }),
    }
}

/// Exclusive pointer bridge: raw mouse -> XZ geometry -> base command
/// messages. Left click = select (Shift add / Ctrl toggle / ground clears);
/// >= threshold drag = box select; right click = smart order for the whole
/// selection. No-ops headless (no window / ButtonInput).
pub fn rts_pointer_bridge(world: &mut World) {
    let Some(cursor) = primary_cursor(world) else {
        return;
    };
    let Some(buttons) = world.get_resource::<ButtonInput<MouseButton>>().cloned() else {
        return;
    };
    let (ctrl, shift) = world
        .get_resource::<ButtonInput<KeyCode>>()
        .map(|keys| {
            (
                keys.any_pressed([KeyCode::ControlLeft, KeyCode::ControlRight]),
                keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]),
            )
        })
        .unwrap_or((false, false));

    // Drag anchor bookkeeping (short mutable borrow, released at block end).
    let mut released_anchor = None;
    {
        let mut drag = world.get_resource_or_insert_with(RtsDrag::default);
        if buttons.just_pressed(MouseButton::Left) {
            drag.anchor = Some(cursor);
        }
        if buttons.just_released(MouseButton::Left) {
            released_anchor = drag.anchor.take();
        }
    }

    for button in [MouseButton::Left, MouseButton::Right] {
        if !buttons.just_released(button) {
            continue;
        }
        // Press->release spanning >= threshold pixels is a box select.
        if button == MouseButton::Left {
            if let Some(anchor) = released_anchor.as_ref() {
                if (cursor - *anchor).length() >= DRAG_THRESHOLD_PX {
                    if let (Some(a), Some(b)) = (xz_at(world, *anchor), xz_at(world, cursor)) {
                        world.write_message(SelectCommand::Aabb {
                            min: a.min(b),
                            max: a.max(b),
                        });
                    }
                    continue;
                }
            }
        }
        // UI panels swallow world picks.
        if ui_blocks_world_pick(world) {
            continue;
        }
        let Some(point) = xz_at(world, cursor) else {
            continue;
        };
        let hits = pick_bodies(world, point);
        match button {
            MouseButton::Left => {
                let command = match hits.first() {
                    Some(&first) if shift => SelectCommand::Add(first),
                    Some(&first) if ctrl => SelectCommand::Toggle(first),
                    Some(&first) => SelectCommand::Set(vec![first]),
                    None if !shift => SelectCommand::Clear,
                    None => continue,
                };
                world.write_message(command);
            }
            MouseButton::Right => {
                let selected = world.resource::<SelectionState>().entities.clone();
                if selected.is_empty() {
                    continue;
                }
                let hostility = world.resource::<Hostility>().clone();
                let side = world
                    .get::<Allegiance>(selected[0])
                    .copied()
                    .unwrap_or(Allegiance::Defender);
                let hostile = hits.first().is_some_and(|&target| {
                    world
                        .get::<Allegiance>(target)
                        .is_some_and(|target_side| hostility.hostile(side, *target_side))
                });
                if let Some(order) = right_click_order(hits.first().copied(), hostile, point) {
                    world.write_message(UnitCommand { order, subjects: selected });
                }
            }
            _ => {}
        }
    }
}
