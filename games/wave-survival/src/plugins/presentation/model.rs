//! Presentation domain — model skinning (player + monsters) and per-kind tint.
//! Cards 12 / 13 / 19 / 21 / 22.

use std::collections::HashMap;
use std::time::Duration;

use bevy::{animation::RepeatAnimation, prelude::*, world_serialization::WorldInstanceReady};

use crate::components::{Monster, MonsterKind, Player};
use crate::systems::player::spawn_player;

use super::anim_runtime::{AnimLink, FxWatch};
use super::feel::FeelState;
use super::PresentationSet;

// --- constants -------------------------------------------------------------

/// Player model (card 12 -> 21: player_hunyuan, a 5-clip graph).
pub const HERO_GLB: &str = "models/player_hunyuan.glb";
/// Hero clip indices (alphabetical order inside the glb; pinned by a test).
pub const HERO_CLIP_ATTACK: usize = 0;
pub const HERO_CLIP_HIT: usize = 1;
pub const HERO_CLIP_IDLE: usize = 2;
pub const HERO_CLIP_RUN: usize = 3;
pub const HERO_CLIP_WALK: usize = 4;
/// World-height parity: the authored hero is 1.33 m, scaled to ~1.80 m.
pub const HERO_SCALE: f32 = 1.353;
/// Vertical offset that plants the model's feet on the ground (root is at y=0).
pub(crate) const MODEL_Y_OFFSET: f32 = -0.5;
/// How strongly a kind's scheme-C color is blended over its source material.
const MODEL_TINT_STRENGTH: f32 = 0.65;

// --- components / resources ------------------------------------------------

/// Marker on a skinned player root (so skin_player can skip already-skin ones).
#[derive(Component)]
pub(crate) struct RoleModel;

/// Marker: the monster root has been given a model wrapper (card 13 / 19).
#[derive(Component)]
struct MonsterSkinned;

/// Marker: the monster's model subtree has been bound (materials+animation).
#[derive(Component)]
struct MonsterBound;

/// Memo cache of (source material, kind) -> tinted instance, so a kind's
/// lineage is cloned-and-retinted once, not per frame.
#[derive(Resource, Default)]
pub(crate) struct MonsterSkinCache(
    HashMap<(AssetId<StandardMaterial>, u8), Handle<StandardMaterial>>,
);

/// Domain plugin: skin + bind models.
pub struct ModelPlugin;

impl Plugin for ModelPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MonsterSkinCache>()
            .add_systems(Startup, skin_player.after(spawn_player))
            .add_systems(
                Update,
                (skin_new_monsters, bind_monster_models)
                    .chain()
                    .in_set(PresentationSet::Model),
            );
    }
}

// --- card 12: player -------------------------------------------------------

/// Attach the player_hunyuan.glb model to the player root and retire the
/// placeholder cube visuals from the render world (card 21: 4-clip graph —
/// walk/idle state machine + attack/hit one-shots). Logic components are never
/// touched: Player/Hp/Transform/rigidbody/collider all stay exactly as spawned.
fn skin_player(
    mut commands: Commands,
    players: Query<Entity, (With<Player>, Without<RoleModel>)>,
    assets: Res<AssetServer>,
    mut graphs: ResMut<Assets<AnimationGraph>>,
) {
    let Ok(root) = players.single() else {
        return;
    };
    info!("[presentation] skinning player with {HERO_GLB} (5-clip graph)");

    let clip = |i: usize| assets.load(GltfAssetLabel::Animation(i).from_asset(HERO_GLB));
    let mut graph = AnimationGraph::new();
    let attack = graph.add_clip(clip(HERO_CLIP_ATTACK), 1.0, graph.root);
    let hit = graph.add_clip(clip(HERO_CLIP_HIT), 1.0, graph.root);
    let idle = graph.add_clip(clip(HERO_CLIP_IDLE), 1.0, graph.root);
    let run = graph.add_clip(clip(HERO_CLIP_RUN), 1.0, graph.root);
    let walk = graph.add_clip(clip(HERO_CLIP_WALK), 1.0, graph.root);
    let link = AnimLink {
        graph_handle: graphs.add(graph),
        walk,
        run: Some(run),
        idle: Some(idle),
        attack: Some(attack),
        hit: Some(hit),
        current: None, // force the first state command after binding
        one_shot: None,
        pending: None,
    };

    let _wrapper = commands
        .spawn((
            ChildOf(root),
            link,
            WorldAssetRoot(assets.load(GltfAssetLabel::Scene(0).from_asset(HERO_GLB))),
            FeelState::default(),
            Transform {
                translation: Vec3::new(0.0, MODEL_Y_OFFSET, 0.0),
                scale: Vec3::splat(HERO_SCALE),
                ..default()
            },
        ))
        .observe(on_model_ready)
        .id();

    commands
        .entity(root)
        .insert((
            RoleModel,
            FxWatch {
                last_cooldown: 0.0,
                last_flash: 0.0,
            },
        ))
        // replace the placeholder visuals, not hide them (root Visibility would
        // cascade down onto the model subtree)
        .remove::<Mesh3d>()
        .remove::<MeshMaterial3d<StandardMaterial>>();
}

/// Model subtree landed under the wrapper: bind the 4-clip animation graph and
/// an AnimationTransitions onto the subtree's AnimationPlayer (card 21). The
/// first sync frame commands walk/idle from scratch (`current: None`), so no
/// clip is started here — that keeps one code path for all playback decisions.
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
            // start idle looping so the very first frames after load have a
            // clip running even before sync issues its state command
            if let Some(idle) = link.idle {
                let mut transitions = AnimationTransitions::new();
                transitions
                    .play(&mut player, idle, Duration::ZERO)
                    .set_repeat(RepeatAnimation::Forever);
                commands
                    .entity(node)
                    .insert((AnimationGraphHandle(link.graph_handle.clone()), transitions));
                info!("[presentation] hero 5-clip graph bound to {node:?} (idle initial)");
            } else {
                // legacy single-clip owners (defensive: monsters never take
                // this observer, but keep the old path compiling)
                player.play(link.walk).repeat();
                commands
                    .entity(node)
                    .insert((AnimationGraphHandle(link.graph_handle.clone()),));
            }
        }
    }
}

// --- card 13: monsters -----------------------------------------------------

/// Give every model-less monster root its kind's model wrapper (card 19 enemy
/// definition table: model file + wrapper scale + walk-clip index live on
/// `MonsterKind`; card 22 adds the attack/hit clips). The placeholder cube is
/// retired from the render world; gameplay data on the root stays untouched.
fn skin_new_monsters(
    mut commands: Commands,
    monsters: Query<(Entity, &MonsterKind), (With<Monster>, Without<MonsterSkinned>)>,
    assets: Res<AssetServer>,
    mut graphs: ResMut<Assets<AnimationGraph>>,
    // one graph per kind (each model carries its own clip set/indices)
    mut cached: Local<
        HashMap<
            MonsterKind,
            (
                Handle<AnimationGraph>,
                AnimationNodeIndex,
                AnimationNodeIndex,
                AnimationNodeIndex,
            ),
        >,
    >,
) {
    if monsters.is_empty() {
        return;
    }

    for (root, kind) in monsters.iter() {
        let model = kind.model();
        // resolve the per-kind graph lazily so an empty map costs nothing
        let (graph_handle, walk, attack, hit) = match cached.get(kind) {
            Some(quadruple) => (quadruple.0.clone(), quadruple.1, quadruple.2, quadruple.3),
            None => {
                let clip = |i: usize| assets.load(GltfAssetLabel::Animation(i).from_asset(model));
                let mut graph = AnimationGraph::new();
                let walk = graph.add_clip(clip(kind.walk_clip()), 1.0, graph.root);
                let attack = graph.add_clip(clip(kind.attack_clip()), 1.0, graph.root);
                let hit = graph.add_clip(clip(kind.hit_clip()), 1.0, graph.root);
                let handle = graphs.add(graph);
                cached.insert(*kind, (handle.clone(), walk, attack, hit));
                (handle, walk, attack, hit)
            }
        };

        let scale = kind.wrapper_scale();
        let _wrapper = commands
            .spawn((
                ChildOf(root),
                AnimLink {
                    graph_handle: graph_handle.clone(),
                    walk,
                    run: None,
                    idle: None,
                    attack: Some(attack),
                    hit: Some(hit),
                    current: None,
                    one_shot: None,
                    pending: None,
                },
                WorldAssetRoot(assets.load(GltfAssetLabel::Scene(0).from_asset(model))),
                FeelState::default(),
                Transform {
                    translation: Vec3::new(0.0, MODEL_Y_OFFSET, 0.0),
                    scale: Vec3::splat(scale),
                    ..default()
                },
            ))
            .observe(on_monster_model_ready)
            .id();
        info!("[presentation] skinning {kind:?} root {root:?} with {model} (scale {scale:.2})");
        commands
            .entity(root)
            .insert(FxWatch::default())
            // visual-pass fix #3: strip the placeholder cube the same way the
            // player root does — the model IS the body now
            .insert(MonsterSkinned)
            .remove::<Mesh3d>()
            .remove::<MeshMaterial3d<StandardMaterial>>();
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
                // card 22: monsters join the transitions world too (one-shot
                // combat clips share the machinery); walk starts immediately
                let mut transitions = AnimationTransitions::new();
                transitions
                    .play(&mut player, link.walk, Duration::ZERO)
                    .set_repeat(RepeatAnimation::Forever);
                commands
                    .entity(node)
                    .insert((AnimationGraphHandle(link.graph_handle.clone()), transitions));
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
        info!("[presentation] monster model bound on {root:?} ({kind:?}, walk+combat clips)");
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
        MonsterKind::Elite => 3,
    }
}
