//! Generic battle FX (WX4, warden FX1/FX2 lifted): health bars, hit flash,
//! blast rings, death shells — the visual primitives every battle game
//! shares. Data sources are base components (Hp/MaxHp) and the `FxBlast`
//! message; bar styling is game-supplied via `HealthBarStyle` (data-driven).
//! Game-identity visuals — status tints, unit badges, heavy skill VFX —
//! stay game-side by design (they consume DamageDealt/skill events).
//!
//! FXP (round-3 plan Phase 1): presentation primitives that read ONLY base
//! messages/components — damage floats (DamageDealt), status markers
//! (Stunned/Silenced, CJK glyphs via the game-injected `FxFont`), aura rings
//! (Auras carriers), kill bursts (UnitDied). Zero game logic here; every
//! system is a pure consumer.

use bevy::prelude::*;

use crate::unit::{Hp, MaxHp};
use crate::WarbaseSet;

// --- Health bars (warden FX1 generalized) ----------------------------------------

/// Marker on a unit's health bar (a child entity of the unit).
#[derive(Component, Debug)]
pub struct HealthBar {
    pub target: Entity,
    /// Full-hp width in world units (from the style or the default).
    pub width: f32,
}

/// Optional styling a game inserts on a unit BEFORE first damage: bar width
/// and color (e.g. wider + golden for elites/bosses). Absent => defaults.
#[derive(Component, Debug, Clone, Copy)]
pub struct HealthBarStyle {
    pub width: f32,
    pub color: Color,
}

/// Default bar width / color / height offset.
pub const BAR_WIDTH: f32 = 0.9;
pub const BAR_COLOR: Color = Color::srgb(0.3, 0.9, 0.35);
pub const BAR_HEIGHT: f32 = 0.9;

/// Maintain health bars: a bar appears above a unit only once it is damaged
/// (no full-health spam), width == hp/max_hp x style width, hidden again at
/// full hp. Despawns bars whose target died.
pub fn refresh_health_bars(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    units: Query<(Entity, &Hp, &MaxHp, Option<&HealthBarStyle>)>,
    mut bars: Query<(Entity, &HealthBar, &mut Transform, &mut Visibility)>,
) {
    // Ensure every damaged unit has a bar.
    for (uid, hp, max, style) in &units {
        if hp.0 >= max.0 || hp.0 <= 0.0 {
            continue;
        }
        if bars.iter().any(|(_, b, _, _)| b.target == uid) {
            continue;
        }
        let (width, color) = match style {
            Some(s) => (s.width, s.color),
            None => (BAR_WIDTH, BAR_COLOR),
        };
        commands.entity(uid).with_children(|p| {
            p.spawn((
                HealthBar { target: uid, width },
                Mesh3d(meshes.add(Cuboid::new(1.0, 0.07, 0.05))),
                MeshMaterial3d(materials.add(StandardMaterial {
                    base_color: color,
                    unlit: true,
                    ..default()
                })),
                Visibility::Hidden,
                Transform::from_xyz(0.0, BAR_HEIGHT, 0.0),
            ));
        });
    }
    // Sync visibility + width; drop bars whose target is gone.
    for (bid, bar, mut tf, mut vis) in &mut bars {
        let Ok((_, hp, max, _)) = units.get(bar.target) else {
            commands.entity(bid).despawn();
            continue;
        };
        let damaged = hp.0 < max.0 && hp.0 > 0.0;
        *vis = if damaged { Visibility::Visible } else { Visibility::Hidden };
        if damaged {
            tf.scale.x = (hp.0 / max.0).max(0.01) * bar.width;
        }
    }
}

// --- Hit flash (warden FX2) -------------------------------------------------------

/// A white flash on a damaged unit; expires back to the original color.
#[derive(Component, Debug)]
pub struct HitFlash {
    pub timer: f32,
    pub original: Option<Color>,
}

/// The hit flash window (shared by every injector).
pub const HIT_FLASH_SECS: f32 = 0.15;

/// Tick hit flashes: capture the original color once, flash white, restore.
pub fn tick_hit_flash(
    time: Res<Time>,
    mut commands: Commands,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut q: Query<(Entity, &mut HitFlash, &MeshMaterial3d<StandardMaterial>)>,
) {
    for (eid, mut flash, mat) in &mut q {
        flash.timer -= time.delta_secs();
        if let Some(mut m) = materials.get_mut(&mat.0) {
            if flash.original.is_none() {
                flash.original = Some(m.base_color);
                m.base_color = Color::srgb(1.0, 1.0, 0.95);
            }
            if flash.timer <= 0.0 {
                if let Some(orig) = flash.original {
                    m.base_color = orig;
                }
                commands.entity(eid).remove::<HitFlash>();
            }
        }
    }
}

// --- Blast rings (warden FX2) -----------------------------------------------------

/// Observability message for area detonations: one per resolved impact with
/// a blast radius (games consume/emit their own equivalents and forward).
#[derive(Message, Debug, Clone, Copy)]
pub struct FxBlast {
    pub position: Vec3,
    pub radius: f32,
}

pub const BLAST_RING_SECS: f32 = 0.5;

/// Marker on an expanding AOE blast ring.
#[derive(Component, Debug)]
pub struct BlastRing {
    pub timer: f32,
    pub max_radius: f32,
}

/// On every blast message, spawn an expanding ring at the point.
pub fn spawn_blast_rings(
    mut blasts: MessageReader<FxBlast>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for blast in blasts.read() {
        if blast.radius <= 0.0 {
            continue;
        }
        commands.spawn((
            BlastRing {
                timer: BLAST_RING_SECS,
                max_radius: blast.radius,
            },
            Mesh3d(meshes.add(Circle::new(1.0))),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: Color::srgba(1.0, 0.7, 0.25, 0.6),
                unlit: true,
                alpha_mode: AlphaMode::Blend,
                ..default()
            })),
            Visibility::Visible,
            Transform::from_xyz(blast.position.x, 0.1, blast.position.z)
                .with_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)),
        ));
    }
}

/// Expand + fade blast rings, despawn after BLAST_RING_SECS.
pub fn tick_blast_rings(
    time: Res<Time>,
    mut commands: Commands,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut q: Query<(Entity, &mut BlastRing, &mut Transform, &MeshMaterial3d<StandardMaterial>)>,
) {
    for (eid, mut ring, mut tf, mat) in &mut q {
        ring.timer -= time.delta_secs();
        let t = 1.0 - (ring.timer / BLAST_RING_SECS).clamp(0.0, 1.0); // 0->1
        let r = ring.max_radius * (0.2 + 0.8 * t);
        tf.scale = Vec3::splat(r);
        if let Some(mut m) = materials.get_mut(&mat.0) {
            m.base_color.set_alpha(0.6 * (1.0 - t));
        }
        if ring.timer <= 0.0 {
            commands.entity(eid).despawn();
        }
    }
}

// --- Death shells (warden FX2) ----------------------------------------------------

pub const DEATH_SHELL_SECS: f32 = 0.4;

/// Marker on a death shell (pure visual; the logic despawn already happened).
#[derive(Component, Debug)]
pub struct DeathShell {
    pub timer: f32,
}

/// Spawn a dissolving shell for a dying unit. Called right before the
/// despawn — rewards and timing stay in game logic; this only adds the
/// visual.
pub fn spawn_death_shell(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    tf: &Transform,
    color: Color,
) {
    commands.spawn((
        DeathShell {
            timer: DEATH_SHELL_SECS,
        },
        Mesh3d(meshes.add(Cuboid::new(0.7, 0.7, 0.7))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: color,
            unlit: true,
            alpha_mode: AlphaMode::Blend,
            ..default()
        })),
        Visibility::Visible,
        Transform::from_translation(tf.translation),
    ));
}

/// Shrink + fade death shells, despawn after DEATH_SHELL_SECS.
pub fn tick_death_shells(
    time: Res<Time>,
    mut commands: Commands,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut q: Query<(Entity, &mut DeathShell, &mut Transform, &MeshMaterial3d<StandardMaterial>)>,
) {
    for (eid, mut shell, mut tf, mat) in &mut q {
        shell.timer -= time.delta_secs();
        let t = (shell.timer / DEATH_SHELL_SECS).clamp(0.0, 1.0); // 1->0
        tf.scale = Vec3::splat(t.max(0.05));
        if let Some(mut m) = materials.get_mut(&mat.0) {
            m.base_color.set_alpha(t);
        }
        if shell.timer <= 0.0 {
            commands.entity(eid).despawn();
        }
    }
}

// --- Damage floats (FXP, round-3 plan Phase 1) ------------------------------------

/// Float lifetime in seconds (fade window).
pub const DAMAGE_FLOAT_TTL: f32 = 1.2;
/// Rise speed in world units per second.
pub const DAMAGE_FLOAT_RISE: f32 = 0.8;
/// On-screen pool cap; overflow drops the OLDEST float.
pub const DAMAGE_FLOAT_POOL: usize = 32;

// Color/size tiers mirror the boarded Plan-A tokens (docs/ui-design-brief.md
// §5). The fx feature must not depend on the ui feature (they gate
// independently), so the token VALUES are copied here, digit for digit:
// normal = foreground (plan-A text_primary), crit = destructive (the WC3
// semantic enemy red from §5.2). Heal floats are intentionally NOT spawned:
// the base ships no resolved-heal message (HealRequest is pre-resolution —
// it no-ops on dead/full-hp targets), so consuming it would misreport;
// revisit when a HealDealt-style settlement message lands.
pub const FLOAT_COLOR: Color = Color::srgb(0.98, 0.95, 0.86);
pub const FLOAT_CRIT_COLOR: Color = Color::srgb(0.85, 0.20, 0.20);
pub const FLOAT_FONT_SIZE: f32 = 17.0;
pub const FLOAT_CRIT_FONT_SIZE: f32 = 24.0;

/// The color/size tier for one float (plan-A tokens, see the consts above).
pub fn float_style(crit: bool) -> (Color, f32) {
    if crit {
        (FLOAT_CRIT_COLOR, FLOAT_CRIT_FONT_SIZE)
    } else {
        (FLOAT_COLOR, FLOAT_FONT_SIZE)
    }
}

/// Pure logic data for one floating damage number (FXP): spawned by
/// [`damage_floats`] from a `DamageDealt` message; the render pass decorates
/// the same entity with the UI Text bundle (rise + fade). `crit` rides the
/// data (not just the message) so the render tier stays derivable.
#[derive(Component, Debug, Clone)]
pub struct DamageFloat {
    /// Rounded number text, e.g. "37".
    pub text: String,
    /// World position captured at hit time (the target may move/die after).
    pub world_pos: Vec3,
    pub age: f32,
    pub ttl: f32,
    /// Monotonic spawn ordinal ([`FloatSeq`]) — the pool's drop order.
    pub seq: u64,
    /// Crit tier flag (destructive color + larger size).
    pub crit: bool,
}

/// Monotonic spawn counter for damage floats: pool overflow drops the
/// smallest `seq` (the true oldest by SPAWN order — independent of system
/// execution order and query iteration order, both of which are
/// nondeterministic across frames; age alone can tie whole batches).
#[derive(Resource, Debug, Default)]
pub struct FloatSeq(pub u64);

/// Marks a DamageFloat already carrying its UI Text decoration.
#[derive(Component, Debug)]
pub struct FloatTextUi;

/// Cleanup: one `DamageDealt` => exactly one `DamageFloat` (rounded text,
/// position captured from the target's Transform — ZERO for bare test
/// entities, same Option-tolerant convention as resolve_damage). Runs in
/// `WarbaseSet::Cleanup` BEFORE `cleanup_dead` so dying targets still have
/// positions; overflow past [`DAMAGE_FLOAT_POOL`] drops the float with the
/// smallest spawn `seq` (deterministic, D6 — no reliance on execution or
/// query iteration order).
pub fn damage_floats(
    mut commands: Commands,
    mut dealt: MessageReader<crate::combat::DamageDealt>,
    mut seq: ResMut<FloatSeq>,
    units: Query<&Transform>,
    floats: Query<(Entity, &DamageFloat)>,
) {
    // Spawns queued this frame are invisible to `floats` (deferred), so the
    // live pool is tracked locally: (entity, seq).
    let mut live: Vec<(Entity, u64)> = floats.iter().map(|(e, f)| (e, f.seq)).collect();
    for msg in dealt.read() {
        let pos = units.get(msg.target).map(|t| t.translation).unwrap_or_default();
        seq.0 += 1;
        let id = commands
            .spawn(DamageFloat {
                text: format!("{}", msg.amount.round() as i64),
                world_pos: pos,
                age: 0.0,
                ttl: DAMAGE_FLOAT_TTL,
                seq: seq.0,
                crit: msg.crit,
            })
            .id();
        live.push((id, seq.0));
        while live.len() > DAMAGE_FLOAT_POOL {
            // Strictly-less keeps the FIRST minimal entry: same-batch seq
            // ties are impossible (counter), so this is exactly the oldest.
            let mut oldest = 0;
            for i in 1..live.len() {
                if live[i].1 < live[oldest].1 {
                    oldest = i;
                }
            }
            let (drop, _) = live.remove(oldest);
            commands.entity(drop).despawn();
        }
    }
}

/// Age floats; despawn past ttl (half-frame tolerance, the house convention
/// for exact frame counts at fixed steps).
pub fn tick_damage_floats(
    time: Res<Time>,
    mut commands: Commands,
    mut floats: Query<(Entity, &mut DamageFloat)>,
) {
    let dt = time.delta_secs();
    for (e, mut f) in &mut floats {
        f.age += dt;
        if f.age + dt * 0.5 >= f.ttl {
            commands.entity(e).despawn();
        }
    }
}

/// Render: decorate fresh DamageFloats with the floating Text (tier styling
/// from `crit`). Pure component insertion — headless apps carry the
/// components inert (the same contract the command-card panel uses).
pub fn render_float_texts(
    mut commands: Commands,
    font: Res<FxFont>,
    floats: Query<(Entity, &DamageFloat), Without<FloatTextUi>>,
) {
    for (e, f) in &floats {
        let (color, size) = float_style(f.crit);
        commands.entity(e).insert((
            FloatTextUi,
            Node {
                position_type: PositionType::Absolute,
                ..default()
            },
            Text::new(f.text.clone()),
            TextFont {
                font: font.source(),
                font_size: FontSize::Px(size),
                ..default()
            },
            TextColor(color),
        ));
    }
}

/// Render: floats rise [`DAMAGE_FLOAT_RISE`] units/s and fade out over ttl;
/// screen position comes from the camera (no camera => positioning skipped,
/// headless apps keep the default anchor).
pub fn refresh_float_texts(
    cameras: Query<(&Camera, &GlobalTransform)>,
    mut floats: Query<(&DamageFloat, &mut Node, &mut TextColor), With<FloatTextUi>>,
) {
    let Some((cam, cam_tf)) = cameras.iter().next() else { return };
    for (f, mut node, mut color) in &mut floats {
        let risen =
            f.world_pos + Vec3::new(0.0, DAMAGE_FLOAT_RISE * f.age, 0.0);
        if let Ok(p) = cam.world_to_viewport(cam_tf, risen) {
            node.left = Val::Px(p.x - 9.0);
            node.top = Val::Px(p.y);
        }
        let t = (f.age / f.ttl).clamp(0.0, 1.0);
        let (base, _) = float_style(f.crit);
        color.0 = base.with_alpha(1.0 - t);
    }
}

// --- Status markers (FXP): Stunned/Silenced diamonds + aura rings ------------------

/// Height above the unit origin where the status diamond floats.
pub const STATUS_MARKER_OFFSET: f32 = 1.7;

// Token mirrors (same rationale as the float consts): stun = warning gold,
// silence = mana blue (brief §5.2 semantic colors).
pub const MARKER_STUN_COLOR: Color = Color::srgb(1.0, 0.80, 0.20);
pub const MARKER_SILENCE_COLOR: Color = Color::srgb(0.25, 0.50, 1.00);
/// Aura-carrier ground ring: plan-A gold accent, thin.
pub const AURA_RING_COLOR: Color = Color::srgba(1.0, 0.80, 0.28, 0.75);

/// The glyph + diamond color for one status kind (晕 = stun, 静 = silence).
pub fn marker_style(kind: crate::skill::StatusKind) -> (Color, &'static str) {
    match kind {
        crate::skill::StatusKind::Stun => (MARKER_STUN_COLOR, "晕"),
        crate::skill::StatusKind::Silence => (MARKER_SILENCE_COLOR, "静"),
    }
}

/// Pure logic data for one overhead status diamond (FXP). Maintained by
/// [`status_markers`]; the render pass decorates the entity with the diamond
/// (rotated square) + CJK glyph. `target` + `kind` identify it uniquely.
#[derive(Component, Debug)]
pub struct StatusMarker {
    pub target: Entity,
    pub kind: crate::skill::StatusKind,
    /// World-space height above the target's origin.
    pub offset: f32,
}

/// Marks a StatusMarker already decorated.
#[derive(Component, Debug)]
pub struct StatusMarkerUi;

/// Tick: keep exactly one marker per (target, kind) while the status holds;
/// removal (expiry/cleanse/despawn) drops the marker the same frame. Runs
/// after `tick_skill_cooldowns` so a stun landing this frame is already
/// visible — markers appear within one frame of the status.
pub fn status_markers(
    mut commands: Commands,
    stunned: Query<(Entity, &crate::skill::Stunned)>,
    silenced: Query<(Entity, &crate::skill::Silenced)>,
    markers: Query<(Entity, &StatusMarker)>,
) {
    // Drop markers whose status is gone (expired, cleansed, target despawned).
    for (me, m) in &markers {
        let still = match m.kind {
            crate::skill::StatusKind::Stun => stunned.get(m.target).is_ok(),
            crate::skill::StatusKind::Silence => silenced.get(m.target).is_ok(),
        };
        if !still {
            commands.entity(me).despawn();
        }
    }
    // Ensure one marker per live status (stun and silence stack side by side).
    let ensure = |ue: Entity,
                      kind: crate::skill::StatusKind,
                      markers: &Query<(Entity, &StatusMarker)>,
                      commands: &mut Commands| {
        if markers.iter().any(|(_, m)| m.target == ue && m.kind == kind) {
            return;
        }
        commands.spawn(StatusMarker { target: ue, kind, offset: STATUS_MARKER_OFFSET });
    };
    for (ue, _) in &stunned {
        ensure(ue, crate::skill::StatusKind::Stun, &markers, &mut commands);
    }
    for (ue, _) in &silenced {
        ensure(ue, crate::skill::StatusKind::Silence, &markers, &mut commands);
    }
}

/// Render: decorate fresh markers — a rotated-square diamond with the CJK
/// glyph counter-rotated upright inside.
pub fn render_status_markers(
    mut commands: Commands,
    font: Res<FxFont>,
    markers: Query<(Entity, &StatusMarker), Without<StatusMarkerUi>>,
) {
    for (e, m) in &markers {
        let (color, glyph) = marker_style(m.kind);
        commands
            .entity(e)
            .insert((
                StatusMarkerUi,
                Node {
                    position_type: PositionType::Absolute,
                    width: Val::Px(20.0),
                    height: Val::Px(20.0),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    border: UiRect::all(Val::Px(1.0)),
                    ..default()
                },
                UiTransform::from_rotation(Rot2::degrees(45.0)),
                BackgroundColor(color.with_alpha(0.22)),
                BorderColor::all(color),
            ))
            .with_children(|p| {
                p.spawn((
                    UiTransform::from_rotation(Rot2::degrees(-45.0)),
                    Text::new(glyph),
                    TextFont {
                        font: font.source(),
                        font_size: FontSize::Px(11.0),
                        ..default()
                    },
                    TextColor(color),
                ));
            });
    }
}

/// Render: markers follow their target's screen position (offset above the
/// head; stun sits left, silence right, so a doubly-afflicted unit shows
/// both). No camera => positioning skipped.
pub fn refresh_status_markers(
    cameras: Query<(&Camera, &GlobalTransform)>,
    units: Query<&GlobalTransform>,
    mut markers: Query<(&StatusMarker, &mut Node), With<StatusMarkerUi>>,
) {
    let Some((cam, cam_tf)) = cameras.iter().next() else { return };
    for (m, mut node) in &mut markers {
        let Ok(unit_tf) = units.get(m.target) else { continue };
        let pos = unit_tf.translation() + Vec3::new(0.0, m.offset, 0.0);
        let Ok(p) = cam.world_to_viewport(cam_tf, pos) else { continue };
        let side = match m.kind {
            crate::skill::StatusKind::Stun => -14.0,
            crate::skill::StatusKind::Silence => 14.0,
        };
        node.left = Val::Px(p.x + side - 10.0);
        node.top = Val::Px(p.y - 10.0);
    }
}

/// Thin ground ring under an aura carrier (FXP): the ring's radius is the
/// carrier's largest aura radius, so it reads as the aura's reach. A child of
/// the unit (follows movement, despawns with the unit); dropped when the
/// `Auras` component goes away.
#[derive(Component, Debug, Clone, Copy)]
pub struct AuraRing {
    pub target: Entity,
}

/// Tick: keep one ring child per `Auras` carrier; drop rings whose carrier
/// lost its auras (or despawned — child cleanup is automatic).
pub fn aura_rings(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    carriers: Query<(Entity, &crate::buff::Auras)>,
    rings: Query<(Entity, &AuraRing)>,
) {
    for (re, ring) in &rings {
        if carriers.get(ring.target).is_err() {
            commands.entity(re).despawn();
        }
    }
    for (ue, auras) in &carriers {
        if rings.iter().any(|(_, r)| r.target == ue) {
            continue;
        }
        let radius = auras.0.iter().map(|a| a.radius).fold(0.6, f32::max);
        commands.entity(ue).with_children(|p| {
            p.spawn((
                AuraRing { target: ue },
                Mesh3d(meshes.add(Torus::new((radius - 0.04).max(0.05), radius))),
                MeshMaterial3d(materials.add(StandardMaterial {
                    base_color: AURA_RING_COLOR,
                    unlit: true,
                    alpha_mode: AlphaMode::Blend,
                    ..default()
                })),
                Visibility::Visible,
                Transform::from_xyz(0.0, 0.05, 0.0)
                    .with_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)),
            ));
        });
    }
}

// --- Kill bursts (FXP): short pulse ring on UnitDied -------------------------------

pub const KILL_BURST_SECS: f32 = 0.3;
/// Outer radius the pulse reaches (world units).
pub const KILL_BURST_RADIUS: f32 = 0.9;
pub const KILL_BURST_COLOR: Color = Color::srgba(1.0, 0.85, 0.30, 0.9);

/// Short pulse ring over a kill (FXP): layered on the death-shell family —
/// games keep spawning their shells; this reads `UnitDied` directly so the
/// base itself always has kill feedback.
#[derive(Component, Debug)]
pub struct KillBurst {
    pub timer: f32,
    pub max_radius: f32,
}

/// Cleanup: one pulse per `UnitDied`, at the dying body's last position.
/// Runs BEFORE `cleanup_dead` (the transform must still exist); a message
/// whose body is already gone is skipped silently.
pub fn spawn_kill_bursts(
    mut commands: Commands,
    mut deaths: MessageReader<crate::combat::UnitDied>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    units: Query<&Transform>,
) {
    for d in deaths.read() {
        let Ok(tf) = units.get(d.entity) else { continue };
        // Unit-radius mesh; the tick scales it out to max_radius.
        commands.spawn((
            KillBurst { timer: KILL_BURST_SECS, max_radius: KILL_BURST_RADIUS },
            Mesh3d(meshes.add(Torus::new(0.94, 1.0))),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: KILL_BURST_COLOR,
                unlit: true,
                alpha_mode: AlphaMode::Blend,
                ..default()
            })),
            Visibility::Visible,
            Transform::from_xyz(tf.translation.x, 0.08, tf.translation.z)
                .with_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)),
        ));
    }
}

/// Expand + fade kill bursts, despawn after [`KILL_BURST_SECS`].
pub fn tick_kill_bursts(
    time: Res<Time>,
    mut commands: Commands,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut q: Query<(Entity, &mut KillBurst, &mut Transform, &MeshMaterial3d<StandardMaterial>)>,
) {
    for (eid, mut burst, mut tf, mat) in &mut q {
        burst.timer -= time.delta_secs();
        let t = 1.0 - (burst.timer / KILL_BURST_SECS).clamp(0.0, 1.0); // 0->1
        tf.scale = Vec3::splat(burst.max_radius * (0.3 + 0.7 * t));
        if let Some(mut m) = materials.get_mut(&mat.0) {
            m.base_color.set_alpha(0.9 * (1.0 - t));
        }
        if burst.timer <= 0.0 {
            commands.entity(eid).despawn();
        }
    }
}

// --- FXP font seam -----------------------------------------------------------------

/// CJK-capable font for FX text (FXP). bevy's default font has no CJK
/// glyphs, so the status-marker glyphs (晕/静) would render as tofu without
/// it. Injection seam: the GAME resolves a handle (loop-td reuses its shared
/// NotoSansSC) and inserts this resource; `None` (the default) = bevy's
/// default font — zero drift for apps that never set it.
#[derive(Resource, Debug, Default, Clone)]
pub struct FxFont(pub Option<Handle<Font>>);

impl FxFont {
    /// The `TextFont` source this resource resolves to.
    pub fn source(&self) -> bevy::text::FontSource {
        match &self.0 {
            Some(h) => bevy::text::FontSource::Handle(h.clone()),
            None => bevy::text::FontSource::default(),
        }
    }
}

pub struct FxPlugin;

impl Plugin for FxPlugin {
    fn build(&self, app: &mut App) {
        // Headless apps (tests/harness) carry no RenderPlugin, but the FX
        // systems take the asset stores as params — init them (idempotent;
        // real renders already provide their own).
        app.init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<StandardMaterial>>()
            .init_resource::<FxFont>()
            .init_resource::<FloatSeq>()
            .add_message::<FxBlast>()
            .add_systems(
                Update,
                (
                    refresh_health_bars,
                    tick_hit_flash,
                    spawn_blast_rings,
                    tick_blast_rings,
                    tick_death_shells,
                ),
            )
            // FXP: status markers/aura rings track the Tick-phase statuses —
            // after tick_skill_cooldowns so a stun landing this frame (a
            // deferred insert) is flushed and visible same-frame.
            .add_systems(
                Update,
                (status_markers, aura_rings)
                    .chain()
                    .in_set(WarbaseSet::Tick)
                    .after(crate::skill::tick_skill_cooldowns),
            )
            // FXP: damage floats + kill bursts read Resolve-phase messages
            // and need the dying bodies' Transforms — before cleanup_dead
            // despawns them.
            .add_systems(
                Update,
                (damage_floats, spawn_kill_bursts)
                    .in_set(WarbaseSet::Cleanup)
                    .before(crate::combat::cleanup_dead),
            )
            // FXP: aging/decoration/refresh passes (plain Update; order
            // among themselves only, decoration lags at most a frame).
            .add_systems(
                Update,
                (
                    tick_damage_floats,
                    render_float_texts,
                    refresh_float_texts,
                    render_status_markers,
                    refresh_status_markers,
                    tick_kill_bursts,
                )
                    .chain(),
            );
    }
}
