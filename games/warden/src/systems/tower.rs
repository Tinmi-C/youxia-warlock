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
    AimTracker, AttackType, BallistaMount, Enemy, FireRecoil, FusionKind, Projectile,
    ProjectileOrigin, Slow, Tower, TowerKind,
};
use crate::systems::fx::{HitFlash, HIT_FLASH_SECS};
use crate::resources::{Boosts, TowerDef, PROJECTILE_SPEED};

/// The bundled 3D arrow tower (ArtVault 岩库 locked v1 via art-sync, ADR-0009;
/// `art-assets.json` is the source of truth). Rodin image-to-3D, 23332 tris,
/// bbox ±0.95 in Y centered at the model origin. Known defect: the flag did
/// not generate (ADR-0009) — an in-engine patch is a follow-up card.
pub const ARROW_TOWER_MODEL: &str = "models/tower_arrow.glb";

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

/// The in-game look of a base tower (SM1 appearance seam): the archer model
/// scene with its ballista mount, or the tinted gray box. Pure presentation —
/// no rule components. Shared by `spawn_base_tower` and the showroom exhibit
/// spawner so the display can never drift from the real game.
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
            let scene = server.load(
                bevy::gltf::GltfAssetLabel::Scene(0).from_asset(ARROW_TOWER_MODEL),
            );
            let ballista = server.load(
                bevy::gltf::GltfAssetLabel::Scene(0).from_asset(BALLISTA_MODEL),
            );
            ec.with_children(|p| {
                p.spawn((
                    bevy::world_serialization::WorldAssetRoot(scene),
                    Transform::from_xyz(0.0, 0.35, 0.0),
                ));
                // ART2: the ballista rides the tower top as its own entity
                // (not part of the body scene) so the future turn-to-track
                // and procedural fire cards can drive it independently.
                // ART7: it carries the generic AimTracker (turn-to-track +
                // muzzle offset from the def) driving that rotation.
                // ART8: plus the generic FireRecoil (kick + linear recovery,
                // also def-configured) riding its translation.
                p.spawn((
                    BallistaMount,
                    AimTracker {
                        turn_rate: AIM_TURN_RATE,
                        rest_bearing: BALLISTA_REST_BEARING,
                        return_delay: AIM_RETURN_DELAY,
                        forward_axis: BALLISTA_FORWARD_AXIS,
                        muzzle_offset: def.muzzle_offset,
                        target: None,
                        since_target: 0.0,
                    },
                    FireRecoil {
                        depth: def.recoil_depth,
                        recover_secs: def.recoil_recover_secs,
                        rest_translation: Vec3::new(0.0, BALLISTA_MOUNT_Y, 0.0),
                        elapsed: None,
                    },
                    bevy::world_serialization::WorldAssetRoot(ballista),
                    // Spawn AT the rest bearing (not identity) so the
                    // muzzle faces +X world before the first target — with
                    // the −X forward sign this is 180°, and identity would
                    // have the weapon spawn backwards.
                    Transform::from_xyz(0.0, BALLISTA_MOUNT_Y, 0.0)
                        .with_rotation(Quat::from_rotation_y(
                            BALLISTA_REST_BEARING.to_radians(),
                        ))
                        .with_scale(Vec3::splat(BALLISTA_SCALE)),
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
    boost: Res<Boosts>,
    frenzy: Res<crate::resources::WarFrenzy>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut fired: MessageWriter<TowerFired>,
    mut towers: Query<(Entity, &mut Tower, &Transform, Option<&Children>)>,
    mut enemies: Query<(Entity, &Transform, &mut Enemy)>,
    mounts: Query<(&Transform, &AimTracker), With<BallistaMount>>,
) {
    // SK4: a global timed multiplier — while the frenzy lasts, EVERY tower's
    // fire cooldown is divided by it (towers placed mid-frenzy included).
    let frenzy_mult = if frenzy.remaining > 0.0 {
        crate::resources::FRENZY_SPEED_MULT
    } else {
        1.0
    };
    // `tower_eid` feeds the ART8 fire-moment message.
    for (tower_eid, mut tower, ttf, children) in &mut towers {
        tower.cooldown -= time.delta_secs();

        let marksman = tower.kind == TowerKind::Fused(FusionKind::Marksman);

        // AC4 stat boosts (polish): a tower reads the multiplier of its
        // `tower_index` — for a base tower that is its own type; for a fused
        // tower it is the first ingredient's type, so fused towers also benefit.
        let idx = tower.tower_index;
        let dmg_mult = boost.damage_mult.get(idx).copied().unwrap_or(1.0);
        let speed_mult = boost.attack_speed_mult.get(idx).copied().unwrap_or(1.0);
        let range_mult = boost.range_mult.get(idx).copied().unwrap_or(1.0);
        let base_dmg = tower.damage * dmg_mult;
        let effective_range = tower.range * range_mult;

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
            // Mount-less towers (every headless world, gray-box types) keep
            // the original anchor+0.4Y birth point, so all pre-ART7 flight
            // timing regressions (PR1 ≈0.8s band) are untouched.
            let start = children
                .and_then(|ch| ch.iter().find_map(|c| mounts.get(c).ok()))
                .map(|(mtf, tracker)| {
                    let world = ttf.mul_transform(*mtf);
                    let forward = (world.rotation * tracker.forward_axis).normalize_or_zero();
                    world.translation + forward * tracker.muzzle_offset
                })
                .unwrap_or_else(|| ttf.translation + Vec3::Y * 0.4);
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

        tower.cooldown = 1.0 / (tower.attack_speed * speed_mult * frenzy_mult);
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
