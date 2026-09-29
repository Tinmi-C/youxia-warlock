//! Commander skills on the generic per-entity framework (ADR-0011; phase-1
//! cards SK1-SK4, migrated by SK8). The commander is a persistent singleton
//! entity carrying a `SkillCaster` with the three skills — meteor rain /
//! frost nova / war frenzy, all free, purely cooldown-gated:
//!   - per-slot cooldowns tick with virtual game time in BOTH phases, never
//!     reset across waves, start ready, freeze on pause (state-gated system
//!     + zero virtual delta under the debug freeze);
//!   - hotkeys Z/X/C and the control-zone buttons both write `CastSkill`;
//!     the framework validates (cooldown gate) and either confirms
//!     (`SkillCast` -> `commander_skill_effects`) or enters point aiming
//!     (`SkillAiming` — indicator ring + left-click confirm, Esc/right-click
//!     cancel via command::cancel_modes);
//!   - buttons live in the command-bar CONTROL zone (right), never in the
//!     QWER/ASDF command grid (§16.7); skills cast in build AND combat
//!     phases (§17.1) — the "lock building" iron law is untouched.

use bevy::input::mouse::MouseButton;
use bevy::prelude::*;
use bevy::window::Window;

use crate::resources::{
    BuildMode, DebugPanelCapture, FRENZY_DURATION, FRENZY_SPEED_MULT, Lang, SkillBalance,
    SkillDef, SkillDefs, SkillId, SkillKind, SkillTargetMode, SkillAiming, WarFrenzy,
    PROJECTILE_SPEED, SKILL_COOLDOWNS,
};
use crate::states::GameState;
use crate::components::{
    AttackType, Commander, Enemy, Projectile, ProjectileOrigin, SkillCaster, SkillSlot,
};
use crate::systems::command::{cursor_over_blocking_ui, BlocksWorldInput, ControlZoneRoot};
use crate::systems::hud::UiFont;
use crate::systems::l10n;

/// Registry ids of the commander skills (SK8). Order is contract: the
/// registry starts with the commander rows; archer rows land after them
/// (SK9). `SkillId::index()` equals its registry id (see resources.rs).
pub const SKILL_METEOR_RAIN: usize = 0;
pub const SKILL_FROST_NOVA: usize = 1;
pub const SKILL_WAR_FRENZY: usize = 2;

/// The production skill registry (SK8: the three commander rows; SK9 appends
/// the archer kit). Structure only — numbers live in `SkillBalance`.
pub fn production_skill_defs() -> SkillDefs {
    SkillDefs(vec![
        SkillDef {
            name: "meteor_rain",
            kind: SkillKind::Active,
            target: SkillTargetMode::Point,
            max_level: 1,
            badge: 'Z',
            hotkey: Some(KeyCode::KeyZ),
            indicator_radius: METEOR_RADIUS,
        },
        SkillDef {
            name: "frost_nova",
            kind: SkillKind::Active,
            target: SkillTargetMode::Point,
            max_level: 1,
            badge: 'X',
            hotkey: Some(KeyCode::KeyX),
            indicator_radius: NOVA_RADIUS,
        },
        SkillDef {
            name: "war_frenzy",
            kind: SkillKind::Active,
            target: SkillTargetMode::None,
            max_level: 1,
            badge: 'C',
            hotkey: Some(KeyCode::KeyC),
            indicator_radius: 0.0,
        },
    ])
}

/// The production `SkillBalance` rows, parallel to the registry (SK8). The
/// cooldown constants move here wholesale with SK11.
pub fn production_skill_balance() -> SkillBalance {
    SkillBalance {
        cooldowns: vec![
            vec![SKILL_COOLDOWNS[0]],
            vec![SKILL_COOLDOWNS[1]],
            vec![SKILL_COOLDOWNS[2]],
        ],
    }
}

/// Spawn the persistent commander singleton with its three skills (SK8).
/// Lives for the whole app run — cooldowns survive waves AND retries, the
/// phase-1 semantics (SkillState never reset).
pub fn spawn_commander(mut commands: Commands) {
    let mut caster = SkillCaster { slots: vec![] };
    learn_skill(&mut caster, SKILL_METEOR_RAIN, 1);
    learn_skill(&mut caster, SKILL_FROST_NOVA, 1);
    learn_skill(&mut caster, SKILL_WAR_FRENZY, 1);
    commands.spawn((Commander, caster));
    info!("[sk] commander entity spawned with three skills");
}

/// SK4: tick the frenzy countdown with virtual game time (frozen on pause
/// like every timer).
pub fn tick_war_frenzy(time: Res<Time>, mut frenzy: ResMut<WarFrenzy>) {
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

// --- UI: control-zone skill buttons (right zone, §17.3) -------------------------

/// Marker on the skill button row (a child of the command-bar control zone;
/// re-attached automatically when the zone rebuilds on language change).
#[derive(Component)]
pub struct SkillsRow;

/// One skill button in the control zone: `slot` indexes the commander's
/// `SkillCaster` slots; `skill` mirrors its def as the legacy SkillId for
/// the l10n labels and tooltips.
#[derive(Component)]
pub struct SkillButton {
    pub slot: usize,
    pub skill: SkillId,
}

/// Marker on a skill button's main label (cooldown seconds / name).
#[derive(Component)]
pub struct SkillButtonText;

/// Z/X/C hotkeys (§17.3, SK8): the keys write the same `CastSkill` request a
/// button click produces — the framework validates either identically.
pub fn skill_hotkeys(
    keys: Res<ButtonInput<KeyCode>>,
    panel: Res<DebugPanelCapture>,
    defs: Res<SkillDefs>,
    commanders: Query<(Entity, &SkillCaster), With<Commander>>,
    mut requests: MessageWriter<CastSkill>,
) {
    // Typing into a debug-panel field must not fire skill hotkeys (DB1).
    if panel.keyboard {
        return;
    }
    let Ok((commander, caster)) = commanders.single() else {
        return;
    };
    for (slot_idx, slot) in caster.slots.iter().enumerate() {
        if let Some(def) = defs.0.get(slot.def) {
            if def.hotkey.is_some_and(|k| keys.just_pressed(k)) {
                requests.write(CastSkill {
                    caster: commander,
                    slot: slot_idx,
                });
            }
        }
    }
}

/// Button clicks (SK1 semantics, SK8 path): a press writes a CastSkill for
/// the button's slot on the commander; the framework gates it (cooldown,
/// Passive, point picking) exactly like the hotkeys.
pub fn handle_skill_buttons(
    q: Query<(&Interaction, &SkillButton), Changed<Interaction>>,
    commanders: Query<Entity, With<Commander>>,
    mut requests: MessageWriter<CastSkill>,
    state: Res<State<GameState>>,
) {
    if *state.get() != GameState::Playing {
        return;
    }
    let Ok(commander) = commanders.single() else {
        return;
    };
    for (inter, btn) in &q {
        if *inter == Interaction::Pressed {
            requests.write(CastSkill {
                caster: commander,
                slot: btn.slot,
            });
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

/// Maintain the three control-zone skill buttons (SK1, SK8): spawn/respawn
/// the row when missing or on language change (the control zone rebuilds on
/// toggle and takes the row with it — self-healing), then keep every
/// button's enabled look, label and cooldown seconds live each frame — all
/// read from the commander entity's SkillCaster now.
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
    defs: Res<SkillDefs>,
    commanders: Query<&SkillCaster, With<Commander>>,
    mut built_lang: Local<Option<Lang>>,
) {
    let Some(caster) = commanders.single().ok() else {
        return;
    };
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
                    for (slot_idx, slot) in caster.slots.iter().enumerate() {
                        let def = defs.get(slot.def);
                        // Command-zone buttons are commander-owned; their
                        // defs are the registry's first rows.
                        let skill_id = SkillId::ALL
                            .get(slot.def)
                            .copied()
                            .expect("commander defs are the SkillId rows");
                        row.spawn((
                            Button,
                            SkillButton {
                                slot: slot_idx,
                                skill: skill_id,
                            },
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
                                Text::new(l10n::skill_name(skill_id, *lang)),
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
        let remaining = caster
            .slots
            .get(btn.slot)
            .map(|s| s.cooldown)
            .unwrap_or(0.0)
            .max(0.0);
        let enabled = remaining <= 0.0;
        if let Ok(mut bg) = backgrounds.get_mut(eid) {
            bg.0 = if enabled {
                Color::srgb(0.45, 0.22, 0.5)
            } else {
                Color::srgb(0.16, 0.16, 0.18)
            };
        }
        let (label, color) = if remaining > 0.0 {
            (format!("{remaining:.0}"), Color::srgb(0.75, 0.75, 0.78))
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
        // A cast owns the next click: disarm any pending build.
        build.armed = false;
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

/// The commander's skill effects (SK8): dispatches confirmed `SkillCast`s of
/// the three commander defs to their SK2/SK3/SK4 behaviour. Other defs are
/// not ours (SK9's archer effects system owns those).
pub fn commander_skill_effects(
    mut casts: MessageReader<SkillCast>,
    mut commands: Commands,
    // Without<SkillIndicatorRing> keeps this read disjoint from the aiming
    // ring's Transform writes (B0001; an enemy never carries the marker).
    enemies: Query<(Entity, &Transform), Without<SkillIndicatorRing>>,
    mut frenzy: ResMut<WarFrenzy>,
) {
    for cast in casts.read() {
        match cast.def {
            SKILL_METEOR_RAIN => {
                // SK2: schedule the three-drop volley at the confirmed point.
                let point = cast.point.unwrap_or_default();
                commands.spawn(MeteorVolley {
                    point,
                    shots_left: METEOR_COUNT,
                    timer: 0.0,
                });
            }
            SKILL_FROST_NOVA => {
                // SK3: everything within NOVA_RADIUS (XZ distance — ground
                // effects) takes the nova slow via the shared merge path.
                let point = cast.point.unwrap_or_default();
                let mut hits = 0u32;
                for (eid, tf) in enemies.iter() {
                    let dx = tf.translation.x - point.x;
                    let dz = tf.translation.z - point.z;
                    if dx * dx + dz * dz <= NOVA_RADIUS * NOVA_RADIUS {
                        crate::systems::tower::apply_slow(
                            &mut commands,
                            eid,
                            NOVA_SLOW_FACTOR,
                            NOVA_SLOW_DURATION,
                        );
                        hits += 1;
                    }
                }
                info!(
                    "[sk] frost nova at ({:.1}, {:.1}) slowed {hits} enemies",
                    point.x, point.z
                );
            }
            SKILL_WAR_FRENZY => {
                // SK4: global timed attack-speed burst; the SK6 aggregator
                // folds it into every tower's EffectiveStats at fire time.
                frenzy.remaining = FRENZY_DURATION;
                info!(
                    "[sk] war frenzy: all towers x{FRENZY_SPEED_MULT} for {FRENZY_DURATION}s"
                );
            }
            _ => {} // not a commander skill
        }
    }
}

/// Point-aim picking for the generic framework (SK5): while `SkillAiming`
/// is set, the indicator ring (radius = the def's effect radius) follows
/// the cursor's ground point; a left click away from the UI confirms the
/// cast there (`SkillCast` + cooldown starts, aiming clears). Esc /
/// right-click cancel via `command::cancel_modes` — a cancel never pays.
/// Runs before `select_object` (the confirming click must not also
/// re-select) and before the cast handlers (the click that started the
/// cast cannot confirm itself one frame early).
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
