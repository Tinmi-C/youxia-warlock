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
//!              the animation graph onto its AnimationPlayer and retint
//!              instance materials per MonsterKind. The hero (card 21) gets a
//!              4-clip graph (walk/idle/attack/hit) + AnimationTransitions;
//!              monsters keep their single walk clip.
//!   sync     : monsters mirror WalkCycle.playing onto play/pause with a
//!              stateless frame-0 idle hold; the hero runs a walk/idle state
//!              machine with attack/hit one-shots on combat edges (card 21).

use std::collections::HashMap;
use std::time::Duration;

use bevy::{
    animation::RepeatAnimation, math::VectorSpace, prelude::*,
    world_serialization::WorldInstanceReady,
};

use crate::components::{Attack, Heading, Monster, MonsterKind, Player, Visual, WalkCycle};
use crate::states::GameState;

// Assets live under assets/models/ in this project (spike kept them at the root).
const HERO_GLB: &str = "models/player_hunyuan.glb";
/// Clip layout of player_hunyuan.glb (alphabetical in the glTF: attack/hit/
/// idle/walk). Pinned here because AnimationNodeIndex addressing is positional;
/// if the asset is ever re-exported with a different order, re-derive these.
pub const HERO_CLIP_ATTACK: usize = 0;
pub const HERO_CLIP_HIT: usize = 1;
pub const HERO_CLIP_IDLE: usize = 2;
pub const HERO_CLIP_WALK: usize = 3;
/// player_hunyuan is 1.33 m tall raw; scale up to the CesiumMan-era 1.8 m world
/// height the game's sizes (doorways, monster heights) were tuned around.
const HERO_SCALE: f32 = 1.353;
/// Strike/flinch display windows: the raw attack clip runs 4.7 s (way past the
/// 1.5 s slash cooldown) so we only show its early strike and blend back.
const HERO_ATTACK_WINDOW: f32 = 0.6;
const HERO_HIT_WINDOW: f32 = 0.4;
/// Cross-fade between clips (walk<->idle, one-shots, state resume).
const HERO_BLEND: Duration = Duration::from_millis(200);
// --- card 22: monster combat clips -----------------------------------------
/// All four first-batch monster models share one clip layout, and their
/// attack/hit clips are short enough to play whole — no windowing needed
/// (measured 2026-08-28: attack 0.38 s, hit 0.29 s on every kind).
const MONSTER_ATTACK_SECS: f32 = 0.38;
const MONSTER_HIT_SECS: f32 = 0.29;
/// Monsters swing as they enter biting distance: trigger slightly outside the
/// real bite radius (contact.rs CONTACT_DIST) so the wind-up precedes contact.
use crate::systems::contact::CONTACT_DIST;
const MONSTER_ATTACK_RANGE: f32 = CONTACT_DIST + 0.15;
/// The root transform centers on the physics ball (y = 0.5); all models so far
/// (legacy CesiumMan, card-19 Quaternius set, card-21 hunyuan) keep their
/// origin at the feet, so shift the model down half a unit to keep feet on the
/// ground exactly like the placeholder cube was.
const MODEL_Y_OFFSET: f32 = -0.5;
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

/// Which clip the hero state machine is addressing (card 21).
#[derive(Clone, Copy, PartialEq, Eq)]
enum HeroClip {
    Walk,
    Idle,
    Attack,
    Hit,
}

/// Per-owner animation link carried on the wrapper child; the ready observers
/// and the sync system read it to address graph/nodes/clips.
/// Card 21 gave the hero the 4-clip state machine (walk/idle + attack/hit
/// one-shots); card 22 fills the monsters' attack/hit slots from the
/// definition table and routes every owner through the shared one-shot
/// machinery — no owner-specific playback paths remain.
#[derive(Component)]
struct AnimLink {
    graph_handle: Handle<AnimationGraph>,
    walk: AnimationNodeIndex,
    idle: Option<AnimationNodeIndex>,
    attack: Option<AnimationNodeIndex>,
    hit: Option<AnimationNodeIndex>,
    /// Hero state machine bookkeeping: last clip commanded (None = never, so
    /// the first sync frame after binding always issues an initial command).
    current: Option<HeroClip>,
    /// Active one-shot: (clip, world elapsed at fire, display seconds). The
    /// state machine holds the clip for the window, then blends back.
    one_shot: Option<(HeroClip, f32, f32)>,
    /// One-shot queued by a combat edge this frame, started by the node loop
    /// (attack wins over hit if both edges land on the same frame).
    pending: Option<HeroClip>,
}

/// Combat-edge observation state (card 12 observer pattern: read-only view of
/// logic components). Edges are detected as increases because cooldown only
/// decreases and flash only decays otherwise. Card 22: every animated root
/// carries one — the player additionally watches cooldown (slash fires),
/// monsters additionally watch their distance to the player (bite wind-up).
#[derive(Component, Default)]
struct FxWatch {
    last_cooldown: f32,
    last_flash: f32,
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
                    face_towards_heading,
                )
                    .chain(),
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
    info!("[presentation] skinning player with {HERO_GLB} (4-clip graph)");

    let clip = |i: usize| assets.load(GltfAssetLabel::Animation(i).from_asset(HERO_GLB));
    let mut graph = AnimationGraph::new();
    let attack = graph.add_clip(clip(HERO_CLIP_ATTACK), 1.0, graph.root);
    let hit = graph.add_clip(clip(HERO_CLIP_HIT), 1.0, graph.root);
    let idle = graph.add_clip(clip(HERO_CLIP_IDLE), 1.0, graph.root);
    let walk = graph.add_clip(clip(HERO_CLIP_WALK), 1.0, graph.root);
    let link = AnimLink {
        graph_handle: graphs.add(graph),
        walk,
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
                info!("[presentation] hero 4-clip graph bound to {node:?} (idle initial)");
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
                    idle: None,
                    attack: Some(attack),
                    hit: Some(hit),
                    current: None,
                    one_shot: None,
                    pending: None,
                },
                WorldAssetRoot(assets.load(GltfAssetLabel::Scene(0).from_asset(model))),
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

// --- shared playback sync --------------------------------------------------

/// Playback sync, card 22 edition. All animated owners now run through
/// AnimationTransitions; each owner contributes its own edges to the shared
/// one-shot machinery:
///   player slash edge (cooldown up)  -> attack one-shot (0.6 s window)
///   monster in bite range            -> attack one-shot (0.38 s, whole clip)
///   any owner flash edge (bit/hit)   -> hit one-shot (0.4 s hero / 0.29 s)
/// State clips: hero walks/idles (card 21); monsters walk-loop while Playing.
/// Outside Playing everything still freezes statelessly (accepted pause
/// behavior): heroes pause_all(), monsters hold walk at frame 0.
/// Combat observation follows the card-12 pattern: read-only; edges are
/// increases because cooldown only decreases and flash only decays otherwise.
fn sync_walk_playback(
    state: Res<State<GameState>>,
    time: Res<Time>,
    mut roots: Query<(
        Entity,
        &Transform,
        &WalkCycle,
        Option<&Attack>,
        Option<&Visual>,
        Option<&mut FxWatch>,
        &Children,
    )>,
    player_pos: Query<&Transform, With<Player>>,
    mut links: Query<&mut AnimLink>,
    children: Query<&Children>,
    mut players: Query<&mut AnimationPlayer>,
    mut transitions: Query<&mut AnimationTransitions>,
) {
    let in_game = *state.get() == GameState::Playing;
    let now = time.elapsed_secs();
    let player_at = player_pos.single().ok().map(|t| t.translation);

    for (_root, root_tf, walk, attack, visual, watch, owner_children) in &mut roots {
        // find the wrapper child carrying the animation link
        let mut found = None;
        for kid in owner_children.iter() {
            if links.contains(kid) {
                found = Some(kid);
            }
        }
        let Some(wrapper) = found else {
            continue;
        };
        let Ok(mut link) = links.get_mut(wrapper) else {
            continue;
        };
        let moving = walk.playing;
        let is_hero = link.idle.is_some();

        // Combat bookkeeping: edges + one-shot expiry (Playing only; outside
        // the game state everything freezes and resets in the node loop).
        if in_game && link.attack.is_some() {
            if let Some(mut w) = watch {
                let fired = if is_hero {
                    // player: slash edge = cooldown reset jump
                    attack.is_some_and(|a| a.cooldown > w.last_cooldown + 1e-4)
                } else {
                    // monster: level-triggered bite wind-up (see range const)
                    player_at.is_some_and(|p| {
                        let d = Vec2::new(root_tf.translation.x - p.x, root_tf.translation.z - p.z)
                            .length();
                        d <= MONSTER_ATTACK_RANGE
                    })
                };
                let bitten = visual.is_some_and(|v| v.flash > w.last_flash + 1e-4);
                w.last_cooldown = attack.map_or(0.0, |a| a.cooldown);
                w.last_flash = visual.map_or(0.0, |v| v.flash);
                if link.one_shot.is_none() && link.pending.is_none() {
                    if fired {
                        link.pending = Some(HeroClip::Attack);
                    } else if bitten {
                        link.pending = Some(HeroClip::Hit);
                    }
                }
            }
            if let Some((_, at, window)) = link.one_shot {
                if now - at >= window {
                    // window over: blend back to the state clip next frame
                    link.one_shot = None;
                    link.current = None;
                }
            }
        }

        for node in children.iter_descendants(wrapper) {
            let Ok(mut player) = players.get_mut(node) else {
                continue;
            };

            if !in_game {
                if is_hero {
                    // hero: freeze every clip (walk AND idle) while paused
                    player.pause_all();
                    link.pending = None;
                    link.one_shot = None;
                    link.current = None;
                } else if let Some(active) = player.animation_mut(link.walk) {
                    // monsters: stateless hold at frame 0 (card 12, verbatim)
                    if !active.is_paused() || active.repeat_mode() != RepeatAnimation::Never {
                        active.pause();
                        active.seek_to(0.0);
                    }
                }
                continue;
            }

            let Ok(mut trans) = transitions.get_mut(node) else {
                continue;
            };

            // Shared one-shot fire: attack wins over hit on the same frame.
            if let Some(clip) = link.pending.take() {
                let (idx, window) = match clip {
                    HeroClip::Attack => {
                        let window = if is_hero {
                            HERO_ATTACK_WINDOW
                        } else {
                            MONSTER_ATTACK_SECS
                        };
                        (link.attack.expect("owner carries attack"), window)
                    }
                    HeroClip::Hit => {
                        let window = if is_hero {
                            HERO_HIT_WINDOW
                        } else {
                            MONSTER_HIT_SECS
                        };
                        (link.hit.expect("owner carries hit"), window)
                    }
                    HeroClip::Walk | HeroClip::Idle => {
                        unreachable!("one-shots are never state clips")
                    }
                };
                trans
                    .play(&mut player, idx, HERO_BLEND)
                    .set_repeat(RepeatAnimation::Never);
                link.one_shot = Some((clip, now, window));
                link.current = Some(clip);
            } else if link.one_shot.is_none() {
                // State clips.
                if is_hero {
                    let desired = if moving {
                        HeroClip::Walk
                    } else {
                        HeroClip::Idle
                    };
                    if link.current != Some(desired) {
                        let idx = match desired {
                            HeroClip::Walk => link.walk,
                            HeroClip::Idle => link.idle.expect("hero carries idle"),
                            HeroClip::Attack | HeroClip::Hit => {
                                unreachable!("state clips are never one-shots")
                            }
                        };
                        trans
                            .play(&mut player, idx, HERO_BLEND)
                            .set_repeat(RepeatAnimation::Forever);
                        link.current = Some(desired);
                    }
                } else if moving {
                    // monsters chase while Playing: keep the walk loop up
                    if link.current != Some(HeroClip::Walk) {
                        trans
                            .play(&mut player, link.walk, HERO_BLEND)
                            .set_repeat(RepeatAnimation::Forever);
                        link.current = Some(HeroClip::Walk);
                    }
                } else if let Some(active) = player.animation_mut(link.walk) {
                    // robustness: a not-moving monster while Playing should not
                    // happen (chasing is all they do); hold frame 0 if it does
                    if !active.is_paused() || active.repeat_mode() != RepeatAnimation::Never {
                        active.pause();
                        active.seek_to(0.0);
                    }
                }
            }
        }
    }
}

// --- card 15: monster facing ------------------------------------------------

/// Constant yaw turn speed for heading convergence (card 15 acceptance math:
/// a full about-face takes 180/540 = 0.33 s, inside the 0.6 s deadline).
pub const MAX_TURN_RATE_DEG: f32 = 540.0;

/// Turn any heading owner's model wrapper (monsters AND the player, card 18)
/// towards its logic-side `Heading` at a constant rate along the shortest arc.
/// Wrapper-local only — roots never rotate, physics never sees this
/// (rotation-locked dynamic bodies; the player root is position-driven).
/// Both wrappers carry AnimLink, which is what identifies the model child.
fn face_towards_heading(
    time: Res<Time>,
    headings: Query<(&Heading, &Children)>,
    links: Query<&AnimLink>,
    mut wrappers: Query<&mut Transform>,
) {
    let dt = time.delta_secs();
    let max_step = MAX_TURN_RATE_DEG.to_radians() * dt;

    for (heading, owner_children) in &headings {
        // the wrapper child carrying the model + AnimLink
        let mut wrapper = None;
        for kid in owner_children.iter() {
            if links.contains(kid) {
                wrapper = Some(kid);
            }
        }
        let Some(wrapper) = wrapper else { continue };
        let Ok(mut tf) = wrappers.get_mut(wrapper) else {
            continue;
        };

        // model faces +Z at yaw 0 ⇒ forward(yaw) = (sin, cos) must equal
        // (dir.x, dir.y[=world +Z]) ⇒ yaw = atan2(dir.x, dir.z)
        let target_yaw = f32::atan2(heading.dir.x, heading.dir.y);
        let current_yaw = 2.0 * f32::atan2(tf.rotation.y, tf.rotation.w);
        let diff = wrap_pi(target_yaw - current_yaw);
        let step = diff.clamp(-max_step, max_step);
        if step != 0.0 {
            tf.rotation = Quat::from_rotation_y(wrap_pi(current_yaw + step));
        }
    }
}

/// Wrap an angle into (-π, π].
fn wrap_pi(a: f32) -> f32 {
    const TWO_PI: f32 = std::f32::consts::TAU;
    (a + std::f32::consts::PI).rem_euclid(TWO_PI) - std::f32::consts::PI
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

/// Card 14 flash curve: emissive rests at BLACK and whiteouts as `flash`
/// approaches 1. Single source of truth — the original formula lerped from
/// `base_color`, which left a permanent additive self-glow at flash=0 and
/// washed every model out to flat gray (art regression, fixed 2026-08-28).
fn flash_emissive(flash: f32) -> LinearRgba {
    LinearRgba::BLACK.lerp(LinearRgba::WHITE, flash)
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
                        // rest-at-black: see flash_emissive docs
                        variant.emissive = flash_emissive(vis.flash);
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
                material.emissive = flash_emissive(vis.flash);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flash_emissive_rests_at_black() {
        // THE regression: the old formula rested at base_color, adding a
        // permanent self-glow that washed every character to flat gray.
        let rest = flash_emissive(0.0);
        assert!(rest.red < 1e-6 && rest.green < 1e-6 && rest.blue < 1e-6);
    }

    #[test]
    fn flash_emissive_whiteouts_at_one() {
        let full = flash_emissive(1.0);
        assert!((full.red - 1.0).abs() < 1e-6);
        assert!((full.green - 1.0).abs() < 1e-6);
        assert!((full.blue - 1.0).abs() < 1e-6);
    }

    #[test]
    fn flash_emissive_is_linear_in_flash() {
        let quarter = flash_emissive(0.25);
        assert!((quarter.red - 0.25).abs() < 1e-6);
        assert!((quarter.blue - 0.25).abs() < 1e-6);
    }
}
