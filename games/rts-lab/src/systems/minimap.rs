//! Minimap (RL3, FOG3 + round-3 MMP): consumes the base `MinimapModel` data
//! layer and renders it as a bevy-native panel in the bottom-right corner —
//! map background + one colored square per visible blip (greybox, zero
//! assets). MMP adds the interaction + alert layer:
//! - left click jumps the camera center to the clicked world point;
//! - Alt + left click writes a base `MinimapPing` (NO camera jump) —
//!   [`spawn_minimap_pings`]/[`animate_minimap_pings`] draw it as a
//!   team-colored expanding ring, two 1.5s waves, 3s total;
//! - right click with a live selection issues `UnitCommand::Move` to the
//!   clicked world point (same ground-move semantics as the main-viewport
//!   bridge's right click); with an empty selection it just jumps the
//!   camera.
//! Plus the shared CJK UI-font wiring for the base tooltip seam
//! ([`setup_shared_ui_font`]).

use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use warbase::prelude::*;

use crate::systems::camera::TopDownCamera;

/// The minimap panel root node.
#[derive(Component, Debug)]
pub struct MinimapPanel;

/// One spawned blip square (rebuilt every frame; counts are tiny).
#[derive(Component, Debug)]
pub struct MinimapBlipNode;

/// A live ping ripple (MMP): an expanding square ring at the ping's panel
/// point, team-colored, two [`PING_WAVE_SECS`] waves ([`PING_LIFETIME_SECS`]
/// total) then despawned. `at` is the virtual `Time::elapsed_secs()` the
/// ping landed (the input-domain determinism discipline — never wall clock).
#[derive(Component, Debug)]
pub struct MinimapPingRipple {
    pub at: f32,
    pub team: TeamId,
    /// Panel-space ring center (px from the panel's top-left).
    pub center: Vec2,
}

/// Panel size in logical pixels (square).
pub const PANEL_PX: f32 = 176.0;
/// Blip square edge (heroes get double).
const BLIP_PX: f32 = 6.0;

/// MMP ripple timing: two 1.5s expanding-ring waves = 3.0s total marker
/// lifetime (round-3 plan §Phase-1 MMP row).
pub const PING_WAVE_SECS: f32 = 1.5;
pub const PING_WAVES: u32 = 2;
pub const PING_LIFETIME_SECS: f32 = PING_WAVE_SECS * PING_WAVES as f32;
/// Ring growth range, panel px (about a third of the panel at max).
const RING_START_PX: f32 = 10.0;
const RING_MAX_PX: f32 = 56.0;
/// Ring stroke width, px.
const RING_STROKE_PX: f32 = 2.0;

/// The local player's team (MMP ping writer/reader convention). rts-lab is
/// single-local-player; `unit::Hostility` maps `Allegiance::Defender ->
/// TeamId(0)` — the base's documented single-local-player convention (see
/// `f1_hero_focus`, which picks own-side heroes the same way).
pub const LOCAL_TEAM: TeamId = TeamId(0);

/// Ripple size (edge px) and alpha at `age` seconds since the ping landed:
/// `None` once past the lifetime. Each wave grows the ring
/// [`RING_START_PX`]..[`RING_MAX_PX`] while fading 1.0..0. Pure math — the
/// animation system and the domain tests share it.
pub fn ripple_at(age: f32) -> Option<(f32, f32)> {
    if age < 0.0 || age >= PING_LIFETIME_SECS {
        return None;
    }
    let p = (age % PING_WAVE_SECS) / PING_WAVE_SECS;
    Some((RING_START_PX + (RING_MAX_PX - RING_START_PX) * p, 1.0 - p))
}

/// Team color of a ping ripple, read from the live `UiTheme` (MMP spec:
/// own gold / enemy red — the theme's `state_colors.gold` / `.enemy`, so a
/// re-skin recolors pings with zero code change).
pub fn ping_color(team: TeamId, theme: &warbase::ui::UiTheme) -> Color {
    if team == LOCAL_TEAM {
        theme.state_colors.gold.color()
    } else {
        theme.state_colors.enemy.color()
    }
}

pub fn spawn_minimap(mut commands: Commands) {
    commands.spawn((
        MinimapPanel,
        Node {
            position_type: PositionType::Absolute,
            right: Val::Px(12.0),
            bottom: Val::Px(12.0),
            width: Val::Px(PANEL_PX),
            height: Val::Px(PANEL_PX),
            border: UiRect::all(Val::Px(2.0)),
            ..default()
        },
        BackgroundColor(Color::srgb(0.07, 0.09, 0.07)),
        BorderColor::all(Color::srgb(0.4, 0.5, 0.4)),
    ));
}

/// Project a world-plane point (x, z) into panel-local pixels.
fn to_panel(pos: Vec2, bounds: &Rect) -> Vec2 {
    let size = bounds.size().max(Vec2::ONE * 1e-3); // degenerate guard
    Vec2::new(
        (pos.x - bounds.min.x) / size.x * PANEL_PX,
        (pos.y - bounds.min.y) / size.y * PANEL_PX,
    )
}

/// Rebuild the blip squares from `MinimapModel` (deterministic blip order —
/// the base sorts by entity index, D6). Invisible units leave no blip.
pub fn refresh_minimap_view(
    model: Res<MinimapModel>,
    mut commands: Commands,
    panel: Query<Entity, With<MinimapPanel>>,
    blips: Query<Entity, With<MinimapBlipNode>>,
) {
    let Ok(panel) = panel.single() else { return };
    for old in &blips {
        commands.entity(old).despawn();
    }
    if model.map_bounds.is_empty() {
        return; // no vision geometry: empty panel
    }
    let bounds = model.map_bounds;
    for blip in &model.blips {
        let p = to_panel(blip.pos, &bounds);
        let edge = if blip.is_hero { BLIP_PX * 2.0 } else { BLIP_PX };
        let color = match blip.side {
            Allegiance::Defender => Color::srgb(0.30, 0.75, 1.00),
            Allegiance::Attacker => Color::srgb(1.00, 0.25, 0.20),
        };
        let color = if blip.is_hero { Color::srgb(1.0, 0.85, 0.3) } else { color };
        commands.spawn((
            MinimapBlipNode,
            ChildOf(panel),
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px((p.x - edge / 2.0).max(0.0)),
                top: Val::Px((p.y - edge / 2.0).max(0.0)),
                width: Val::Px(edge),
                height: Val::Px(edge),
                ..default()
            },
            BackgroundColor(color),
        ));
    }
}

/// Consume `MinimapPing` messages (MMP): each becomes one ripple node on the
/// panel, positioned through the same bounds projection the blips use. No
/// panel or no vision geometry => the message is dropped (nothing to draw
/// on); message semantics stay owned by the base camera domain.
pub fn spawn_minimap_pings(
    mut commands: Commands,
    mut pings: MessageReader<MinimapPing>,
    model: Res<MinimapModel>,
    panel: Query<Entity, With<MinimapPanel>>,
    theme: Res<warbase::ui::UiTheme>,
    time: Res<Time>,
) {
    if model.map_bounds.is_empty() {
        return;
    }
    let Ok(panel) = panel.single() else { return };
    let now = time.elapsed_secs();
    for ping in pings.read() {
        let center = to_panel(ping.pos, &model.map_bounds);
        commands.spawn((
            MinimapPingRipple { at: now, team: ping.team, center },
            ChildOf(panel),
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px((center.x - RING_START_PX / 2.0).max(0.0)),
                top: Val::Px((center.y - RING_START_PX / 2.0).max(0.0)),
                width: Val::Px(RING_START_PX),
                height: Val::Px(RING_START_PX),
                border: UiRect::all(Val::Px(RING_STROKE_PX)),
                ..default()
            },
            BorderColor::all(ping_color(ping.team, &theme)),
            BackgroundColor(Color::NONE),
        ));
    }
}

/// Advance every live ripple: ring size/position and fade alpha recomputed
/// from virtual age each frame (theme re-read, so skins recolor live pings);
/// ripples past [`PING_LIFETIME_SECS`] despawn.
pub fn animate_minimap_pings(
    mut commands: Commands,
    mut ripples: Query<(Entity, &MinimapPingRipple, &mut Node, &mut BorderColor)>,
    theme: Res<warbase::ui::UiTheme>,
    time: Res<Time>,
) {
    let now = time.elapsed_secs();
    for (entity, ripple, mut node, mut border) in &mut ripples {
        let Some((size, alpha)) = ripple_at(now - ripple.at) else {
            commands.entity(entity).despawn();
            continue;
        };
        *border = BorderColor::all(ping_color(ripple.team, &theme).with_alpha(alpha));
        node.width = Val::Px(size);
        node.height = Val::Px(size);
        node.left = Val::Px((ripple.center.x - size / 2.0).max(0.0));
        node.top = Val::Px((ripple.center.y - size / 2.0).max(0.0));
    }
}

/// Shared click geometry (extracted from the original jump system so all
/// three click flavors share ONE rect/mapping math): cursor + panel rect +
/// live `MinimapModel` bounds -> the clicked world point (x, z). None when
/// the cursor is off the panel or the map has no vision geometry.
fn panel_world_point(world: &mut World) -> Option<Vec2> {
    let cursor = world.iter_entities().find_map(|entity| {
        if entity.get::<PrimaryWindow>().is_none() {
            return None;
        }
        entity.get::<Window>()?.cursor_position()
    })?;
    // Panel rect in window pixels (UI GlobalTransform = node CENTER).
    let (center, size) = world
        .query_filtered::<(&GlobalTransform, &ComputedNode), With<MinimapPanel>>()
        .iter(world)
        .next()
        .map(|(tf, node)| (tf.translation().truncate(), node.size * node.inverse_scale_factor()))?;
    let half = size / 2.0;
    let rect_min = center - half;
    let rect_max = center + half;
    if cursor.x < rect_min.x || cursor.x > rect_max.x || cursor.y < rect_min.y || cursor.y > rect_max.y
    {
        return None;
    }
    let bounds = world.resource::<MinimapModel>().map_bounds;
    if bounds.is_empty() {
        return None;
    }
    // Window y is down, panel y is down, world z is panel-y: proportional map.
    let bsize = bounds.size().max(Vec2::ONE * 1e-3);
    Some(Vec2::new(
        bounds.min.x + (cursor.x - rect_min.x) / size.x * bsize.x,
        bounds.min.y + (cursor.y - rect_min.y) / size.y * bsize.y,
    ))
}

/// Recenter the ortho camera on a ground point (the original click-to-camera
/// behavior, XZ plane, height kept).
fn jump_camera_to(world: &mut World, to: Vec2) {
    let mut cams = world.query_filtered::<&mut Transform, With<TopDownCamera>>();
    for mut tf in cams.iter_mut(world) {
        tf.translation.x = to.x;
        tf.translation.z = to.y;
    }
}

/// Minimap interaction router (MMP): one exclusive system owns every panel
/// click flavor so their semantics can never drift apart —
/// - Alt + left press -> ONE `MinimapPing` at the clicked world point,
///   attributed to [`LOCAL_TEAM`]; the camera does NOT move;
/// - plain left press -> camera jump to the clicked world point (legacy
///   RL3 behavior, unchanged), no ping;
/// - right press -> `UnitCommand::Move` for the whole live selection (the
///   same ground-move semantics as the main-viewport bridge), camera stays;
///   with an empty selection it falls back to a camera jump.
pub fn minimap_clicks(world: &mut World) {
    let Some(buttons) = world.get_resource::<ButtonInput<MouseButton>>().cloned() else {
        return;
    };
    let (left, right) = (
        buttons.just_pressed(MouseButton::Left),
        buttons.just_pressed(MouseButton::Right),
    );
    if !left && !right {
        return;
    }
    let alt = world
        .get_resource::<ButtonInput<KeyCode>>()
        .is_some_and(|keys| keys.any_pressed([KeyCode::AltLeft, KeyCode::AltRight]));
    let Some(point) = panel_world_point(world) else {
        return;
    };
    if right {
        let selected = world.resource::<SelectionState>().entities.clone();
        if selected.is_empty() {
            jump_camera_to(world, point);
            info!("[rts-lab] minimap: camera -> ({:.1}, {:.1})", point.x, point.y);
        } else {
            // Same ground-move semantics as the main-viewport right click
            // (input_bridge: open ground -> Move on the y=0 plane).
            world.write_message(UnitCommand {
                order: Order::Move { to: Vec3::new(point.x, 0.0, point.y) },
                subjects: selected,
            });
            info!("[rts-lab] minimap: move order -> ({:.1}, {:.1})", point.x, point.y);
        }
        return;
    }
    if alt {
        world.write_message(MinimapPing { pos: point, team: LOCAL_TEAM });
    } else {
        jump_camera_to(world, point);
        info!("[rts-lab] minimap: camera -> ({:.1}, {:.1})", point.x, point.y);
    }
}

/// Shared CJK UI font wiring (MMP tooltip CJK): the base `warbase::ui`
/// tooltip renders through the `warbase::ui::UiFont` resource — `None` (the
/// default) keeps bevy's default font, zero drift. This loads the shared
/// Noto Sans SC ONCE from the workspace-root `assets/fonts/` (the same
/// single-copy pattern as warden/loop-td `setup_ui_font`) and hands the
/// handle to the base seam. Bare headless (no `Assets<Font>`) no-ops —
/// nothing to add into, the seam stays default.
pub fn setup_shared_ui_font(
    mut commands: Commands,
    fonts: Option<ResMut<Assets<Font>>>,
) {
    let Some(mut fonts) = fonts else { return };
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../assets/fonts/NotoSansSC-VariableFont_wght.ttf");
    match std::fs::read(&path) {
        Ok(bytes) => {
            commands.insert_resource(warbase::ui::UiFont(Some(fonts.add(Font::from_bytes(bytes)))));
            info!("[rts-lab] shared CJK ui font wired (base tooltip seam)");
        }
        Err(e) => {
            info!("[rts-lab] shared ui font unreadable ({e}) — default fallback");
        }
    }
}
