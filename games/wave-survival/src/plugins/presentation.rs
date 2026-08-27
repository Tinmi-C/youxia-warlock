//! PresentationPlugin (capability card 12 HeroPresentation): render-world skin
//! for the logical player entity.
//!
//! Added ONLY by build_app() — headless test apps assemble their own plugin set
//! and never see glTF, the asset server, or this module. In test worlds the
//! placeholder cube stays in place unseen ("nobody looks at it"), which is why
//! every logic regression keeps passing untouched.
//!
//! Mechanic (bevy-spike-validated path):
//!   Startup: find the player root -> spawn a wrapper child carrying
//!            WorldAssetRoot(hero.glb scene) + the animation link + ChildOf(root),
//!            then strip the root's placeholder cube meshes.
//!   On<WorldInstanceReady>: locate AnimationPlayer inside the model subtree,
//!            bind the graph, play clip 0 on repeat.
//!   Update : mirror WalkCycle.playing (logic side) onto play/pause — review
//!            decision "walk only while actually moving".

use bevy::{prelude::*, world_serialization::WorldInstanceReady};

use crate::components::{Player, WalkCycle};

const HERO_GLB: &str = "hero.glb";
/// The root transform centers on the physics ball (y = 0.5); CesiumMan's origin
/// sits at its feet, so shift the model down half a unit to keep feet on the
/// ground exactly like the placeholder cube was.
const MODEL_Y_OFFSET: f32 = -0.5;

/// Marks the player root as already skinned (idempotence guard).
#[derive(Component)]
struct RoleModel;

/// Per-player animation link carried on the wrapper child; both the ready
/// observer and the sync system read it to address graph/node/clip.
#[derive(Component)]
struct HeroAnim {
    graph_handle: Handle<AnimationGraph>,
    index: AnimationNodeIndex,
}

pub struct PresentationPlugin;

impl Plugin for PresentationPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Startup,
            // after spawn_player so the Player entity exists on first run
            skin_player.after(crate::systems::player::spawn_player),
        )
        .add_systems(Update, sync_walk_playback);
    }
}

/// Attach the hero.glb model to the player root and retire the placeholder
/// cube visuals from the render world. Logic components are never touched:
/// Player/Hp/Transform/rigidbody/collider all stay exactly as spawned.
fn skin_player(
    mut commands: Commands,
    players: Query<Entity, (With<Player>, Without<RoleModel>)>,
    assets: Res<AssetServer>,
    mut graphs: ResMut<Assets<AnimationGraph>>,
) {
    let Ok(root) = players.single() else {
        return;
    };
    info!("[presentation] skinning player with {HERO_GLB}");

    let (graph, index) =
        AnimationGraph::from_clip(assets.load(GltfAssetLabel::Animation(0).from_asset(HERO_GLB)));
    let hero_anim = HeroAnim {
        graph_handle: graphs.add(graph),
        index,
    };

    let _wrapper = commands
        .spawn((
            ChildOf(root),
            hero_anim,
            WorldAssetRoot(assets.load(GltfAssetLabel::Scene(0).from_asset(HERO_GLB))),
            Transform::from_xyz(0.0, MODEL_Y_OFFSET, 0.0),
        ))
        .observe(on_model_ready)
        .id();

    commands
        .entity(root)
        .insert(RoleModel)
        // replace the placeholder visuals, not hide them (root Visibility would
        // cascade down onto the model subtree)
        .remove::<Mesh3d>()
        .remove::<MeshMaterial3d<StandardMaterial>>();
}

/// Model subtree landed under the wrapper: bind the animation graph onto the
/// subtree's AnimationPlayer and start clip 0 looping (sync system will pause
/// it again next frame if the player stands still).
fn on_model_ready(
    ready: On<WorldInstanceReady>,
    mut commands: Commands,
    links: Query<&HeroAnim>,
    children: Query<&Children>,
    mut players: Query<&mut AnimationPlayer>,
) {
    let Ok(link) = links.get(ready.entity) else {
        return;
    };
    for node in children.iter_descendants(ready.entity) {
        if let Ok(mut player) = players.get_mut(node) {
            player.play(link.index).repeat();
            commands
                .entity(node)
                .insert((AnimationGraphHandle(link.graph_handle.clone()),));
            info!("[presentation] hero walk-cycle bound to {node:?} (clip 0, repeat)");
        }
    }
}

/// Mirror WalkCycle.playing onto the subtree's AnimationPlayer each frame.
fn sync_walk_playback(
    roots: Query<(Entity, &WalkCycle, &Children), With<Player>>,
    links: Query<&HeroAnim>,
    children: Query<&Children>,
    mut players: Query<&mut AnimationPlayer>,
) {
    let Ok((_root, walk, player_children)) = roots.single() else {
        return;
    };
    // find the wrapper child carrying the animation link
    let mut found = None;
    for kid in player_children.iter() {
        if let Ok(link) = links.get(kid) {
            found = Some((kid, link.index));
        }
    }
    let Some((wrapper, index)) = found else {
        return;
    };
    for node in children.iter_descendants(wrapper) {
        if let Ok(mut player) = players.get_mut(node) {
            if let Some(active) = player.animation_mut(index) {
                if walk.playing && active.is_paused() {
                    active.resume();
                } else if !walk.playing && !active.is_paused() {
                    active.pause();
                }
            }
        }
    }
}
