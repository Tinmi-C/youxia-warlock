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
    BuildMode, DebugPanelCapture, Lang, SkillId, SkillState, SkillTargeting, PROJECTILE_SPEED,
    SKILL_COOLDOWNS,
};
use crate::states::GameState;
use crate::components::{AttackType, Enemy, Projectile, ProjectileOrigin};
use crate::systems::command::{cursor_over_blocking_ui, BlocksWorldInput, ControlZoneRoot};
use crate::systems::hud::UiFont;
use crate::systems::l10n;

/// How a skill is aimed (§17.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkillAiming {
    /// Pick a ground point (indicator ring + left-click confirm).
    Point,
    /// No targeting — fires on the keypress.
    Instant,
}

/// One skill's framework data. `implemented` flips on per skill as SK2-4
/// land; an unimplemented skill shows greyed and can never cast (SK1 ships
/// the framework with none implemented yet).
pub struct SkillDef {
    pub id: SkillId,
    pub aiming: SkillAiming,
    pub implemented: bool,
    pub hotkey: KeyCode,
    pub badge: char,
    /// Indicator ring radius for point skills (the skill's effect radius).
    pub indicator_radius: f32,
}

/// The three skills in button order (Z/X/C, §17.3). Numbers live in
/// `resources::SKILL_COOLDOWNS` and the l10n tooltip.
pub const SKILL_DEFS: [SkillDef; 3] = [
    SkillDef {
        id: SkillId::MeteorRain,
        aiming: SkillAiming::Point,
        implemented: true, // SK2
        hotkey: KeyCode::KeyZ,
        badge: 'Z',
        indicator_radius: 3.0,
    },
    SkillDef {
        id: SkillId::FrostNova,
        aiming: SkillAiming::Point,
        implemented: true, // SK3
        hotkey: KeyCode::KeyX,
        badge: 'X',
        indicator_radius: 5.0,
    },
    SkillDef {
        id: SkillId::WarFrenzy,
        aiming: SkillAiming::Instant,
        implemented: false, // SK4
        hotkey: KeyCode::KeyC,
        badge: 'C',
        indicator_radius: 0.0,
    },
];

/// Framework definition of a skill id.
pub fn skill_def(id: SkillId) -> &'static SkillDef {
    SKILL_DEFS.iter().find(|d| d.id == id).expect("skill def")
}

/// Dispatch one CONFIRMED cast to its effect (the cooldown is started by the
/// caller — a cancelled target pick must never pay). Arms are filled in by
/// SK2/SK3/SK4; the framework keeps them as seams.
fn cast_skill(
    commands: &mut Commands,
    enemies: &Query<(Entity, &Transform), Without<SkillIndicatorRing>>,
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
            // SK4: war frenzy global burst.
            let _ = point;
        }
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
    for def in SKILL_DEFS.iter() {
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
    mut build: ResMut<BuildMode>,
    mut skills: ResMut<SkillState>,
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
        let def = skill_def(skill);
        if !def.implemented || !skills.ready(skill) {
            continue;
        }
        match def.aiming {
            SkillAiming::Point => {
                build.armed = false; // a cast replaces a pending build arm
                targeting.0 = Some(skill);
                info!("[sk] {:?} target picking", skill);
            }
            SkillAiming::Instant => {
                cast_skill(&mut commands, &enemies, skill, Vec3::ZERO);
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
            let def = skill_def(skill);
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
            cast_skill(&mut commands, &enemies, skill, point);
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
                    for def in SKILL_DEFS.iter() {
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
        let def = skill_def(btn.skill);
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
                Color::WHITE,
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
