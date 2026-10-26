//! Input layer (WC3 alignment, ADR-0016; ui feature): converts pointer
//! events into world-space `PickAttempt`s. The logic stays in `selection`
//! (headless-testable); this module owns screen->world math and raw input
//! bridging only.
//!
//! SEL1a SPIKE CONCLUSION — bevy_picking in Bevy 0.19.1
//! (verified against the vendored crate sources, not docs):
//!
//! 1. Availability: bevy 0.19.1 defaults to `["2d","3d","ui","audio"]` and
//!    both `2d` and `3d` pull the `"picking"` feature, so bevy_picking is
//!    compiled in this workspace by default (no extra dependency).
//!    `DefaultPlugins` auto-adds `bevy_picking::DefaultPickingPlugins`
//!    (bevy_internal-0.19.1/src/default_plugins.rs:103) and `bevy_ui`
//!    auto-adds its UI picking backend (`UiPickingPlugin`).
//! 2. Event shape: picking is pointer-entity + component based — pointer
//!    entities carry `PointerPress`/`PointerInteraction`; bubbled
//!    `Pointer<E>` messages (Click/Down/Up/Over/Out/Drag...) live in
//!    `bevy_picking::prelude`. The canonical hover state is the resource
//!    `bevy::picking::hover::HoverMap` =
//!    `HashMap<PointerId, EntityHashMap<HitData>>` (hover.rs:60).
//! 3. UI occlusion: hits from different backends are depth-sorted by camera
//!    order — a hit UI node sits ABOVE mesh hits, and `Pickable {
//!    should_block_lower: true }` (the default, also for entities without
//!    the component) stops lower entities from being hovered. So the clean
//!    "UI panels block world picks" oracle for a raw-input bridge is:
//!    PointerId::Mouse currently hovers at least one entity carrying a
//!    bevy_ui `Node`.
//! 4. Ortho cameras: `Camera::viewport_to_world(&GlobalTransform, pos) ->
//!    Result<Ray3d, _>` (bevy_camera-0.19.1/src/camera.rs:647) works for ANY
//!    projection incl. orthographic (it inverts `computed.clip_from_view`).
//!    Input is LOGICAL viewport pixels, y-down — the same space as
//!    `Window::cursor_position()`. It does NOT bounds-check: callers must
//!    test `Camera::logical_viewport_rect()` containment themselves.
//!    NOTE: 0.19 stores projections as one `Projection` enum component
//!    (`Projection::Orthographic(..)` — there is no OrthographicProjection
//!    component anymore); Camera2d requires the Orthographic variant.
//! 5. Headless: MinimalPlugins registers none of picking/Window/
//!    ButtonInput — every raw source this bridge reads is Option-gated and
//!    the bridge no-ops; headless tests inject selection messages directly
//!    (logic lives in the selection domain, hard rule).
//!
//! DECISION: the bridge consumes raw `ButtonInput<MouseButton>` +
//! primary-window cursor (predictable without the picking stack; the camera
//! math for a top-down ortho view is one plane intersection) and uses the
//! default picking stack ONLY as the UI-occlusion oracle (point 3) via
//! `HoverMap`. Games that prefer wholesale picking events can swap the
//! bridge; the seam (`WorldPicker` + `PickAttempt`/`SelectCommand`) holds.
//!
//! HOTKEY SEMANTICS (pinned here; the keyboard bridge below translates raw
//! keys into domain messages — the mouse bridge does not handle keys):
//! - Digit 1..=9, non-modal       -> `GroupCommand::Recall(digit)`
//! - Ctrl  + digit                -> `GroupCommand::Bind(digit)`
//! - Shift + digit                -> `GroupCommand::Insert(digit)`
//! - While `selection::ModalKeyClaim` is Some (hole-8): digits are YIELDED
//!   to the claiming modal — no GroupCommand is written; the claim owner
//!   reads ButtonInput itself. Tab / letters / Esc stay live.
//! - Tab                          -> `selection::CycleSubgroups`
//! - A / H / S / P / M / Esc      -> `selection::CommandHotkey` (SEL4: A/P/M
//!   arm `PendingTargetOrder` targeting, H/S fire at once, Esc cancels the
//!   pending state only — the selection set is untouched)

use bevy::picking::hover::HoverMap;
use bevy::picking::pointer::PointerId;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use crate::selection::{
    CommandHotkey, CycleSubgroups, GroupCommand, PickAttempt, PickButton, SelectCommand,
};
use crate::WarbaseSet;

/// Screen -> world mapping seam. Games with unusual cameras swap the
/// resource; the base consumes only `PickAttempt` downstream.
///
/// PLANE CONTRACT (hole-1 fix, unified base convention): the gameplay plane
/// is **y = 0 (XZ)** and every world `Vec2` in the pick/command chain means
/// ground `(x, z)` — the same convention movement/nav/build/combat and
/// warden's perspective picker already use.
pub trait WorldPicker: Send + Sync + 'static {
    /// Map a pointer position (window pixel coords) to ground (x, z). None
    /// when the pointer is off the playable plane.
    fn screen_to_world(&self, world: &World, screen: Vec2) -> Option<Vec2>;
}

/// Test/default implementation: identity mapping (screen == ground (x, z)).
/// Handy for headless harnesses driving picks directly.
#[derive(Resource, Debug, Default)]
pub struct IdentityPicker;

impl WorldPicker for IdentityPicker {
    fn screen_to_world(&self, _world: &World, screen: Vec2) -> Option<Vec2> {
        Some(screen)
    }
}

/// Default world strategy for WC3-style maps: top-down orthographic camera,
/// gameplay on the y=0 plane (XZ ground, hole-1 unified convention). Linear
/// unprojection via the camera inverse (works for panned/zoomed ortho views
/// without any picking backend).
#[derive(Resource, Debug, Default)]
pub struct OrthoTopDownPicker;

impl WorldPicker for OrthoTopDownPicker {
    fn screen_to_world(&self, world: &World, screen: Vec2) -> Option<Vec2> {
        // 0.19 keeps projections in one `Projection` enum component
        // (`Projection::Orthographic(..)`; there is no OrthographicProjection
        // component anymore) and World queries need &mut for QueryState
        // caching — the pinned &World seam iterates entities instead.
        // Highest camera order (the topmost view) wins; ties keep the first.
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
        // viewport_to_world extrapolates instead of failing — reject
        // off-viewport pointers ourselves (SEL1a point 4).
        let rect = cam.logical_viewport_rect()?;
        if screen.x < rect.min.x
            || screen.x > rect.max.x
            || screen.y < rect.min.y
            || screen.y > rect.max.y
        {
            return None;
        }
        // Ray through the pixel, intersected with the y=0 gameplay plane
        // (hole-1: base-wide XZ convention — a straight-down camera has
        // direction.y ~ -1 and lands here; the old z=0 test returned None
        // for every pixel of a top-down view).
        let ray = cam.viewport_to_world(transform, screen).ok()?;
        if ray.direction.y.abs() < 1e-6 {
            return None; // view direction parallel to the ground plane
        }
        let t = -ray.origin.y / ray.direction.y;
        if t < 0.0 {
            return None; // the ground is behind the camera
        }
        let point = ray.origin + ray.direction * t;
        Some(Vec2::new(point.x, point.z))
    }
}

/// The active screen->world strategy (swap seam): replace this resource to
/// change how the bridge maps pointer pixels onto the world plane.
#[derive(Resource)]
pub struct ActiveWorldPicker(pub Box<dyn WorldPicker>);

impl Default for ActiveWorldPicker {
    fn default() -> Self {
        Self(Box::new(OrthoTopDownPicker))
    }
}

/// Screen-space drag bookkeeping for box select (SEL3 semantics).
#[derive(Resource, Debug, Default)]
pub struct PointerDrag {
    /// Press position (window pixels) while the left button is held.
    pub anchor: Option<Vec2>,
}

/// Double-click bookkeeping (SEAM2c select-same-type): the last left click
/// (virtual time, cursor position). A click within
/// [`DOUBLE_CLICK_WINDOW`] seconds and [`DOUBLE_CLICK_SLOP_PX`] pixels of
/// the previous one reports `click_count: 2`.
#[derive(Resource, Debug, Default)]
pub struct ClickTracker {
    pub last: Option<(f32, Vec2)>,
}

/// Virtual-time window inside which two left clicks count as a double
/// click (interaction constant, same class as `DRAG_THRESHOLD_PX`).
const DOUBLE_CLICK_WINDOW: f32 = 0.3;
/// Cursor travel tolerance between the two clicks of a double click.
const DOUBLE_CLICK_SLOP_PX: f32 = 6.0;

/// Press+release travel below this many pixels counts as a click, not a box.
const DRAG_THRESHOLD_PX: f32 = 4.0;

/// Raw pointer bridge: ButtonInput + primary-window cursor -> WorldPicker ->
/// selection messages. Exclusive on purpose — it reads the world through the
/// picker seam and writes messages without param conflicts. No-ops whenever
/// raw input resources are absent (headless; see SEL1a point 5).
fn pointer_bridge(world: &mut World) {
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
        let mut drag = world.resource_mut::<PointerDrag>();
        if buttons.just_pressed(MouseButton::Left) {
            drag.anchor = Some(cursor);
        }
        if buttons.just_released(MouseButton::Left) {
            released_anchor = drag.anchor.take();
        }
    }

    for button in [MouseButton::Left, MouseButton::Right, MouseButton::Middle] {
        if !buttons.just_released(button) {
            continue;
        }
        let pick_button = match button {
            MouseButton::Left => PickButton::Left,
            MouseButton::Right => PickButton::Right,
            _ => PickButton::Middle,
        };
        // Left press->release spanning >= threshold pixels is a box select;
        // anything else (incl. an unmappable corner) falls back to a click.
        if button == MouseButton::Left {
            if let Some(anchor) = released_anchor.as_ref() {
                if (cursor - *anchor).length() >= DRAG_THRESHOLD_PX {
                    if let (Some(a), Some(b)) =
                        (map_to_world(world, *anchor), map_to_world(world, cursor))
                    {
                        world.write_message(SelectCommand::Aabb {
                            min: a.min(b),
                            max: a.max(b),
                        });
                        continue;
                    }
                }
            }
        }
        // UI panels must swallow world picks (SEL1a point 3).
        if ui_blocks_world_pick(world) {
            continue;
        }
        if let Some(point) = map_to_world(world, cursor) {
            // Click count: a left click close enough in time and space to
            // the previous one is a double click (select-same-type, SEAM2c);
            // everything else is a single click.
            let mut click_count = 1;
            if button == MouseButton::Left {
                let now = world.resource::<Time>().elapsed_secs();
                let mut tracker = world.resource_mut::<ClickTracker>();
                click_count = match tracker.last {
                    Some((t, pos))
                        if now - t <= DOUBLE_CLICK_WINDOW
                            && (pos - cursor).length() <= DOUBLE_CLICK_SLOP_PX =>
                    {
                        2
                    }
                    _ => 1,
                };
                tracker.last = Some((now, cursor));
            }
            world.write_message(PickAttempt {
                world: point,
                button: pick_button,
                click_count,
                ctrl,
                shift,
            });
        }
    }
}

fn primary_cursor(world: &World) -> Option<Vec2> {
    // 0.19: the primary window is an entity (Window + PrimaryWindow), not a
    // resource; read it through EntityRef to keep the &World borrow clean.
    world.iter_entities().find_map(|entity| {
        if entity.get::<PrimaryWindow>().is_none() {
            return None;
        }
        entity.get::<Window>()?.cursor_position()
    })
}

fn map_to_world(world: &World, screen: Vec2) -> Option<Vec2> {
    world
        .resource::<ActiveWorldPicker>()
        .0
        .screen_to_world(world, screen)
}

/// True when the primary pointer hovers a UI node (bevy_picking's default
/// stack reports it in `HoverMap`); absent picking (headless) = open field.
fn ui_blocks_world_pick(world: &World) -> bool {
    let Some(hover) = world.get_resource::<HoverMap>() else {
        return false;
    };
    hover
        .get(&PointerId::Mouse)
        .is_some_and(|hits| hits.keys().any(|e| world.get::<Node>(*e).is_some()))
}

/// Digit keys -> group slot digits (1-based), pinned key order.
const DIGIT_KEYS: [(KeyCode, u8); 9] = [
    (KeyCode::Digit1, 1),
    (KeyCode::Digit2, 2),
    (KeyCode::Digit3, 3),
    (KeyCode::Digit4, 4),
    (KeyCode::Digit5, 5),
    (KeyCode::Digit6, 6),
    (KeyCode::Digit7, 7),
    (KeyCode::Digit8, 8),
    (KeyCode::Digit9, 9),
];

/// Raw keyboard bridge (SEL4): the HOTKEY SEMANTICS table above as code —
/// A/H/S/P/M/Esc -> `CommandHotkey`, Tab -> `CycleSubgroups`, digits (with
/// Ctrl/Shift) -> `GroupCommand`. Hole-8: while `ModalKeyClaim` is Some the
/// digit translation is SKIPPED (digits belong to the claiming modal);
/// every other key stays live. The state machine behind those messages
/// stays in `selection` (headless-testable); this is raw-key translation
/// only. Exclusive like the pointer bridge (reads ButtonInput + writes
/// messages). No-ops whenever raw input resources are absent (headless;
/// see SEL1a point 5).
fn keyboard_bridge(world: &mut World) {
    let Some(keys) = world.get_resource::<ButtonInput<KeyCode>>().cloned() else {
        return;
    };
    let ctrl = keys.any_pressed([KeyCode::ControlLeft, KeyCode::ControlRight]);
    let shift = keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]);
    // Hole-8 yield seam: a claimed keyboard means digits are the modal's.
    let modal_claims_digits = world
        .get_resource::<crate::selection::ModalKeyClaim>()
        .is_some_and(|claim| claim.0.is_some());

    if keys.just_pressed(KeyCode::Escape) {
        world.write_message(CommandHotkey::Cancel);
    }
    if keys.just_pressed(KeyCode::KeyA) {
        world.write_message(CommandHotkey::AttackMove);
    }
    if keys.just_pressed(KeyCode::KeyH) {
        world.write_message(CommandHotkey::Hold);
    }
    if keys.just_pressed(KeyCode::KeyS) {
        world.write_message(CommandHotkey::Stop);
    }
    if keys.just_pressed(KeyCode::KeyP) {
        world.write_message(CommandHotkey::Patrol);
    }
    if keys.just_pressed(KeyCode::KeyM) {
        world.write_message(CommandHotkey::Move);
    }
    if keys.just_pressed(KeyCode::Tab) {
        world.write_message(CycleSubgroups);
    }
    if modal_claims_digits {
        return; // digits belong to the modal (see ModalKeyClaim contract)
    }
    for (code, digit) in DIGIT_KEYS {
        if !keys.just_pressed(code) {
            continue;
        }
        let command = if ctrl {
            GroupCommand::Bind(digit)
        } else if shift {
            GroupCommand::Insert(digit)
        } else {
            GroupCommand::Recall(digit)
        };
        world.write_message(command);
    }
}

/// Refreshes [`selection::SelectionScreenRect`] from the topmost
/// orthographic camera (SEAM2c): the ground (y=0) AABB under the camera's
/// logical viewport — corner rays intersected with the gameplay plane,
/// min/max over the four corners (rotation-tolerant). No camera (headless)
/// leaves the default infinite rect: everything is "on screen".
fn refresh_screen_rect(world: &mut World) {
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
    let Some((cam, transform)) = best.map(|(_, c, t)| (c, t)) else {
        return; // headless: keep the default infinite rect
    };
    let Some(rect) = cam.logical_viewport_rect() else { return };
    let corners = [
        rect.min,
        Vec2::new(rect.max.x, rect.min.y),
        Vec2::new(rect.min.x, rect.max.y),
        rect.max,
    ];
    let mut min = Vec2::splat(f32::MAX);
    let mut max = Vec2::splat(f32::MIN);
    let mut seen = false;
    for corner in corners {
        let Ok(ray) = cam.viewport_to_world(transform, corner) else { continue };
        if ray.direction.y.abs() < 1e-6 {
            continue; // view direction parallel to the ground plane
        }
        let t = -ray.origin.y / ray.direction.y;
        if t < 0.0 {
            continue; // ground behind the camera
        }
        let p = ray.origin + ray.direction * t;
        min = min.min(Vec2::new(p.x, p.z));
        max = max.max(Vec2::new(p.x, p.z));
        seen = true;
    }
    if seen {
        world.resource_mut::<crate::selection::SelectionScreenRect>().min = min;
        world.resource_mut::<crate::selection::SelectionScreenRect>().max = max;
    }
}

/// Domain systems (ui feature; registered once by WarbasePlugin).
pub struct InputPlugin;

impl Plugin for InputPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ActiveWorldPicker>()
            .init_resource::<PointerDrag>()
            .init_resource::<ClickTracker>()
            // D2: raw input translation runs at the very front of Update,
            // before WarbaseSet::Tick consumes the translated messages.
            .add_systems(
                Update,
                (pointer_bridge, keyboard_bridge, refresh_screen_rect)
                    .chain()
                    .before(WarbaseSet::Tick),
            );
    }
}

pub mod prelude {
    pub use super::{
        ActiveWorldPicker, ClickTracker, IdentityPicker, InputPlugin, OrthoTopDownPicker,
        PointerDrag, WorldPicker,
    };
}
