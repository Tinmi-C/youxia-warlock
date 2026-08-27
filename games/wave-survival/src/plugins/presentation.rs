//! PresentationPlugin (cards 12/13): render-world skins for the logical
//! player and monster entities.
//!
//! Added ONLY by build_app() — headless test apps assemble their own plugin set
//! and never see glTF, the asset server, or this module. In test worlds the
//! placeholder cubes stay in place unseen ("nobody looks at them"), which is why
//! every logic regression keeps passing untouched.
//!
//! Mechanic (bevy-spike-validated path):
//!   skinning : find unskinned roots -> spawn a wrapper child carrying
//!              WorldAssetRoot(model scene) + AnimLink + ChildOf(root), then
//!              strip the root's placeholder cube meshes.
//!   binding  : once the model subtree actually exists under the wrapper, bind
//!              the animation graph onto its AnimationPlayer, start clip 0 on
//!              repeat and retint instance materials per MonsterKind.
//!   sync     : mirror WalkCycle.playing (logic side) onto play/pause — review
//!              decision "walk only while actually moving" (monsters spawn with
//!              the flag up because chasing is all they do while Playing).

use std::collections::HashMap;

use bevy::{math::VectorSpace, prelude::*, world_serialization::WorldInstanceReady};

use crate::components::{Monster, MonsterKind, Player, Visual, WalkCycle};

// Assets live under assets/models/ in this project (spike kept them at the root).
const HERO_GLB: &str = "models/hero.glb";
const MONSTER_GLB: &str = "models/monster.glb";
/// The root transform centers on the physics ball (y = 0.5); CesiumMan's origin
/// sits at its feet, so shift the model down half a unit to keep feet on the
/// ground exactly like the placeholder cube was.
const MODEL_Y_OFFSET: f32 = -0.5;
/// Raw CesiumMan height used as the normalizing reference for monster scales.
const MONSTER_MODEL_REF_HEIGHT: f32 = 1.8;
/// How strongly a variant's color is blended over the model's own material
/// (scheme C tint half of the colour+body double coding).
const MODEL_TINT_STRENGTH: f32 = 0.65;

/// Marks the player root as already skinned (idempotence guard).
#[derive(Component)]
struct RoleModel;

/// Marks a monster root as already given a model child (idempotence guard).
#[derive(Component)]
struct MonsterSkinned;

/// Marks a monster wrapper whose subtree finished binding (animation + tint).
#[derive(Component)]
struct MonsterBound;

/// Per-owner animation link carried on the wrapper child; both the hero ready
/// observer and the sync system read it to address graph/node/clip.
#[derive(Component)]
struct AnimLink {
    graph_handle: Handle<AnimationGraph>,
    index: AnimationNodeIndex,
}

/// Memoized tinted material variants, keyed by (source material, kind) so each
/// visual lineage spawns exactly one StandardMaterial clone per kind.
#[derive(Resource, Default)]
struct MonsterSkinCache(HashMap<(AssetId<StandardMaterial>, u8), Handle<StandardMaterial>>);

pub struct PresentationPlugin;

impl Plugin for PresentationPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MonsterSkinCache>()
            .init_resource::<FlashAssets>()
            .add_systems(
                Startup,
                // after spawn_player so the Player entity exists on first run
                skin_player.after(crate::systems::player::spawn_player),
            )
            .add_systems(
                Update,
                // pure render-side decoration; safe in every state; flash
                // application runs after material binding, before sync
                (
                    skin_new_monsters,
                    bind_monster_models,
                    apply_flash_visuals,
                    sync_walk_playback,
                )
                    .chain(),
            );
    }
}

// --- card 12: player -------------------------------------------------------

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
    let link = AnimLink {
        graph_handle: graphs.add(graph),
        index,
    };

    let _wrapper = commands
        .spawn((
            ChildOf(root),
            link,
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
/// subtree's AnimationPlayer and start clip 0 looping (sync will pause it again
/// next frame if the player stands still).
fn on_model_ready(
    ready: On<WorldInstanceReady>,
    mut commands: Commands,
    links: Query<&AnimLink>,
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

// --- card 13: monsters -----------------------------------------------------

/// Give every model-less monster root a monster.glb wrapper child, scaled by
/// its kind (scheme C body language). The placeholder cube is retired from the
/// render world; gameplay data on the root stays untouched.
fn skin_new_monsters(
    mut commands: Commands,
    monsters: Query<(Entity, &MonsterKind), (With<Monster>, Without<MonsterSkinned>)>,
    assets: Res<AssetServer>,
    mut graphs: ResMut<Assets<AnimationGraph>>,
    // one shared graph pair for every monster (asset server dedups the clip)
    mut cached: Local<Option<(Handle<AnimationGraph>, AnimationNodeIndex)>>,
) {
    if monsters.is_empty() {
        return;
    }
    // resolve the shared graph lazily so an empty field costs nothing
    let (graph_handle, index) = match cached.as_ref() {
        Some(pair) => (pair.0.clone(), pair.1),
        None => {
            let (graph, index) = AnimationGraph::from_clip(
                assets.load(GltfAssetLabel::Animation(0).from_asset(MONSTER_GLB)),
            );
            let handle = graphs.add(graph);
            *cached = Some((handle.clone(), index));
            (handle, index)
        }
    };

    for (root, kind) in monsters.iter() {
        let scale = kind.cube_size() / MONSTER_MODEL_REF_HEIGHT * kind.visual_scale();
        let _wrapper = commands
            .spawn((
                ChildOf(root),
                AnimLink {
                    graph_handle: graph_handle.clone(),
                    index,
                },
                WorldAssetRoot(assets.load(GltfAssetLabel::Scene(0).from_asset(MONSTER_GLB))),
                Transform {
                    translation: Vec3::new(0.0, MODEL_Y_OFFSET, 0.0),
                    scale: Vec3::splat(scale),
                    ..default()
                },
            ))
            .observe(on_monster_model_ready)
            .id();
        info!(
            "[presentation] skinning {kind:?} root {root:?} with {MONSTER_GLB} (scale {scale:.2})"
        );
        commands.entity(root).insert(MonsterSkinned);
    }
}

/// Wait for the wrapper's WorldAssetRoot load signal, then hand off to the
/// generic finisher by simply marking nothing here — the finisher polls for the
/// spawned subtree instead of racing asset events (robust across load timings).
fn on_monster_model_ready(_ready: On<WorldInstanceReady>) {
    // intentionally empty: binding happens in bind_monster_models (polled), so
    // late or early loads both converge without event-ordering races.
}

/// Once a monster's model subtree exists under its wrapper: bind the walk-cycle
/// AnimationPlayer, swap mesh materials for tinted per-kind instances, then pin
/// the wrapper as bound.
fn bind_monster_models(
    mut commands: Commands,
    wrappers: Query<(Entity, &ChildOf, &AnimLink), Without<MonsterBound>>,
    children: Query<&Children>,
    kinds: Query<&MonsterKind>,
    mut players: Query<&mut AnimationPlayer>,
    skinned_materials: Query<&MeshMaterial3d<StandardMaterial>>,
    meshes: Query<(), With<Mesh3d>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut cache: ResMut<MonsterSkinCache>,
) {
    for (wrapper, parent, link) in &wrappers {
        // the scene children only exist once the glTF instance finished spawning
        let Ok(_subtree) = children.get(wrapper) else {
            continue;
        };
        let Ok(kind) = kinds.get(parent.0) else {
            continue;
        };
        let root = parent.0;

        for node in children.iter_descendants(wrapper) {
            if let Ok(mut player) = players.get_mut(node) {
                player.play(link.index).repeat();
                commands
                    .entity(node)
                    .insert((AnimationGraphHandle(link.graph_handle.clone()),));
            }
            // tint only real mesh entities that carry their own material slot
            if meshes.contains(node) {
                if let Ok(slot) = skinned_materials.get(node) {
                    let tinted = tinted_material_for(&mut materials, &mut cache.0, &slot.0, *kind);
                    commands.entity(node).insert(MeshMaterial3d(tinted));
                }
            }
        }
        commands.entity(wrapper).insert(MonsterBound);
        info!("[presentation] monster model bound on {root:?} ({kind:?}, walk loop)");
    }
}

/// Clone-and-retint one material lineage once per (source handle, kind):
/// blends the source base color toward the kind's scheme-C color.
fn tinted_material_for(
    assets: &mut Assets<StandardMaterial>,
    cache: &mut HashMap<(AssetId<StandardMaterial>, u8), Handle<StandardMaterial>>,
    source: &Handle<StandardMaterial>,
    kind: MonsterKind,
) -> Handle<StandardMaterial> {
    let key = (source.id(), kind_ordinal(kind));
    if let Some(existing) = cache.get(&key) {
        return existing.clone();
    }
    let mut variant = match assets.get(source) {
        Some(material) => material.clone(),
        None => StandardMaterial::default(),
    };
    variant.base_color = variant.base_color.mix(&kind.color(), MODEL_TINT_STRENGTH);
    let handle = assets.add(variant);
    cache.insert(key, handle.clone());
    handle
}

/// Stable u8 key for the memo cache.
fn kind_ordinal(kind: MonsterKind) -> u8 {
    match kind {
        MonsterKind::Grunt => 0,
        MonsterKind::Runner => 1,
        MonsterKind::Tank => 2,
    }
}

// --- shared playback sync --------------------------------------------------

/// Mirror WalkCycle.playing onto each owner's model subtree every frame —
/// works uniformly for the player (movement-derived) and monsters (spawned up,
/// cleared outside Playing by logic-side clear_walk_on_pause).
fn sync_walk_playback(
    roots: Query<(Entity, &WalkCycle, &Children)>,
    links: Query<&AnimLink>,
    children: Query<&Children>,
    mut players: Query<&mut AnimationPlayer>,
) {
    for (_root, walk, owner_children) in &roots {
        // find the wrapper child carrying the animation link
        let mut found = None;
        for kid in owner_children.iter() {
            if let Ok(link) = links.get(kid) {
                found = Some((kid, link.index));
            }
        }
        let Some((wrapper, index)) = found else {
            continue;
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
}

// --- card 14: hit-flash visuals --------------------------------------------

/// Lazily-created private material clones per owner, so emissive flash edits
/// never bleed between entities that share a (per-kind) base material. Entries
/// live for the session; a despawned owner leaves only a few orphaned assets —
/// bounded by the count of entities that ever flashed this run.
#[derive(Resource, Default)]
struct FlashAssets {
    privates: HashMap<Entity, Vec<Handle<StandardMaterial>>>,
}

/// Card 14 (presentation half): mirror each owner's Visual.flash onto its model
/// materials as an emissive whiteout. The logic side owns the number; here we
/// only paint it — headless worlds stay untouched by construction.
fn apply_flash_visuals(
    mut commands: Commands,
    holders: Query<(Entity, &Visual)>,
    children: Query<&Children>,
    slots: Query<&MeshMaterial3d<StandardMaterial>>,
    meshes: Query<(), With<Mesh3d>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut flash: ResMut<FlashAssets>,
) {
    for (root, vis) in &holders {
        // Privatize on first sight: clone current mesh-material handles into
        // per-owner instances and swap them in. Re-runs harmlessly until at
        // least one slot exists (models load a few frames after spawn).
        let entry = flash.privates.entry(root).or_default();
        if entry.is_empty() {
            let mut collected: Vec<Handle<StandardMaterial>> = Vec::new();
            for node in children.iter_descendants(root) {
                if meshes.contains(node) {
                    if let Ok(slot) = slots.get(node) {
                        let mut variant = match materials.get(&slot.0) {
                            Some(material) => material.clone(),
                            None => StandardMaterial::default(),
                        };
                        variant.emissive = variant
                            .base_color
                            .to_linear()
                            .lerp(LinearRgba::WHITE, vis.flash);
                        let handle = materials.add(variant);
                        commands.entity(node).insert(MeshMaterial3d(handle.clone()));
                        collected.push(handle);
                    }
                }
            }
            *entry = collected;
            continue; // first pass already paints via the clones' initial state
        }
        // steady state: repaint private instances in place — no allocations,
        // and other entities sharing the pre-clone lineage are never touched.
        for handle in entry {
            if let Some(mut material) = materials.get_mut(handle) {
                material.emissive = material
                    .base_color
                    .to_linear()
                    .lerp(LinearRgba::WHITE, vis.flash);
            }
        }
    }
}
