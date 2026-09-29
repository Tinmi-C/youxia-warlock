//! Towers: auto-fire + targeting + one-axis armor, including the fused tower
//! behaviours (Marksman highest-HP priority, AOE blast, slow).
//! Capability cards: TO3 (fire — resolution moved into projectiles by PR1),
//! TO4 (targeting), TO5 (fusion result), TO6 (slow — on-hit since PR1).
//!
//! Hybrid attack model (requirements §5.1): ranged towers (archer / mage /
//! cannon and every fused tower) LAUNCH a homing projectile (damage snapshot
//! at fire, armor at impact, slow rides the hit); the melee shield tower
//! keeps its instant hit.

use bevy::prelude::*;

use crate::components::{
    AimTracker, AttackType, BallistaMount, EffectiveStats, Enemy, FireRecoil, FusionKind,
    Projectile, ProjectileOrigin, Slow, Tower, TowerKind,
};
// ART12 ranger defender wiring shares the enemy rig machinery.
use crate::components::{RangerBody, RangerCombat, RangerRig};
use crate::systems::fx::{HitFlash, HIT_FLASH_SECS};
use crate::resources::{TowerDef, PROJECTILE_SPEED};

/// The bundled 3D arrow tower (ArtVault 岩库 locked v1 via art-sync, ADR-0009;
/// `art-assets.json` is the source of truth). Rodin image-to-3D, 23332 tris,
/// bbox ±0.95 in Y centered at the model origin. Known defect: the flag did
/// not generate (ADR-0009) — an in-engine patch is a follow-up card.
/// ART12: RETIRED from the archer look (the ranger defender replaced the
/// building+ballista assembly); kept pinned — the showroom and any future
/// "building" skins still reference it.
pub const ARROW_TOWER_MODEL: &str = "models/tower_arrow.glb";

/// ART12: the archer tower IS the KayKit Ranger character (岩库 v2), bow in
/// hand — the defense reads as a person, not a building. Same rig family as
/// the enemies (feet origin, ~2.17 m bind pose, 22 clips, loader-planted
/// AnimationPlayer + AnimatedBy links — drive THAT player, see ART11's
/// T-pose lesson).
pub const RANGER_MODEL: &str = "models/tower_ranger.glb";
/// Visual scale: matches the enemy archetypes (0.8 → ≈1.75 m).
pub const RANGER_SCALE: f32 = 0.8;
/// The ranger's feet sit on the ground (world 0) — the tower logic anchor
/// rides at 0.6, so the scene child offsets -0.6.
pub const RANGER_MOUNT_Y: f32 = -0.6;
/// Bow with string (KayKit weapons pack, 岩库 v2). The ranger body ships
/// without a weapon mesh; KayKit rigs expose a dedicated `handslot.r` bone
/// exactly for this.
pub const BOW_MODEL: &str = "models/weapon_bow.glb";
/// KayKit's right-hand weapon mount bone (named node inside the rig scene).
pub const BOW_SLOT_NODE: &str = "handslot.r";
/// Idle clip preference (KayKit names: `Rig_Medium.002|Idle_A/B`).
pub const RANGER_IDLE_CLIP: &str = "idle_a";
/// Attack clip: KayKit has no bow-shoot clip; `Throw` (draw arm back, snap
/// forward) is the pack's stand-in and reads as loosing an arrow.
pub const RANGER_ATTACK_CLIP: &str = "throw";

/// The bundled ballista weapon (ArtVault v2 — the game-side compressed
/// reflow: 4096→2048 PNG, 35.2 MB → 8.9 MB, geometry untouched). Mounted on
/// the archer tower (ART2 assembly handoff).
pub const BALLISTA_MODEL: &str = "models/weapon_ballista.glb";
/// Mount height above the tower's logic anchor. The tower visual spans
/// world 0.0–1.9, but its bbox top (+0.953 model space) is thin tip
/// decoration — the vertex histogram shows the actual deck plane at +0.75
/// (1077-vertex full-footprint band; parapet ring caps at +0.83). The
/// ballista origin is bottom-centered, so it rides the DECK: 0.6 anchor +
/// 0.35 child offset puts the tower origin at world 0.95, deck at world
/// 1.70 → tower-local 1.10. (ART2 first set 1.3 = bbox top, which visibly
/// floated the weapon above the deck; corrected 2026-09-20 from offline
/// vertex analysis after a user report.)
pub const BALLISTA_MOUNT_Y: f32 = 1.10;
/// The ballista reads large against the tower — uniform shrink (handoff:
/// 0.7–0.75; 0.72 chosen, tune on visual acceptance).
pub const BALLISTA_SCALE: f32 = 0.72;

// --- ART7: aim tracking + muzzle launch --------------------------------------

/// Max turn rate of the mounted weapon in degrees per second (card starting
/// value; tune on visual acceptance — 240°/s covers any practical target
/// swing at projectile speed 10).
pub const AIM_TURN_RATE: f32 = 240.0;
/// Seconds without a target before the weapon eases back to its rest bearing
/// (card starting value 2.0).
pub const AIM_RETURN_DELAY: f32 = 2.0;
/// Rest bearing in degrees — 180 keeps the −X muzzle facing +X world at
/// rest (the towers face the enemy entry side of the L-path on most slots);
/// mirrored from the original 0° when the forward sign flipped (2026-09-21).
pub const BALLISTA_REST_BEARING: f32 = 180.0;
/// Model-space unit axis pointing out of the MUZZLE. Orientation basis:
/// the offline vertex heuristic (2026-09-20: tallest rail-bridge cluster on
/// the +X half → +X) was WRONG — the one-time manual acceptance (2026-09-21,
/// youxia via the shooting range) saw the weapon facing away from the bolt
/// direction and the recoil kicking the wrong way, so the muzzle is −X.
/// Flipped together with the pinned tests `ballista_projectile_spawns_at_muzzle`
/// and `ballista_aim_converges_holds_then_returns_to_rest` per the ART7
/// flip protocol.
pub const BALLISTA_FORWARD_AXIS: Vec3 = Vec3::NEG_X;

/// Wrap an angle difference into (-π, π] (shortest rotational direction).
fn wrap_angle(a: f32) -> f32 {
    (a + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU)
        - std::f32::consts::PI
}

/// ART8: one message per tower fire — the fire-moment seam the presentation
/// layer reacts to (mount recoil now; the ART10 string/bolt animation later)
/// without the rule side knowing any of them.
#[derive(Message, Debug)]
pub struct TowerFired {
    pub tower: Entity,
}

/// Yaw (radians, +X-frame: `rot_y(0)·X = +X`) of a horizontal direction —
/// the yaw whose `rot_y·X` points along `v` on the XZ plane.
fn xz_yaw(v: Vec3) -> f32 {
    f32::atan2(-v.z, v.x)
}

/// Yaw (radians) that rotates the model-space `forward` axis to point from
/// `from` toward `to` on the XZ plane, or None when the two share XZ
/// coordinates (degenerate — keep current facing). Axis-aware: desired =
/// yaw(dir) − yaw(forward), so a −X-muzzle model tracks 180° apart from a
/// +X one while both end up visually facing the target.
fn bearing_yaw(from: Vec3, to: Vec3, forward: Vec3) -> Option<f32> {
    let d = to - from;
    (d.x * d.x + d.z * d.z > 1e-8).then_some(wrap_angle(xz_yaw(d) - xz_yaw(forward)))
}

/// ART7 domain bridge: mirror each tower's current rule-layer target into
/// its mounted part's `AimTracker`. Keeps `tick_aim_trackers` free of any
/// tower dependency (the generic turner only knows `AimTracker`).
pub fn tower_aim_bridge(
    towers: Query<(&Tower, &Children)>,
    mut parts: Query<&mut AimTracker>,
) {
    for (tower, children) in &towers {
        for child in children {
            if let Ok(mut tracker) = parts.get_mut(*child) {
                tracker.target = tower.target;
            }
        }
    }
}

/// ART7 generic turner: rotate every `AimTracker` part toward its target's
/// bearing at `turn_rate` (yaw only — the ballista trains horizontally);
/// after `return_delay` seconds without a target, rotate back to
/// `rest_bearing`. Pure presentation: never fires, never blocks firing
/// (rhythm stays owned by `tower_fire`, the ART1 gateway philosophy).
pub fn tick_aim_trackers(
    time: Res<Time>,
    mut parts: Query<(&mut AimTracker, &mut Transform, &GlobalTransform)>,
    targets: Query<&Transform, Without<AimTracker>>,
) {
    let dt = time.delta_secs();
    for (mut tracker, mut tf, gtf) in &mut parts {
        // Read the facing back from the transform (we own it: only yaw is
        // ever written here).
        let yaw = tf.rotation.to_euler(EulerRot::YXZ).0;
        let desired = match tracker.target.and_then(|e| targets.get(e).ok()) {
            Some(ttf) => {
                tracker.since_target = 0.0;
                bearing_yaw(gtf.translation(), ttf.translation, tracker.forward_axis)
            }
            None => {
                tracker.since_target += dt;
                (tracker.since_target >= tracker.return_delay)
                    .then_some(tracker.rest_bearing.to_radians())
            }
        };
        let Some(desired) = desired else { continue };
        let max_step = tracker.turn_rate.to_radians() * dt;
        let diff = wrap_angle(desired - yaw).clamp(-max_step, max_step);
        tf.rotation = Quat::from_rotation_y(yaw + diff);
    }
}

/// ART8 recoil driver: a `TowerFired` for an owning tower RESETS every
/// mounted `FireRecoil` child's cycle (a re-trigger mid-recovery restarts —
/// kicks never accumulate), then the displacement eases linearly back to
/// rest. Runs right after `tower_fire` so the kick appears on the fire
/// frame. Pure presentation: rules, targeting and the muzzle math read
/// nothing from it, and the kick only rides the part's TRANSLATION (aim
/// convergence owns the rotation), so recoil never disturbs tracking.
pub fn tick_fire_recoil(
    time: Res<Time>,
    mut fired: MessageReader<TowerFired>,
    towers: Query<&Children>,
    mut parts: Query<(&mut FireRecoil, &mut Transform, &AimTracker)>,
) {
    for msg in fired.read() {
        if let Ok(children) = towers.get(msg.tower) {
            for child in children {
                if let Ok((mut recoil, _, _)) = parts.get_mut(*child) {
                    recoil.elapsed = Some(0.0);
                }
            }
        }
    }
    let dt = time.delta_secs();
    for (mut recoil, mut tf, aim) in &mut parts {
        let displacement = match recoil.elapsed {
            None => 0.0,
            Some(t) => {
                let t = t + dt;
                recoil.elapsed = (t < recoil.recover_secs).then_some(t);
                recoil.depth * (1.0 - t / recoil.recover_secs).clamp(0.0, 1.0)
            }
        };
        // Kick along the part's OWN backward axis (parent-space direction;
        // mount parents are unrotated tower roots, so parent == world — and
        // using the live rotation keeps the offset axis correct while the
        // AimTracker turns mid-recovery).
        let backward = tf.rotation * -aim.forward_axis;
        tf.translation = recoil.rest_translation + backward * displacement;
    }
}


/// Spawn a placed base tower of archetype `def` at world position `pos`
/// (shared by the player flows and the DB debug scenarios). Returns the
/// spawned entity.
///
/// Presentation gateway (ART1): the archer (index 0) uses the bundled 3D
/// model when an `AssetServer` exists — the scene rides a child entity lifted
/// by 0.35 so the model's base touches the ground under the 0.6 logic anchor;
/// every other tower (and every headless test world) keeps the tinted gray
/// box. Logic components and the anchor position are identical either way.
pub fn spawn_base_tower(
    commands: &mut Commands,
    def: &TowerDef,
    tower_index: usize,
    pos: Vec3,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    server: Option<&AssetServer>,
) -> bevy::ecs::entity::Entity {
    let mut ec = commands.spawn((
        Tower {
            tower_index,
            damage: def.damage,
            attack_speed: def.attack_speed,
            range: def.range,
            attack_type: def.attack_type,
            cooldown: 0.0,
            target: None,
            kind: TowerKind::Base(tower_index),
            aoe_radius: 0.0,
            slow_factor: 0.0,
            slow_duration: 0.0,
            invested: def.cost,
        },
        // Explicit Visibility: the model path has no mesh on this entity, so
        // without it the scene child's InheritedVisibility breaks (B0004).
        Visibility::Visible,
        Transform::from_xyz(pos.x, 0.6, pos.z),
    ));
    attach_tower_appearance(&mut ec, def, tower_index, meshes, materials, server);
    ec.id()
}

/// The in-game look of a base tower (SM1 appearance seam): ART12 — the
/// archer IS the KayKit Ranger character (bow wired to handslot.r, idle loop
/// + Throw-on-fire, body yaw follows the target); every other tower keeps
/// the tinted gray box. Pure presentation — no rule components. Shared by
/// `spawn_base_tower` and the showroom exhibit spawner so the display can
/// never drift from the real game.
pub fn attach_tower_appearance(
    ec: &mut EntityCommands,
    def: &TowerDef,
    tower_index: usize,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    server: Option<&AssetServer>,
) {
    if tower_index == 0 {
        if let Some(server) = server {
            let scene =
                server.load(bevy::gltf::GltfAssetLabel::Scene(0).from_asset(RANGER_MODEL));
            let gltf = server.load::<bevy::gltf::Gltf>(RANGER_MODEL);
            ec.with_children(|p| {
                // Feet on the ground (world 0); the uniform scale shares the
                // Transform that `ranger_face_targets` yaws.
                p.spawn((
                    RangerRig {
                        gltf,
                        wired: None,
                    },
                    crate::components::RangerBody,
                    bevy::world_serialization::WorldAssetRoot(scene),
                    Transform::from_xyz(0.0, RANGER_MOUNT_Y, 0.0)
                        .with_scale(Vec3::splat(RANGER_SCALE)),
                ));
            });
            return;
        }
    }
    // Gray-box fallback: every other tower + all headless test worlds.
    ec.insert((
        Mesh3d(meshes.add(Cuboid::new(1.0, 1.0, 1.0))),
        MeshMaterial3d(materials.add(match def.attack_type {
            AttackType::Physical => Color::srgb(0.7, 0.7, 0.8),
            AttackType::Magic => Color::srgb(0.5, 0.4, 0.9),
            AttackType::Mixed => Color::srgb(0.6, 0.9, 0.6),
        })),
    ));
}

/// ART12: wire the ranger rig — and keep it wired. Bevy re-instantiates the
/// whole scene on a WorldAsset `Modified` event (e.g. a second archer
/// re-loading the same GLB), which swaps in a FRESH loader player; a
/// consume-once marker would leave the rebuilt rig permanently T-posed. So
/// this system health-checks every frame: if our recorded player died with
/// the old scene, it re-attaches the (app-owned, still-valid) graph,
/// re-creates `RangerCombat`, and re-hangs the bow.
#[allow(clippy::type_complexity)]
pub fn wire_ranger(
    mut commands: Commands,
    mut graphs: ResMut<Assets<AnimationGraph>>,
    gltfs: Res<Assets<bevy::gltf::Gltf>>,
    clips: Res<Assets<AnimationClip>>,
    mut pending: Query<(Entity, &mut RangerRig, &ChildOf)>,
    children: Query<&Children>,
    mut players: Query<&mut AnimationPlayer>,
    combat: Query<(), With<RangerCombat>>,
    names: Query<&Name>,
    server: Option<Res<AssetServer>>,
) {
    for (wrapper, mut rig, parent) in &mut pending {
        // Healthy: recorded player alive and still combat-armed.
        if let Some(w) = &rig.wired {
            if combat.get(w.player).is_ok() {
                continue;
            }
        }
        let Some(gltf) = gltfs.get(&rig.gltf) else {
            continue; // still loading — retry next frame
        };
        // Clip lookup by substring, preferring the `_A` variant of a family.
        let find_clip = |want: &str| -> Option<(String, Handle<AnimationClip>, f32)> {
            let mut best: Option<(&Box<str>, &Handle<AnimationClip>, bool)> = None;
            for (name, handle) in &gltf.named_animations {
                let lower = name.to_lowercase();
                if !lower.contains(want) {
                    continue;
                }
                let preferred = lower.ends_with("_a");
                let take = match best {
                    None => true,
                    Some((_, _, best_pref)) => preferred && !best_pref,
                };
                if take {
                    best = Some((name, handle, preferred));
                }
            }
            best.map(|(name, handle, _)| {
                let secs = clips
                    .get(handle)
                    .map(|c| c.duration())
                    .unwrap_or(0.5);
                (name.to_string(), handle.clone(), secs)
            })
        };
        let Some((idle_name, idle_clip, _)) = find_clip(RANGER_IDLE_CLIP) else {
            info!("[art12] ranger has no idle clip — static pose");
            commands
                .entity(wrapper)
                .remove::<RangerRig>();
            continue;
        };
        let (attack_name, attack_clip, attack_secs) =
            find_clip(RANGER_ATTACK_CLIP).unwrap_or_else(|| {
                (idle_name.clone(), idle_clip.clone(), 0.5)
            });
        // The scene may still be one frame from instantiating its player.
        let Some(player_e) = crate::systems::enemy::find_player(wrapper, &children, &players)
        else {
            continue;
        };
        // Reuse the graph across rebuilds (app-owned, survives the scene).
        let wired = rig.wired.take();
        let (handle, idle_node, attack_node) = match wired {
            Some(w) => (w.graph, w.idle, w.attack),
            None => {
                let (mut graph, idle_node) = AnimationGraph::from_clip(idle_clip);
                let attack_node = graph.add_clip(attack_clip, 1.0, graph.root);
                (graphs.add(graph), idle_node, attack_node)
            }
        };
        let Ok(mut player) = players.get_mut(player_e) else {
            continue;
        };
        player.play(idle_node).repeat();
        commands.entity(player_e).insert((
            AnimationGraphHandle(handle.clone()),
            RangerCombat {
                tower: parent.0,
                idle: idle_node,
                attack: attack_node,
                attack_secs,
                recover: 0.0,
            },
        ));
        rig.wired = Some(crate::components::RangerWired {
            player: player_e,
            graph: handle,
            idle: idle_node,
            attack: attack_node,
            attack_secs,
        });
        // Bow on the dedicated weapon bone (idempotent: a fresh handslot is
        // bare — the rebuild lost the old bow — so attach only when it has
        // no children yet).
        if let Some(server) = server.as_deref() {
            match find_named(wrapper, BOW_SLOT_NODE, &children, &names) {
                Some(slot) => {
                    let bare = children.get(slot).is_err();
                    if bare {
                        let bow = server.load(
                            bevy::gltf::GltfAssetLabel::Scene(0).from_asset(BOW_MODEL),
                        );
                        commands.entity(slot).with_children(|p| {
                            p.spawn((
                                bevy::world_serialization::WorldAssetRoot(bow),
                                Transform::IDENTITY,
                            ));
                        });
                    }
                }
                None => info!("[art12] handslot.r not found — bow skipped"),
            }
        }
        info!("[art12] ranger wired: idle={idle_name} attack={attack_name}");
    }
}

/// Depth-first search for the entity named `want` below `root` (rig bones
/// carry their authoring `Name`s into the spawned scene).
fn find_named(
    root: Entity,
    want: &str,
    children: &Query<&Children>,
    names: &Query<&Name>,
) -> Option<Entity> {
    if let Ok(n) = names.get(root) {
        if n.as_str() == want {
            return Some(root);
        }
    }
    let kids = children.get(root).ok()?;
    kids.iter().find_map(|k| find_named(k, want, children, names))
}

/// The in-game look of a fused tower: its per-kind color on a 1.2 box (SM1
/// appearance seam — the single source the fusion placement and the showroom
/// both read, extracted from command.rs unchanged).
pub fn fused_tower_color(kind: FusionKind) -> Color {
    match kind {
        FusionKind::Marksman => Color::srgb(0.2, 0.8, 1.0),
        FusionKind::Archmage => Color::srgb(0.7, 0.2, 1.0),
        FusionKind::Hybrid => Color::srgb(0.2, 0.9, 0.4),
        FusionKind::Bastion => Color::srgb(1.0, 0.6, 0.1),
    }
}

/// Resolve one-axis armor: physical is halved vs armored enemies; magic and
/// mixed (穿甲) bypass it.
pub fn effective_damage(attack_type: AttackType, physical_armor: bool, base: f32) -> f32 {
    if physical_armor && attack_type == AttackType::Physical {
        base * 0.5
    } else {
        base
    }
}

/// The melee exception of the hybrid model (§5.1): the base shield tower.
fn is_melee(tower: &Tower) -> bool {
    matches!(tower.kind, TowerKind::Base(1))
}

pub fn tower_fire(
    time: Res<Time>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut fired: MessageWriter<TowerFired>,
    // EffectiveStats (SK6) carries the folded numbers: base fields × Boosts
    // × Frenzy × StatMods. This system no longer reads Boosts/WarFrenzy.
    mut towers: Query<(Entity, &mut Tower, &Transform, Option<&Children>, &EffectiveStats)>,
    mut enemies: Query<(Entity, &Transform, &mut Enemy)>,
    mounts: Query<(&Transform, &AimTracker), With<BallistaMount>>,
) {
    // `tower_eid` feeds the ART8 fire-moment message.
    for (tower_eid, mut tower, ttf, children, estats) in &mut towers {
        tower.cooldown -= time.delta_secs();

        let marksman = tower.kind == TowerKind::Fused(FusionKind::Marksman);

        let base_dmg = estats.damage;
        let effective_range = estats.range;

        // Pick target: nearest, or highest-HP for the Marksman.
        let mut best: Option<(Entity, f32, f32)> = None; // (eid, dist, hp)
        for (eid, etf, en) in &enemies {
            if en.hp <= 0.0 {
                continue;
            }
            let d = ttf.translation.distance(etf.translation);
            if d > effective_range {
                continue;
            }
            let better = match best {
                None => true,
                Some((_, bd, bhp)) => {
                    if marksman {
                        en.hp > bhp
                    } else {
                        d < bd
                    }
                }
            };
            if better {
                best = Some((eid, d, en.hp));
            }
        }
        // ART7: persist the acquisition so the presentation layer (mount
        // turn-to-track via `tower_aim_bridge`) can follow it. This is
        // rule→rule bookkeeping only — the fire decision below is unchanged.
        tower.target = best.map(|(eid, _, _)| eid);
        let Some((target, _, _)) = best else {
            continue;
        };
        if tower.cooldown > 0.0 {
            continue;
        }

        if is_melee(&tower) {
            // Melee (shield tower): instant hit — damage + slow on the target
            // only (TO6 rework: the slow rides the hit, §5 original wording).
            // The slow goes through the shared merge path (SK3 semantics).
            if let Ok((_, _, mut en)) = enemies.get_mut(target) {
                let applied = effective_damage(tower.attack_type, en.physical_armor, base_dmg);
                en.hp -= applied;
                commands.entity(target).insert(HitFlash {
                    timer: HIT_FLASH_SECS,
                    original: None,
                });
                if tower.slow_factor > 0.0 {
                    apply_slow(
                        &mut commands,
                        target,
                        tower.slow_factor,
                        tower.slow_duration,
                    );
                }
                info!("[tower] melee hit dmg={applied:.1} enemy_hp={:.0}", en.hp);
            }
        } else {
            // Ranged: launch a homing projectile (§5.1) — damage and type are
            // snapshotted here; armor resolves at impact (projectiles.rs).
            // ART7 muzzle launch: when the tower carries an aimed mount (the
            // ballista), the round leaves the mount's world position offset
            // along the mount's CURRENT facing — presentation supplies the
            // birth point, the rules stay unchanged (firing does NOT wait
            // for the turn to converge; the homing round corrects course).
            // ART12: the ranger defender carries no mount — the round leaves
            // from bow height (anchor 0.6 + 0.9 = world 1.5 for a 1.75 m
            // archer); every headless world keeps this same fallback.
            let start = children
                .and_then(|ch| ch.iter().find_map(|c| mounts.get(c).ok()))
                .map(|(mtf, tracker)| {
                    let world = ttf.mul_transform(*mtf);
                    let forward = (world.rotation * tracker.forward_axis).normalize_or_zero();
                    world.translation + forward * tracker.muzzle_offset
                })
                .unwrap_or_else(|| ttf.translation + Vec3::Y * 0.9);
            let (mesh, color) = projectile_visual(&tower, tower.aoe_radius);
            commands.spawn((
                Projectile {
                    origin: ProjectileOrigin::Homing { target },
                    damage: base_dmg,
                    attack_type: tower.attack_type,
                    aoe_radius: tower.aoe_radius,
                    slow_factor: tower.slow_factor,
                    slow_duration: tower.slow_duration,
                    speed: PROJECTILE_SPEED,
                    last_target_pos: enemies
                        .get(target)
                        .map(|(_, etf, _)| etf.translation)
                        .unwrap_or(start),
                },
                Mesh3d(meshes.add(mesh)),
                MeshMaterial3d(materials.add(StandardMaterial {
                    base_color: color,
                    unlit: true,
                    ..default()
                })),
                Transform::from_translation(start),
            ));
        }

        // ART8: the fire-moment seam — one message per shot (melee and
        // ranged alike); presentation (recoil, later ART10) self-selects.
        fired.write(TowerFired { tower: tower_eid });

        // SK6: attack speed (Boosts × Frenzy × StatMods already folded).
        tower.cooldown = 1.0 / estats.attack_speed;
    }
}

/// ART12: the fire-moment seam for the ranger — each shot plays the one-shot
/// attack clip (Throw) and arms the recovery timer; when it counts out the
/// idle loop resumes. Re-firing mid-attack restarts the clip (matches the
/// ART8 recoil reset semantics).
pub fn ranger_play_attack(
    mut fired: MessageReader<TowerFired>,
    mut rangers: Query<(&mut AnimationPlayer, &mut RangerCombat)>,
) {
    for msg in fired.read() {
        for (mut player, mut combat) in &mut rangers {
            if combat.tower == msg.tower {
                player.play(combat.attack);
                combat.recover = combat.attack_secs;
            }
        }
    }
}

/// ART12: countdown back to the idle loop once the attack clip finished.
pub fn ranger_tick_recover(
    time: Res<Time>,
    mut rangers: Query<(&mut AnimationPlayer, &mut RangerCombat)>,
) {
    for (mut player, mut combat) in &mut rangers {
        if combat.recover > 0.0 {
            combat.recover -= time.delta_secs();
            if combat.recover <= 0.0 {
                player.play(combat.idle).repeat();
            }
        }
    }
}

/// ART12: the ranger's body faces the current target (same +Z-forward
/// convention as the enemies; the head/arms detail comes from the rig).
/// Without a target the last facing holds — towers idle exactly where they
/// last aimed, which reads naturally for a standing guard.
pub fn ranger_face_targets(
    towers: Query<(&Tower, &Transform)>,
    enemies: Query<&Transform, With<Enemy>>,
    mut bodies: Query<
        (&mut Transform, &ChildOf),
        (With<RangerBody>, Without<Tower>, Without<Enemy>),
    >,
) {
    for (mut tf, parent) in &mut bodies {
        let Ok((tower, ttf)) = towers.get(parent.0) else {
            continue;
        };
        let Some(target) = tower.target else {
            continue;
        };
        let Ok(etf) = enemies.get(target) else {
            continue;
        };
        let d = etf.translation - ttf.translation;
        if d.x * d.x + d.z * d.z > 1e-8 {
            tf.rotation = Quat::from_rotation_y(f32::atan2(d.x, d.z));
        }
    }
}

/// Round visuals (§5.1): arrow = grey-white sliver, magic round = purple
/// sphere, cannon round = orange sphere.
fn projectile_visual(tower: &Tower, aoe_radius: f32) -> (Mesh, Color) {
    match tower.attack_type {
        AttackType::Physical if aoe_radius > 0.0 => {
            (Mesh::from(Sphere::new(0.25)), Color::srgb(1.0, 0.6, 0.15))
        }
        AttackType::Physical => (
            Mesh::from(Cuboid::new(0.08, 0.08, 0.55)),
            Color::srgb(0.85, 0.85, 0.85),
        ),
        _ => (Mesh::from(Sphere::new(0.18)), Color::srgb(0.65, 0.4, 1.0)),
    }
}

/// Tick down slow timers on enemies and remove expired effects.
pub fn tick_slow(
    time: Res<Time>,
    mut commands: Commands,
    mut slowed: Query<(Entity, &mut Slow)>,
) {
    for (eid, mut slow) in &mut slowed {
        slow.timer -= time.delta_secs();
        if slow.timer <= 0.0 {
            commands.entity(eid).remove::<Slow>();
        }
    }
}

/// Deferred slow application with coexistence semantics (SK3, requirements
/// §17.2): when a slow is already running, keep the STRONGER factor (the
/// smaller multiplier — e.g. frost nova 0.5 vs bastion 0.7 keeps 0.5) and
/// refresh the timer to the new application's duration. Slows never stack
/// multiplicatively. A command so every application site (bastion hits, the
/// nova) shares one merge path without extra queries.
pub struct ApplySlow {
    pub entity: Entity,
    pub factor: f32,
    pub duration: f32,
}

impl Command for ApplySlow {
    type Out = ();
    fn apply(self, world: &mut World) {
        let merged = match world.get::<Slow>(self.entity) {
            Some(existing) => Slow {
                factor: existing.factor.min(self.factor),
                timer: self.duration,
            },
            None => Slow {
                factor: self.factor,
                timer: self.duration,
            },
        };
        world.entity_mut(self.entity).insert(merged);
    }
}

/// Queue a slow effect with the SK3 merge semantics (see `ApplySlow`).
pub fn apply_slow(commands: &mut Commands, entity: Entity, factor: f32, duration: f32) {
    commands.queue(ApplySlow {
        entity,
        factor,
        duration,
    });
}
