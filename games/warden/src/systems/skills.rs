//! Commander skills, phase 1 (SK1 framework; requirements §17).
//! Three global skills — meteor rain (SK2) / frost nova (SK3) / war frenzy
//! (SK4) — all free, purely cooldown-gated:
//!   - cooldown ticks with virtual game time in BOTH phases, never resets
//!     across waves, starts ready, freezes on pause (state-gated system +
//!     zero virtual delta under the debug freeze);
//!   - point skills enter target picking (indicator ring → left-click
//!     confirms, Esc/right-click cancels — same interaction shape as fusion
//!     target picking, §17.3); instant skills cast on the keypress;
//!   - buttons live in the command-bar CONTROL zone (right), never in the
//!     QWER/ASDF command grid (§16.7 gave those keys to build); hotkeys are
//!     Z/X/C with the letter printed on the corner badge.
//! Skills are castable in build AND combat phases (they are not building,
//! §17.1); the "lock building" iron law is untouched.

use bevy::input::mouse::MouseButton;
use bevy::prelude::*;
use bevy::window::Window;

use crate::resources::{
    BuildMode, DebugPanelCapture, FRENZY_DURATION, FRENZY_SPEED_MULT, Lang, SkillBalance,
    SkillDefs, SkillId, SkillKind, SkillState, SkillTargeting, SkillTargetMode, SkillAiming,
    PROJECTILE_SPEED, SKILL_COOLDOWNS,
};
use crate::states::GameState;
use crate::components::{AttackType, Enemy, Projectile, ProjectileOrigin, SkillCaster, SkillSlot};
use crate::systems::command::{cursor_over_blocking_ui, BlocksWorldInput, ControlZoneRoot};
use crate::systems::hud::UiFont;
use crate::systems::l10n;

/// How a phase-1 commander skill is aimed (§17.3). Superseded by the
/// generic `SkillTargetMode` as of SK5; stays until SK8 migrates the
/// commander path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommanderSkillAiming {
    /// Pick a ground point (indicator ring + left-click confirm).
    Point,
    /// No targeting — fires on the keypress.
    Instant,
}

/// One phase-1 commander skill's framework data (superseded by the generic
/// `resources::SkillDef` registry as of SK5; this stays until SK8 migrates
/// the commander path onto the framework). `implemented` flips on per skill
/// as SK2-4 land; an unimplemented skill shows greyed and can never cast.
pub struct CommanderSkillDef {
    pub id: SkillId,
    pub aiming: CommanderSkillAiming,
    pub implemented: bool,
    pub hotkey: KeyCode,
    pub badge: char,
    /// Indicator ring radius for point skills (the skill's effect radius).
    pub indicator_radius: f32,
}

/// The three skills in button order (Z/X/C, §17.3). Numbers live in
/// `resources::SKILL_COOLDOWNS` and the l10n tooltip.
pub const COMMANDER_SKILL_DEFS: [CommanderSkillDef; 3] = [
    CommanderSkillDef {
        id: SkillId::MeteorRain,
        aiming: CommanderSkillAiming::Point,
        implemented: true, // SK2
        hotkey: KeyCode::KeyZ,
        badge: 'Z',
        indicator_radius: 3.0,
    },
    CommanderSkillDef {
        id: SkillId::FrostNova,
        aiming: CommanderSkillAiming::Point,
        implemented: true, // SK3
        hotkey: KeyCode::KeyX,
        badge: 'X',
        indicator_radius: 5.0,
    },
    CommanderSkillDef {
        id: SkillId::WarFrenzy,
        aiming: CommanderSkillAiming::Instant,
        implemented: true, // SK4
        hotkey: KeyCode::KeyC,
        badge: 'C',
        indicator_radius: 0.0,
    },
];

/// Framework definition of a phase-1 commander skill id.
pub fn commander_skill_def(id: SkillId) -> &'static CommanderSkillDef {
    COMMANDER_SKILL_DEFS
        .iter()
        .find(|d| d.id == id)
        .expect("commander skill def")
}

/// Dispatch one CONFIRMED cast to its effect (the cooldown is started by the
/// caller — a cancelled target pick must never pay). Arms are filled in by
/// SK2/SK3/SK4; the framework keeps them as seams.
fn cast_skill(
    commands: &mut Commands,
    enemies: &Query<(Entity, &Transform), Without<SkillIndicatorRing>>,
    frenzy: &mut crate::resources::WarFrenzy,
    skill: SkillId,
    point: Vec3,
) {
    match skill {
        SkillId::MeteorRain => {
            // SK2: schedule the three-drop volley at the confirmed point.
            commands.spawn(MeteorVolley {
                point,
                shots_left: METEOR_COUNT,
                timer: 0.0,
            });
        }
        SkillId::FrostNova => {
            // SK3: everything within NOVA_RADIUS (XZ distance — effects are
            // ground-based) takes the nova slow via the shared merge path.
            let mut hits = 0u32;
            for (eid, tf) in enemies.iter() {
                let dx = tf.translation.x - point.x;
                let dz = tf.translation.z - point.z;
                if dx * dx + dz * dz <= NOVA_RADIUS * NOVA_RADIUS {
                    crate::systems::tower::apply_slow(
                        commands,
                        eid,
                        NOVA_SLOW_FACTOR,
                        NOVA_SLOW_DURATION,
                    );
                    hits += 1;
                }
            }
            info!("[sk] frost nova at ({:.1}, {:.1}) slowed {hits} enemies", point.x, point.z);
        }
        SkillId::WarFrenzy => {
            // SK4: global timed attack-speed burst (towers read it at fire
            // time; the point is meaningless for an instant cast).
            frenzy.remaining = FRENZY_DURATION;
            info!(
                "[sk] war frenzy: all towers x{FRENZY_SPEED_MULT} for {FRENZY_DURATION}s"
            );
        }
    }
}

/// SK4: tick the frenzy countdown with virtual game time (frozen on pause
/// like every timer).
pub fn tick_war_frenzy(time: Res<Time>, mut frenzy: ResMut<crate::resources::WarFrenzy>) {
    if frenzy.remaining > 0.0 {
        frenzy.remaining = (frenzy.remaining - time.delta_secs()).max(0.0);
    }
}

// --- SK3: frost nova ---------------------------------------------------------------

/// SK3 starting values (§17.2; tune via §14): slow to 50% for 2s in radius 5.
pub const NOVA_RADIUS: f32 = 5.0;
pub const NOVA_SLOW_FACTOR: f32 = 0.5;
pub const NOVA_SLOW_DURATION: f32 = 2.0;

// --- SK2: meteor rain ------------------------------------------------------------

/// One confirmed meteor-rain cast: a rule-layer scheduler entity dropping the
/// remaining meteors at `point`, spaced by METEOR_INTERVAL.
#[derive(Component)]
pub struct MeteorVolley {
    pub point: Vec3,
    pub shots_left: u32,
    pub timer: f32,
}

/// SK2 starting values (§17.2; tune via §14).
pub const METEOR_COUNT: u32 = 3;
pub const METEOR_INTERVAL: f32 = 0.3;
pub const METEOR_DAMAGE: f32 = 40.0;
pub const METEOR_RADIUS: f32 = 3.0;
/// Meteors spawn this far above the point and fall at PROJECTILE_SPEED
/// (10 u/s -> ~1s of visible flight before impact).
pub const METEOR_FALL_HEIGHT: f32 = 10.0;

/// Release the pending meteor drops (SK2): every METEOR_INTERVAL one rule-layer
/// projectile launches from above the point, flying straight down to it — the
/// fixed-point origin seam of `Projectile` (§17.2 发起方=技能). Each impact
/// resolves 40 MAGIC damage to everything within METEOR_RADIUS (armor per
/// victim: magic pierces physical armor, §6).
pub fn release_meteor_shots(
    time: Res<Time>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut volleys: Query<(Entity, &mut MeteorVolley)>,
) {
    for (eid, mut volley) in &mut volleys {
        if volley.shots_left == 0 {
            commands.entity(eid).despawn();
            continue;
        }
        volley.timer -= time.delta_secs();
        if volley.timer > 0.0 {
            continue;
        }
        let start = volley.point + Vec3::Y * METEOR_FALL_HEIGHT;
        commands.spawn((
            Projectile {
                origin: ProjectileOrigin::Fixed {
                    point: volley.point,
                },
                damage: METEOR_DAMAGE,
                attack_type: AttackType::Magic,
                // Legacy SkillId index until SK8 registers the commander
                // skills in the SkillDefs registry.
                source: crate::systems::damage::DamageSource::CommanderSkill(
                    SkillId::MeteorRain.index(),
                ),
                aoe_radius: METEOR_RADIUS,
                slow_factor: 0.0,
                slow_duration: 0.0,
                speed: PROJECTILE_SPEED,
                last_target_pos: volley.point,
            },
            Mesh3d(meshes.add(Sphere::new(0.3))),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: Color::srgb(1.0, 0.45, 0.1),
                unlit: true,
                ..default()
            })),
            Transform::from_translation(start),
        ));
        volley.shots_left -= 1;
        volley.timer = METEOR_INTERVAL;
        info!(
            "[sk] meteor {} of {} away -> ({:.1}, {:.1})",
            METEOR_COUNT - volley.shots_left,
            METEOR_COUNT,
            volley.point.x,
            volley.point.z
        );
        if volley.shots_left == 0 {
            commands.entity(eid).despawn();
        }
    }
}

/// Tick every cooldown down with virtual game time (SK1). Runs only in
/// `Playing`, so the player pause freezes it; the DB3 debug freeze zeroes
/// the delta for the same effect. Crosses wave boundaries untouched.
pub fn tick_skill_cooldowns(time: Res<Time>, mut skills: ResMut<SkillState>) {
    for cd in skills.cooldowns.iter_mut() {
        if *cd > 0.0 {
            *cd = (*cd - time.delta_secs()).max(0.0);
        }
    }
}

// --- UI: control-zone skill buttons (right zone, §17.3) -------------------------

/// Marker on the skill button row (a child of the command-bar control zone;
/// re-attached automatically when the zone rebuilds on language change).
#[derive(Component)]
pub struct SkillsRow;

/// One skill button in the control zone.
#[derive(Component)]
pub struct SkillButton {
    pub skill: SkillId,
}

/// Marker on a skill button's main label (cooldown seconds / name).
#[derive(Component)]
pub struct SkillButtonText;

/// Z/X/C hotkeys (§17.3): press the matching button by injecting
/// `Interaction::Pressed` — clicks and keys then share one handler (same
/// pattern as the UI12 grid hotkeys).
pub fn skill_hotkeys(
    keys: Res<ButtonInput<KeyCode>>,
    panel: Res<DebugPanelCapture>,
    buttons: Query<(Entity, &SkillButton)>,
    mut commands: Commands,
) {
    // Typing into a debug-panel field must not fire skill hotkeys (DB1).
    if panel.keyboard {
        return;
    }
    for def in COMMANDER_SKILL_DEFS.iter() {
        if !keys.just_pressed(def.hotkey) {
            continue;
        }
        if let Some((e, _)) = buttons.iter().find(|(_, b)| b.skill == def.id) {
            commands.entity(e).insert(Interaction::Pressed);
        }
    }
}

/// Button clicks / injected hotkey presses (SK1). A skill only responds when
/// implemented AND off cooldown; point skills enter target picking (disarming
/// any pending build — the skill owns the next click), instant skills cast
/// right away.
pub fn handle_skill_buttons(
    mut commands: Commands,
    mut q: Query<(&Interaction, &SkillButton), Changed<Interaction>>,
    enemies: Query<(Entity, &Transform), Without<SkillIndicatorRing>>,
    mut targeting: ResMut<SkillTargeting>,
    mut aiming: ResMut<crate::resources::SkillAiming>,
    mut build: ResMut<BuildMode>,
    mut skills: ResMut<SkillState>,
    mut frenzy: ResMut<crate::resources::WarFrenzy>,
    state: Res<State<GameState>>,
) {
    if *state.get() != GameState::Playing {
        return;
    }
    for (inter, btn) in &mut q {
        if *inter != Interaction::Pressed {
            continue;
        }
        let skill = btn.skill;
        let def = commander_skill_def(skill);
        if !def.implemented || !skills.ready(skill) {
            continue;
        }
        match def.aiming {
            CommanderSkillAiming::Point => {
                build.armed = false; // a cast replaces a pending build arm
                aiming.0 = None; // transitional cross-clear, dies with SK8
                targeting.0 = Some(skill);
                info!("[sk] {:?} target picking", skill);
            }
            CommanderSkillAiming::Instant => {
                cast_skill(&mut commands, &enemies, &mut frenzy, skill, Vec3::ZERO);
                skills.start_cooldown(skill);
                info!("[sk] {:?} instant cast, cd {:.0}s", skill, SKILL_COOLDOWNS[skill.index()]);
            }
        }
    }
}

/// Ground point under the cursor (y=0 plane), or None without a
/// projectable camera / cursor.
fn cursor_ground_point(
    windows: &Query<&Window>,
    camera: &Query<(&Camera, &GlobalTransform)>,
) -> Option<Vec3> {
    let (cam, cam_tf) = camera.single().ok()?;
    let cursor = windows.single().ok().and_then(|w| w.cursor_position())?;
    let ray = cam.viewport_to_world(cam_tf, cursor).ok()?;
    if ray.direction.y.abs() < 1e-6 {
        return None; // parallel to the ground plane
    }
    let t = -ray.origin.y / ray.direction.y;
    if t < 0.0 {
        return None; // the ground is behind the camera
    }
    Some(ray.origin + *ray.direction * t)
}

/// Marker on the point-skill indicator ring (§17.3 地图指示圈).
#[derive(Component)]
pub struct SkillIndicatorRing;

/// Point-skill target picking (SK1, §17.3): while active, the indicator ring
/// follows the cursor's ground point; a left click away from the UI confirms
/// the cast at that point (cooldown starts, targeting clears). Esc /
/// right-click cancel via `command::cancel_modes` — a cancel never pays the
/// cooldown. Runs BEFORE `select_object` so the confirming click cannot also
/// re-select/deselect (skills own the click, mirroring the fusion-picking
/// guard) and before `handle_skill_buttons` so the click that pressed the
/// skill button cannot confirm itself one frame early.
#[allow(clippy::too_many_arguments)]
pub fn update_skill_targeting(
    mut commands: Commands,
    buttons: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window>,
    camera: Query<(&Camera, &GlobalTransform)>,
    blocking: Query<&bevy::ui::RelativeCursorPosition, With<BlocksWorldInput>>,
    panel: Res<DebugPanelCapture>,
    state: Res<State<GameState>>,
    mut targeting: ResMut<SkillTargeting>,
    mut skills: ResMut<SkillState>,
    mut frenzy: ResMut<crate::resources::WarFrenzy>,
    // Without<SkillIndicatorRing> keeps this read disjoint from the ring
    // write below (B0001; an enemy never carries the ring marker).
    enemies: Query<(Entity, &Transform), Without<SkillIndicatorRing>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    // Without<Enemy> keeps the ring Transform write disjoint from the
    // enemies' Transform read (B0001; a ring is never an enemy).
    mut rings: Query<
        (Entity, &mut Transform, &mut Visibility),
        (With<SkillIndicatorRing>, Without<Enemy>),
    >,
    mut cache: Local<Option<(SkillId, Entity)>>, // (active skill, ring entity)
) {
    // Ring lifecycle: one ring while picking, none otherwise; respawn when
    // the picked skill (radius) changes.
    let want = if *state.get() == GameState::Playing {
        targeting.0
    } else {
        None
    };
    if cache.map_or(true, |(s, _)| Some(s) != want) {
        if let Some((_, ring)) = *cache {
            commands.entity(ring).despawn();
        }
        if let Some(skill) = want {
            let def = commander_skill_def(skill);
            let ring = commands
                .spawn((
                    SkillIndicatorRing,
                    Mesh3d(meshes.add(Circle::new(def.indicator_radius))),
                    MeshMaterial3d(materials.add(StandardMaterial {
                        base_color: Color::srgba(0.95, 0.85, 0.3, 0.4),
                        unlit: true,
                        alpha_mode: AlphaMode::Blend,
                        ..default()
                    })),
                    Visibility::Hidden,
                    Transform::from_translation(Vec3::ZERO)
                        .with_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)),
                ))
                .id();
            *cache = Some((skill, ring));
        } else {
            *cache = None;
        }
    }
    let Some(skill) = want else {
        return;
    };
    // Follow the cursor's ground point.
    let ground = cursor_ground_point(&windows, &camera);
    if let Some((_, ring)) = *cache {
        if let Ok((_, mut tf, mut vis)) = rings.get_mut(ring) {
            match ground {
                Some(p) => {
                    tf.translation = Vec3::new(p.x, 0.09, p.z);
                    *vis = Visibility::Visible;
                }
                None => *vis = Visibility::Hidden,
            }
        }
    }
    // Left click (away from the UI) confirms the cast at the ground point.
    if buttons.just_pressed(MouseButton::Left) && !cursor_over_blocking_ui(&blocking, &panel) {
        if let Some(point) = ground {
            cast_skill(&mut commands, &enemies, &mut frenzy, skill, point);
            skills.start_cooldown(skill);
            targeting.0 = None;
            info!(
                "[sk] {:?} cast at ({:.1}, {:.1}), cd {:.0}s",
                skill,
                point.x,
                point.z,
                SKILL_COOLDOWNS[skill.index()]
            );
        }
    }
}

/// Maintain the three control-zone skill buttons (SK1): spawn/respawn the row
/// when missing or on language change (the control zone rebuilds on toggle
/// and takes the row with it — self-healing), then keep every button's
/// enabled look, label and cooldown seconds live each frame.
#[allow(clippy::too_many_arguments)]
pub fn refresh_skill_buttons(
    mut commands: Commands,
    font: Res<UiFont>,
    lang: Res<Lang>,
    zones: Query<Entity, With<ControlZoneRoot>>,
    rows: Query<Entity, With<SkillsRow>>,
    buttons: Query<(Entity, &SkillButton, &Children)>,
    mut texts: Query<(&mut Text, &mut TextColor), With<SkillButtonText>>,
    mut backgrounds: Query<&mut BackgroundColor, With<SkillButton>>,
    skills: Res<SkillState>,
    mut built_lang: Local<Option<Lang>>,
) {
    // Build when missing (first frame / the control zone rebuilt on language
    // change and took the row with it) or on language change.
    let needs_build = rows.is_empty() || *built_lang != Some(*lang);
    if needs_build {
        for r in &rows {
            commands.entity(r).despawn();
        }
        if let Some(zone) = zones.single().ok() {
            commands.entity(zone).with_children(|row| {
                row.spawn((SkillsRow, Node {
                    flex_direction: FlexDirection::Row,
                    column_gap: Val::Px(4.0),
                    align_items: AlignItems::Center,
                    ..default()
                }))
                .with_children(|row| {
                    for def in COMMANDER_SKILL_DEFS.iter() {
                        row.spawn((
                            Button,
                            SkillButton { skill: def.id },
                            Interaction::None,
                            Node {
                                width: Val::Px(56.0),
                                height: Val::Px(42.0),
                                justify_content: JustifyContent::Center,
                                align_items: AlignItems::Center,
                                ..default()
                            },
                            BackgroundColor(Color::srgb(0.16, 0.16, 0.18)),
                        ))
                        .with_children(|p| {
                            p.spawn((
                                SkillButtonText,
                                Text::new(l10n::skill_name(def.id, *lang)),
                                TextFont {
                                    font: font.0.clone(),
                                    font_size: FontSize::Px(11.0),
                                    ..default()
                                },
                                TextColor(Color::srgb(0.45, 0.45, 0.45)),
                            ));
                            p.spawn((
                                Text::new(def.badge.to_string()),
                                TextFont {
                                    font: font.0.clone(),
                                    font_size: FontSize::Px(9.0),
                                    ..default()
                                },
                                TextColor(Color::srgb(1.0, 0.85, 0.3)),
                                Node {
                                    position_type: PositionType::Absolute,
                                    top: Val::Px(1.0),
                                    right: Val::Px(3.0),
                                    ..default()
                                },
                            ));
                        });
                    }
                });
            });
        }
        *built_lang = Some(*lang);
        return; // visuals settle next frame (deferred spawns)
    }
    // Per-frame: cooldown seconds / enabled look.
    for (eid, btn, children) in buttons.iter() {
        let def = commander_skill_def(btn.skill);
        let remaining = skills.remaining(btn.skill);
        let enabled = def.implemented && remaining <= 0.0;
        if let Ok(mut bg) = backgrounds.get_mut(eid) {
            bg.0 = if enabled {
                Color::srgb(0.45, 0.22, 0.5)
            } else {
                Color::srgb(0.16, 0.16, 0.18)
            };
        }
        let (label, color) = if remaining > 0.0 {
            (format!("{remaining:.0}"), Color::srgb(0.75, 0.75, 0.78))
        } else if !def.implemented {
            (
                l10n::SKILL_WIP.pick(*lang).to_string(),
                Color::srgb(0.45, 0.45, 0.45),
            )
        } else {
            (
                l10n::skill_name(btn.skill, *lang).to_string(),
                Color::srgb(1.0, 1.0, 1.0),
            )
        };
        for child in children {
            if let Ok((mut text, mut tint)) = texts.get_mut(*child) {
                if text.0 != label {
                    text.0 = label.clone();
                }
                if tint.0 != color {
                    tint.0 = color;
                }
            }
        }
    }
}

// --- Generic skill framework (phase 2, ADR-0011; card SK5) ------------------------
//
// Owner-agnostic lifecycle for per-entity skills: validation, cooldown,
// point aiming, learning/leveling. Everything here reads `SkillCaster` and
// the `SkillDefs`/`SkillBalance` tables — never a specific owner type.
// Concrete skill EFFECTS are separate systems that read `SkillCast` and
// match on the def id; the framework never knows what a skill does.

/// A request to cast slot `slot` of entity `caster`. Writers: UI buttons and
/// hotkeys (SK8/SK10), owner-specific autocast triggers (SK9), tests.
#[derive(Message, Debug)]
pub struct CastSkill {
    pub caster: Entity,
    pub slot: usize,
}

/// A cast that passed validation AND aiming — the dispatch seam every skill
/// effect system consumes (ADR-0011: effects are id-dispatched small
/// systems). The cooldown is already running by the time this is read;
/// `point` is Some only for Point-target skills.
#[derive(Message, Debug)]
pub struct SkillCast {
    pub caster: Entity,
    pub def: usize,
    pub level: u8,
    pub point: Option<Vec3>,
}

/// Learn (or re-learn) a skill on a caster at `level` (SK5 API — content
/// cards SK8/SK9 and tests call it). Pure data, no side effects: starts
/// ready, autocast toggle OFF. Re-learning keeps the current cooldown.
pub fn learn_skill(caster: &mut SkillCaster, def: usize, level: u8) {
    if let Some(slot) = caster.slots.iter_mut().find(|s| s.def == def) {
        slot.level = level;
    } else {
        caster.slots.push(SkillSlot {
            def,
            level,
            cooldown: 0.0,
            autocast: false,
        });
    }
}

/// Raise a slot one level, clamped at the def's `max_level` (SK5 API — SK9
/// wires this to tower invest). Returns the (possibly unchanged) level.
pub fn level_up(defs: &SkillDefs, caster: &mut SkillCaster, slot: usize) -> u8 {
    let max = defs.get(caster.slots[slot].def).max_level.max(1);
    let slot = &mut caster.slots[slot];
    slot.level = (slot.level + 1).min(max);
    slot.level
}

/// Validate `CastSkill` requests (SK5). A request passes when the caster
/// still carries its `SkillCaster`, the slot index is in range, the skill
/// is not Passive, and its cooldown is ready. A passing request with no
/// aiming confirms immediately (`SkillCast` + cooldown starts); a Point
/// skill enters `SkillAiming` instead — the cooldown only starts when the
/// aim confirms, so a cancelled pick never pays (the SK1 law). `Unit`
/// targets confirm immediately until unit picking exists.
pub fn handle_cast_requests(
    mut requests: MessageReader<CastSkill>,
    mut casters: Query<&mut SkillCaster>,
    mut confirmed: MessageWriter<SkillCast>,
    mut aiming: ResMut<SkillAiming>,
    mut legacy_targeting: ResMut<SkillTargeting>, // transitional until SK8
    mut build: ResMut<BuildMode>,
    defs: Res<SkillDefs>,
    balance: Res<SkillBalance>,
) {
    for req in requests.read() {
        let Ok(mut caster) = casters.get_mut(req.caster) else {
            continue; // despawned between request and validation
        };
        let Some(slot) = caster.slots.get(req.slot) else {
            continue;
        };
        let (def_id, level, cooldown) = (slot.def, slot.level, slot.cooldown);
        let def = defs.get(def_id);
        if cooldown > 0.0 || def.kind == SkillKind::Passive {
            continue;
        }
        // A cast owns the next click: disarm any pending build and the
        // phase-1 targeting (transitional cross-clear, dies with SK8).
        build.armed = false;
        legacy_targeting.0 = None;
        match def.target {
            SkillTargetMode::Point => {
                aiming.0 = Some((req.caster, req.slot));
                info!("[sk] {} entering point aiming", def.name);
            }
            SkillTargetMode::None | SkillTargetMode::Unit => {
                let cd = balance.cooldown(def_id, level);
                caster.slots[req.slot].cooldown = cd;
                confirmed.write(SkillCast {
                    caster: req.caster,
                    def: def_id,
                    level,
                    point: None,
                });
                info!("[sk] {} cast (lv {level}), cd {cd:.0}s", def.name);
            }
        }
    }
}

/// Point-aim picking for the generic framework (SK5): while `SkillAiming`
/// is set, the indicator ring (radius = the def's effect radius) follows
/// the cursor's ground point; a left click away from the UI confirms the
/// cast there (`SkillCast` + cooldown starts, aiming clears). Esc /
/// right-click cancel via `command::cancel_modes` — a cancel never pays.
/// Runs before `select_object` (the confirming click must not also
/// re-select) and before `handle_cast_requests` (the click that started
/// the cast cannot confirm itself one frame early) — same ordering
/// contract as the phase-1 `update_skill_targeting`.
#[allow(clippy::too_many_arguments)]
pub fn update_aim_picking(
    mut commands: Commands,
    buttons: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window>,
    camera: Query<(&Camera, &GlobalTransform)>,
    blocking: Query<&bevy::ui::RelativeCursorPosition, With<BlocksWorldInput>>,
    panel: Res<DebugPanelCapture>,
    state: Res<State<GameState>>,
    defs: Res<SkillDefs>,
    balance: Res<SkillBalance>,
    mut aiming: ResMut<SkillAiming>,
    mut casters: Query<&mut SkillCaster>,
    mut confirmed: MessageWriter<SkillCast>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    // Without<Enemy> keeps the ring Transform write disjoint from enemy
    // reads elsewhere in the frame (B0001; a ring is never an enemy).
    mut rings: Query<
        (Entity, &mut Transform, &mut Visibility),
        (With<SkillIndicatorRing>, Without<Enemy>),
    >,
    mut cache: Local<Option<(usize, Entity)>>, // (aimed def, ring entity)
) {
    // Resolve the aimed slot; None while not aiming, paused, or the caster
    // (or its slot) vanished mid-aim.
    let aimed = if *state.get() == GameState::Playing {
        aiming
            .0
            .filter(|(caster, slot)| {
                casters
                    .get(*caster)
                    .is_ok_and(|c| c.slots.get(*slot).is_some())
            })
            .map(|(caster, slot)| {
                let c = casters.get(caster).expect("checked above");
                let s = c.slots.get(slot).expect("checked above");
                (caster, slot, s.def, s.level)
            })
    } else {
        None
    };
    // Ring lifecycle: one ring while aiming, none otherwise; respawn when
    // the aimed def (radius) changes.
    let aimed_def = aimed.map(|(_, _, def, _)| def);
    if cache.map_or(true, |(d, _)| Some(d) != aimed_def) {
        if let Some((_, ring)) = *cache {
            commands.entity(ring).despawn();
        }
        if let Some(def) = aimed_def {
            let d = defs.get(def);
            let ring = commands
                .spawn((
                    SkillIndicatorRing,
                    Mesh3d(meshes.add(Circle::new(d.indicator_radius))),
                    MeshMaterial3d(materials.add(StandardMaterial {
                        base_color: Color::srgba(0.95, 0.85, 0.3, 0.4),
                        unlit: true,
                        alpha_mode: AlphaMode::Blend,
                        ..default()
                    })),
                    Visibility::Hidden,
                    Transform::from_translation(Vec3::ZERO)
                        .with_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)),
                ))
                .id();
            *cache = Some((def, ring));
        } else {
            *cache = None;
        }
    }
    let Some((caster_e, slot_idx, def_id, level)) = aimed else {
        // A pick whose caster despawned is dropped (nothing left to charge);
        // a paused game keeps the pick — only the ring hides (phase-1 shape).
        if *state.get() == GameState::Playing && aiming.0.is_some() {
            aiming.0 = None;
        }
        return;
    };
    // Follow the cursor's ground point.
    let ground = cursor_ground_point(&windows, &camera);
    if let Some((_, ring)) = *cache {
        if let Ok((_, mut tf, mut vis)) = rings.get_mut(ring) {
            match ground {
                Some(p) => {
                    tf.translation = Vec3::new(p.x, 0.09, p.z);
                    *vis = Visibility::Visible;
                }
                None => *vis = Visibility::Hidden,
            }
        }
    }
    // Left click (away from the UI) confirms the cast at the ground point.
    if buttons.just_pressed(MouseButton::Left) && !cursor_over_blocking_ui(&blocking, &panel) {
        if let Some(point) = ground {
            let cd = balance.cooldown(def_id, level);
            if let Ok(mut caster) = casters.get_mut(caster_e) {
                if let Some(slot) = caster.slots.get_mut(slot_idx) {
                    slot.cooldown = cd;
                }
            }
            confirmed.write(SkillCast {
                caster: caster_e,
                def: def_id,
                level,
                point: Some(point),
            });
            aiming.0 = None;
            info!(
                "[sk] {} cast at ({:.1}, {:.1}), cd {cd:.0}s",
                defs.get(def_id).name,
                point.x,
                point.z
            );
        }
    }
}

/// Tick every caster slot's cooldown down with virtual game time (SK5 —
/// the SK1 cooldown law, per entity: flows in both wave phases, never
/// resets across waves, frozen on pause via the Playing gate).
pub fn tick_caster_cooldowns(time: Res<Time>, mut casters: Query<&mut SkillCaster>) {
    for mut caster in &mut casters {
        for slot in caster.slots.iter_mut() {
            if slot.cooldown > 0.0 {
                slot.cooldown = (slot.cooldown - time.delta_secs()).max(0.0);
            }
        }
    }
}
