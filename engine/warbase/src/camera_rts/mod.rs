//! RTS camera control domain (SEAM2d seam, CAM increment, round-2 plan §3):
//! edge scroll, control-group double-tap jump, F1 hero focus, wheel zoom,
//! and the minimap ping vocabulary.
//!
//! LAYERING (round-2 task rule: message/state = pure headless logic;
//! systems that touch the window or drive the camera ride the `ui`
//! feature):
//! - Pure (always compiled, message-in/message-out, headless-testable):
//!   the `CameraJump`/`MinimapPing` messages, the `EdgeScroll`/
//!   `CameraOpts`/`KeyTapTracker` resources, the `group_double_tap`/
//!   `f1_hero_focus` producers, and the `edge_scroll_delta`/`zoomed_scale`
//!   math. Digit taps arrive as `selection::GroupCommand::Recall` — the
//!   ui keyboard bridge already did the raw-key translation (including the
//!   hole-8 `ModalKeyClaim` yield), so this half never reads keys itself.
//! - `ui` feature (need a window or drive a camera): `windowed::{edge_
//!   scroll, wheel_zoom, jump_camera}`.
//! Every default is identity/off — an app that never configures this
//! domain behaves exactly as before (zero drift).

use bevy::prelude::*;

use crate::combat::Dead;
use crate::hero::HeroStats;
use crate::selection::{ControlGroups, GroupCommand};
use crate::unit::{Allegiance, TeamId, XpHolder};
use crate::WarbaseSet;

/// Edge-scroll parameters (resource; a speed of 0.0 — the default —
/// disables the behavior, matching pre-SEAM2d camera-less apps exactly).
#[derive(Resource, Debug, Clone, Copy, PartialEq)]
pub struct EdgeScroll {
    /// Camera travel speed while the cursor rides a window edge
    /// (pixels of window per second of travel at 1:1 mapping; the ui-half
    /// system converts to world units through the camera scale).
    pub speed_px_per_sec: f32,
    /// Dead-zone width from the window border that counts as "on the
    /// edge". 0.0 disables (no pixel is ever "on the edge").
    pub margin_px: f32,
}

impl Default for EdgeScroll {
    fn default() -> Self {
        Self { speed_px_per_sec: 0.0, margin_px: 0.0 }
    }
}

/// Wheel-zoom parameters (resource; `zoom_step` 0.0 — the default —
/// disables the behavior, zero drift for pre-CAM apps).
#[derive(Resource, Debug, Clone, Copy, PartialEq)]
pub struct CameraOpts {
    /// Multiplicative zoom factor per Line-unit wheel step (> 1.0 when
    /// enabled). Scroll UP = zoom IN = the ortho scale SHRINKS.
    pub zoom_step: f32,
    /// Lower ortho-scale clamp (the most zoomed IN view). The identity
    /// default (with zoom disabled) is 0.0.
    pub zoom_min: f32,
    /// Upper ortho-scale clamp (the most zoomed OUT view). Identity
    /// default: +inf (clamping changes nothing).
    pub zoom_max: f32,
}

impl Default for CameraOpts {
    fn default() -> Self {
        Self { zoom_step: 0.0, zoom_min: 0.0, zoom_max: f32::INFINITY }
    }
}

/// Camera-center jump request (ground `(x, z)`, hole-1 XZ convention).
/// Producers are pure logic (group double-tap, F1 hero focus; game minimap
/// layers may write it too); the consumer that actually moves the camera
/// rides the ui feature.
#[derive(Message, Debug, Clone, Copy, PartialEq)]
pub struct CameraJump {
    pub to: Vec2,
}

/// A minimap ping (alert marker at ground `pos`, attributed to `team`).
///
/// SEAM (CAM): the ui minimap layer on the game side WRITES pings — a
/// minimap click maps panel pixels to a ground `(x, z)` and writes this
/// message with the local player's team (alt-click style filtering is game
/// policy). RENDERING the ping marker (pulse/flare) is future ui-feature
/// work; the base guarantees only the message semantics pinned here.
#[derive(Message, Debug, Clone, Copy)]
pub struct MinimapPing {
    pub pos: Vec2,
    pub team: TeamId,
}

/// Double-tap window for control-group jumps (seconds) — the same
/// interaction-constant class as `input::DOUBLE_CLICK_WINDOW`.
const DOUBLE_TAP_WINDOW: f32 = 0.3;

/// Control-group double-tap bookkeeping: the last plain digit recall
/// (`GroupCommand::Recall` digit + virtual time).
///
/// DETERMINISM (pinned): the clock is `Time::elapsed_secs()` — virtual
/// time, fixed-timestep under test — exactly the `input::ClickTracker`
/// discipline; wall-clock reads are forbidden (KB pitfall). Keyboard taps
/// carry no spatial slop, only this temporal window.
#[derive(Resource, Debug, Default)]
pub struct KeyTapTracker {
    pub last: Option<(u8, f32)>,
}

/// GroupCommand slot index (groups are 1..=9); mirrors the private
/// `selection::group_slot` — kept local so this domain never reaches into
/// selection internals.
fn group_slot(digit: u8) -> Option<usize> {
    (1..=9).contains(&digit).then_some(digit as usize - 1)
}

/// Edge-scroll velocity in WINDOW-PIXEL space (+x right, +y DOWN — cursor
/// coordinates): each axis contributes -/+ `speed_px_per_sec` when the
/// cursor sits inside its `margin_px` dead zone at that border; corners
/// combine both axes. `None` cursor, a center cursor, speed <= 0, or a
/// zero margin all yield `Vec2::ZERO` (the ui-half system multiplies by
/// dt and the camera's world-units-per-pixel).
pub fn edge_scroll_delta(
    margin_px: f32,
    speed_px_per_sec: f32,
    cursor: Option<Vec2>,
    window_px: Vec2,
) -> Vec2 {
    let mut delta = Vec2::ZERO;
    let Some(p) = cursor else { return delta };
    if speed_px_per_sec <= 0.0 || margin_px <= 0.0 {
        return delta;
    }
    if p.x < margin_px {
        delta.x -= speed_px_per_sec;
    }
    if p.x > window_px.x - margin_px {
        delta.x += speed_px_per_sec;
    }
    if p.y < margin_px {
        delta.y -= speed_px_per_sec;
    }
    if p.y > window_px.y - margin_px {
        delta.y += speed_px_per_sec;
    }
    delta
}

/// New ortho scale after `wheel_lines` Line-unit wheel steps at `opts`:
/// multiplicative per step (up = zoom IN = scale shrinks):
/// `current * zoom_step^(-lines)`, clamped to `[zoom_min, zoom_max]` (the
/// caller authors min <= max). `zoom_step <= 0` (the default) returns
/// `current` unchanged — the disabled identity.
pub fn zoomed_scale(current: f32, wheel_lines: f32, opts: &CameraOpts) -> f32 {
    if opts.zoom_step <= 0.0 || wheel_lines == 0.0 {
        return current;
    }
    (current * opts.zoom_step.powf(-wheel_lines)).clamp(opts.zoom_min, opts.zoom_max)
}

/// Same-digit plain recall twice inside [`DOUBLE_TAP_WINDOW`] seconds with
/// a non-empty group -> ONE `CameraJump` at the group's first (slot-vector
/// order — Bind stores the index-sorted selection) alive member's ground
/// position. The fired pair is consumed (a third tap starts a fresh pair).
/// Consuming `Recall` — not raw keys — keeps digit semantics single-sourced
/// in the ui keyboard bridge: a modal-claimed digit recalls nothing, so it
/// never jumps either. Pure message-in/message-out; the window/camera
/// half (`windowed::jump_camera`) consumes the jumps.
fn group_double_tap(
    mut recalls: MessageReader<GroupCommand>,
    mut taps: ResMut<KeyTapTracker>,
    groups: Res<ControlGroups>,
    bodies: Query<&Transform, Without<Dead>>,
    time: Res<Time>,
    mut jumps: MessageWriter<CameraJump>,
) {
    let now = time.elapsed_secs();
    for command in recalls.read() {
        let GroupCommand::Recall(digit) = *command else { continue };
        let Some(slot) = group_slot(digit) else { continue };
        let double =
            matches!(taps.last, Some((d, t)) if d == digit && now - t <= DOUBLE_TAP_WINDOW);
        // A fired pair is consumed; a non-firing tap (re)starts the window.
        taps.last = if double { None } else { Some((digit, now)) };
        if !double {
            continue;
        }
        let to = groups.groups[slot].iter().find_map(|e| {
            bodies
                .get(*e)
                .ok()
                .map(|t| Vec2::new(t.translation.x, t.translation.z))
        });
        if let Some(to) = to {
            jumps.write(CameraJump { to });
        }
    }
}

/// F1 -> jump to the first own-side hero: the lowest Entity index (D6)
/// among `XpHolder + HeroStats` units on the Defender side (the base's
/// single-local-player convention across this workspace's games; a
/// local-player-id resource is the multiplayer seam). The keyboard read is
/// Option-gated: headless apps without a keyboard resource no-op (tests
/// inject `ButtonInput` to drive the seam, same as the input bridges).
fn f1_hero_focus(
    keys: Option<Res<ButtonInput<KeyCode>>>,
    heroes: Query<(Entity, &Transform, &Allegiance), (With<XpHolder>, With<HeroStats>)>,
    mut jumps: MessageWriter<CameraJump>,
) {
    let Some(keys) = keys else { return };
    if !keys.just_pressed(KeyCode::F1) {
        return;
    }
    let mut best: Option<(Entity, Vec2)> = None;
    for (entity, transform, side) in &heroes {
        if *side != Allegiance::Defender {
            continue;
        }
        let pos = Vec2::new(transform.translation.x, transform.translation.z);
        if best
            .as_ref()
            .is_none_or(|(leader, _)| entity.index() < leader.index())
        {
            best = Some((entity, pos));
        }
    }
    if let Some((_, to)) = best {
        jumps.write(CameraJump { to });
    }
}

/// The window/camera-touching half of the domain (`ui` feature only): it
/// reads the primary window / raw wheel messages and mutates the topmost
/// orthographic camera. Every entry no-ops when its operands are absent
/// (headless apps register these under default features and idle safely).
#[cfg(feature = "ui")]
mod windowed {
    use super::{edge_scroll_delta, zoomed_scale, CameraJump, CameraOpts, EdgeScroll};
    use bevy::input::mouse::{MouseScrollUnit, MouseWheel};
    use bevy::prelude::*;
    use bevy::window::PrimaryWindow;

    /// Cursor riding a window edge pans the topmost ortho camera along the
    /// screen direction, converted to world units through the camera's
    /// current world-units-per-pixel (ground AABB of the logical viewport /
    /// its pixel size — the same hole-1 corner-ray math as the input
    /// layer's `refresh_screen_rect`): zooming in therefore slows edge
    /// scroll, matching WC3 feel. Disabled parameters = zero drift.
    pub fn edge_scroll(
        opts: Res<EdgeScroll>,
        time: Res<Time>,
        windows: Query<&Window, With<PrimaryWindow>>,
        mut cams: Query<(Entity, &Camera, &Projection, &GlobalTransform, &mut Transform)>,
    ) {
        if opts.speed_px_per_sec <= 0.0 || opts.margin_px <= 0.0 {
            return; // disabled (the default)
        }
        let Ok(window) = windows.single() else { return };
        let delta_px = edge_scroll_delta(
            opts.margin_px,
            opts.speed_px_per_sec,
            window.cursor_position(),
            Vec2::new(window.width(), window.height()),
        );
        if delta_px == Vec2::ZERO {
            return;
        }
        // Topmost ortho camera (the OrthoTopDownPicker rule: highest
        // Camera::order, ties keep the first).
        let mut top: Option<(isize, Entity)> = None;
        for (entity, cam, projection, _, _) in cams.iter_mut() {
            if !matches!(projection, Projection::Orthographic(_)) {
                continue;
            }
            if top.map_or(true, |(order, _)| cam.order > order) {
                top = Some((cam.order, entity));
            }
        }
        let Some((_, entity)) = top else { return };
        let Ok((_, cam, _, global, mut transform)) = cams.get_mut(entity) else { return };
        let Some(rect) = cam.logical_viewport_rect() else { return };

        // Ground AABB under the viewport (hole-1 XZ convention).
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
            let Ok(ray) = cam.viewport_to_world(global, corner) else { continue };
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
        if !seen {
            return;
        }
        let px = rect.size();
        if px.x <= 0.0 || px.y <= 0.0 {
            return;
        }
        let wpp = Vec2::new((max.x - min.x) / px.x, (max.y - min.y) / px.y);

        // Screen basis on the ground plane: camera local X (screen right)
        // and local Y (screen up), XZ-projected; +y in window space is
        // DOWN, so it subtracts from the up basis.
        let basis = global.affine();
        let right = basis.matrix3.col(0).normalize_or_zero();
        let up = basis.matrix3.col(1).normalize_or_zero();
        let dt = time.delta_secs();
        let mv = Vec2::new(right.x, right.z) * (delta_px.x * wpp.x * dt)
            - Vec2::new(up.x, up.z) * (delta_px.y * wpp.y * dt);
        transform.translation.x += mv.x;
        transform.translation.z += mv.y;
    }

    /// Line-unit wheel steps rescale the topmost ortho camera through
    /// [`zoomed_scale`] (one call on the frame's summed lines). Pixel-unit
    /// (trackpad) scrolls are a documented seam — gesture zoom is future
    /// ui work; discrete notches are the pinned round-2 contract.
    pub fn wheel_zoom(
        opts: Res<CameraOpts>,
        mut wheels: MessageReader<MouseWheel>,
        mut cams: Query<(Entity, &Camera, &mut Projection)>,
    ) {
        if opts.zoom_step <= 0.0 {
            return; // disabled (the default)
        }
        let mut lines = 0.0f32;
        for wheel in wheels.read() {
            if wheel.unit == MouseScrollUnit::Line {
                lines += wheel.y;
            }
        }
        if lines == 0.0 {
            return;
        }
        let mut top: Option<(isize, Entity)> = None;
        for (entity, cam, projection) in cams.iter_mut() {
            if !matches!(*projection, Projection::Orthographic(_)) {
                continue;
            }
            if top.map_or(true, |(order, _)| cam.order > order) {
                top = Some((cam.order, entity));
            }
        }
        let Some((_, entity)) = top else { return };
        let Ok((_, _, mut projection)) = cams.get_mut(entity) else { return };
        if let Projection::Orthographic(ortho) = &mut *projection {
            ortho.scale = zoomed_scale(ortho.scale, lines, &opts);
        }
    }

    /// Consumes `CameraJump`s: recenters the topmost ortho camera's ground
    /// position (x/z) on `to`, KEEPING its height (y). Several jumps in one
    /// frame apply in order — the last wins.
    pub fn jump_camera(
        mut jumps: MessageReader<CameraJump>,
        mut cams: Query<(Entity, &Camera, &Projection, &mut Transform)>,
    ) {
        let mut target: Option<Vec2> = None;
        for jump in jumps.read() {
            target = Some(jump.to);
        }
        let Some(to) = target else { return };
        let mut top: Option<(isize, Entity)> = None;
        for (entity, cam, projection, _) in cams.iter_mut() {
            if !matches!(projection, Projection::Orthographic(_)) {
                continue;
            }
            if top.map_or(true, |(order, _)| cam.order > order) {
                top = Some((cam.order, entity));
            }
        }
        let Some((_, entity)) = top else { return };
        let Ok((_, _, _, mut transform)) = cams.get_mut(entity) else { return };
        transform.translation.x = to.x;
        transform.translation.z = to.y;
    }
}

/// Domain plugin (SEAM2d skeleton; systems landed with the CAM increment,
/// ADR-0018 Phase-1).
pub struct CameraRtsPlugin;

impl Plugin for CameraRtsPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<MinimapPing>()
            .add_message::<CameraJump>()
            .init_resource::<EdgeScroll>()
            .init_resource::<CameraOpts>()
            .init_resource::<KeyTapTracker>()
            // Pure producers: raw-input translation block (before Tick, D2)
            // — they read GroupCommands the ui keyboard bridge wrote this
            // frame (pickup at worst one frame later, inside the window).
            .add_systems(
                Update,
                (group_double_tap, f1_hero_focus).chain().before(WarbaseSet::Tick),
            );
        #[cfg(feature = "ui")]
        {
            // Headless apps with the ui feature have no InputPlugin to
            // register the wheel buffer; registering here keeps the reader
            // param panic-free (double registration is a no-op).
            app.add_message::<bevy::input::mouse::MouseWheel>()
                .add_systems(
                    Update,
                    (
                        windowed::edge_scroll,
                        windowed::wheel_zoom,
                        windowed::jump_camera,
                    )
                        .chain()
                        // After the pure producers so a jump lands on the
                        // same frame it was requested.
                        .after(group_double_tap)
                        .after(f1_hero_focus)
                        .before(WarbaseSet::Tick),
                );
        }
    }
}

pub mod prelude {
    pub use super::{
        edge_scroll_delta, zoomed_scale, CameraJump, CameraOpts, CameraRtsPlugin, EdgeScroll,
        KeyTapTracker, MinimapPing,
    };
}
