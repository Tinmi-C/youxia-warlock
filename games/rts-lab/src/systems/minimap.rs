//! Minimap (RL3, FOG3): consumes the base `MinimapModel` data layer and
//! renders it as a bevy-native panel in the bottom-right corner — map
//! background + one colored square per visible blip (greybox, zero assets).
//! Clicking the panel jumps the camera center to the clicked world point
//! (camera-only; issuing orders from the minimap is out of slice scope).

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

/// Panel size in logical pixels (square).
pub const PANEL_PX: f32 = 176.0;
/// Blip square edge (heroes get double).
const BLIP_PX: f32 = 6.0;

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

/// Click-to-camera: a left press inside the panel rect recenters the ortho
/// camera on the clicked world point (XZ plane). Command issuing from the
/// minimap is deliberately out of scope.
pub fn minimap_click_jumps_camera(world: &mut World) {
    let Some(cursor) = (|| {
        world.iter_entities().find_map(|entity| {
            if entity.get::<PrimaryWindow>().is_none() {
                return None;
            }
            entity.get::<Window>()?.cursor_position()
        })
    })() else {
        return;
    };
    let Some(buttons) = world.get_resource::<ButtonInput<MouseButton>>().cloned() else {
        return;
    };
    if !buttons.just_pressed(MouseButton::Left) {
        return;
    }
    // Panel rect in window pixels (UI GlobalTransform = node CENTER).
    let Some((center, size)) = world
        .query_filtered::<(&GlobalTransform, &ComputedNode), With<MinimapPanel>>()
        .iter(world)
        .next()
        .map(|(tf, node)| (tf.translation().truncate(), node.size * node.inverse_scale_factor()))
    else {
        return;
    };
    let half = size / 2.0;
    let rect_min = center - half;
    let rect_max = center + half;
    if cursor.x < rect_min.x || cursor.x > rect_max.x || cursor.y < rect_min.y || cursor.y > rect_max.y
    {
        return;
    }
    let model = world.resource::<MinimapModel>().clone();
    if model.map_bounds.is_empty() {
        return;
    }
    // Window y is down, panel y is down, world z is panel-y: proportional map.
    let bounds = model.map_bounds;
    let bsize = bounds.size().max(Vec2::ONE * 1e-3);
    let world_x = bounds.min.x + (cursor.x - rect_min.x) / size.x * bsize.x;
    let world_z = bounds.min.y + (cursor.y - rect_min.y) / size.y * bsize.y;
    let mut cams = world.query_filtered::<&mut Transform, With<TopDownCamera>>();
    for mut tf in cams.iter_mut(world) {
        tf.translation.x = world_x;
        tf.translation.z = world_z;
        info!("[rts-lab] minimap: camera -> ({world_x:.1}, {world_z:.1})");
    }
}
