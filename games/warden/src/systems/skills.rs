//! Commander skills on the generic per-entity framework (ADR-0011; phase-1
//! cards SK1-SK4, migrated by SK8). WX2 (ADR-0012): the framework TYPES
//! (SkillCaster/SkillSlot/SkillDefs/SkillDef/CastSkill/SkillCast) now come
//! from warbase — re-exported here and in resources/components under their
//! historical paths. What stays warden-side by design: the per-(id, level)
//! numbers (SkillBalance), the point-aiming interaction layer (SkillAiming —
//! the base validates and fires immediately; warden defers Point skills to a
//! ground pick and starts the cooldown only on confirm), and the effect
//! systems. The commander is a persistent singleton entity carrying a
//! `SkillCaster` with the three skills — meteor rain / frost nova / war
//! frenzy, all free, purely cooldown-gated:
//!   - per-slot cooldowns tick with virtual game time in BOTH phases, never
//!     reset across waves, start ready, freeze on pause (state-gated system
//!     + zero virtual delta under the debug freeze);
//!   - hotkeys Z/X/C and the control-zone buttons both write `CastSkill`;
//!     the warden validator (cooldown gate + aiming) either confirms
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
    BuildMode, DebugPanelCapture, Lang, SkillBalance, SkillDef, SkillDefs, SkillId, SkillKind,
    SkillTargetMode, SkillAiming, WarFrenzy, PROJECTILE_SPEED, SKILL_ORDER,
};
use crate::states::GameState;
use crate::components::{
    AttackType, Commander, Enemy, Projectile, ProjectileOrigin, SkillCaster, SkillSlot,
};
use crate::systems::command::{cursor_over_blocking_ui, BlocksWorldInput, ControlZoneRoot};
use crate::systems::hud::UiFont;
use crate::systems::l10n;

// The cast messages are the base's (WX2); SkillCast carries the confirmed
// ground point for Point-target casts, which the warden aiming layer writes.
pub use warbase::prelude::CastSkill;
pub use warbase::prelude::SkillCast;

/// Registry ids of the commander skills (SK8). The string def ids are the
/// contract: the registry starts with the commander rows; archer rows land
/// after them (SK9), fusion rows last (SK12) — see `resources::SKILL_ORDER`.
pub const SKILL_METEOR_RAIN: &str = "meteor_rain";
pub const SKILL_FROST_NOVA: &str = "frost_nova";
pub const SKILL_WAR_FRENZY: &str = "war_frenzy";

/// The production skill registry (SK8 commander rows + SK9 archer rows +
/// SK12 fusion rows): the base's string-keyed SkillDefs. Structure only —
/// numbers live in `SkillBalance` (the def's `cooldown` mirrors the table's
/// lv1 value so the base type stays complete; warden's cast gate always
/// reads the table per (id, level)).
pub fn production_skill_defs() -> SkillDefs {
    let balance = production_skill_balance();
    let mut defs = SkillDefs::default();
    let mut put = |id: &str,
                   kind: SkillKind,
                   target: SkillTargetMode,
                   max_level: u8,
                   badge: Option<char>,
                   hotkey: Option<KeyCode>| {
        defs.defs.insert(
            id.to_string(),
            SkillDef {
                id: id.to_string(),
                kind,
                targeting: target,
                max_level,
                cooldown: balance.cooldown(id, 1),
                badge,
                hotkey,
            },
        );
    };
    put(
        SKILL_METEOR_RAIN,
        SkillKind::Active,
        SkillTargetMode::Point,
        1,
        Some('Z'),
        Some(KeyCode::KeyZ),
    );
    put(
        SKILL_FROST_NOVA,
        SkillKind::Active,
        SkillTargetMode::Point,
        1,
        Some('X'),
        Some(KeyCode::KeyX),
    );
    put(
        SKILL_WAR_FRENZY,
        SkillKind::Active,
        SkillTargetMode::None,
        1,
        Some('C'),
        Some(KeyCode::KeyC),
    );
    for def in crate::systems::archer_skills::archer_skill_defs(&balance) {
        defs.defs.insert(def.id.clone(), def);
    }
    for def in crate::systems::fusion_skills::fusion_skill_defs(&balance) {
        defs.defs.insert(def.id.clone(), def);
    }
    debug_assert!(
        SKILL_ORDER.iter().all(|id| defs.defs.contains_key(*id)),
        "registry covers SKILL_ORDER"
    );
    defs
}

/// The production `SkillBalance` rows, keyed by `SKILL_ORDER` (SK8/9/11/12).
/// THE single source of every skill number in the game (SK11 gathered the
/// last scattered constants here; tune via §14, never inline).
pub fn production_skill_balance() -> SkillBalance {
    SkillBalance {
        ids: SKILL_ORDER.iter().map(|s| s.to_string()).collect(),
        cooldowns: vec![
            vec![45.0], // meteor rain
            vec![40.0], // frost nova
            vec![60.0], // war frenzy
            // The archer kit: a passive never casts; multishot 25s; fire 20s.
            vec![0.0],
            vec![25.0, 25.0, 25.0],
            vec![20.0, 20.0, 20.0],
            // The fusion signatures (SK12).
            vec![30.0, 30.0, 30.0], // deadeye
            vec![35.0, 35.0, 35.0], // arcane blast
            vec![25.0, 25.0, 25.0], // piercing bolt
            vec![30.0, 30.0, 30.0], // concussion
        ],
        // Point-skill effect radii (also the aiming ring's); the piercing
        // bolt's ring is its contact width (a line skill shows a cursor dot).
        radius: vec![
            vec![3.0], // meteor blast
            vec![5.0], // nova
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
            vec![4.0, 4.5, 5.0], // arcane blast
            vec![1.5, 1.5, 1.5], // piercing bolt width
            vec![4.0, 4.0, 4.5], // concussion
        ],
        // 鹰眼 crit rows (registry ids 0-2 are the commander: empty).
        crit_chance: vec![vec![], vec![], vec![], vec![0.10, 0.15, 0.20]],
        crit_multiplier_add: vec![vec![], vec![], vec![], vec![0.0, 0.25, 0.5]],
        // 多重射击 arrows = distinct targets per cast.
        multishot_targets: vec![vec![], vec![], vec![], vec![], vec![3.0, 4.0, 5.0]],
        // 火焰箭: flat damage bonus for its duration.
        fire_arrow_bonus: vec![vec![], vec![], vec![], vec![], vec![], vec![3.0, 5.0, 8.0]],
        fire_arrow_duration: vec![vec![], vec![], vec![], vec![], vec![], vec![8.0, 8.0, 8.0]],
        // 流星火雨: 3 drops, 0.3s apart, 40 magic each, from 10 units up.
        meteor_count: vec![vec![3.0], vec![], vec![], vec![], vec![], vec![]],
        meteor_interval: vec![vec![0.3], vec![], vec![], vec![], vec![], vec![]],
        meteor_damage: vec![vec![40.0], vec![], vec![], vec![], vec![], vec![]],
        // 冰霜新星: slow to 50% for 2s.
        nova_slow_factor: 0.5,
        nova_slow_duration: 2.0,
        // 战意狂暴: all towers +50% attack speed for 3s.
        frenzy_speed_mult: 1.5,
        frenzy_duration: 3.0,
        // SK12 狙杀: normal-attack damage ×4/5/6.
        deadeye_mult: vec![vec![], vec![], vec![], vec![], vec![], vec![], vec![4.0, 5.0, 6.0]],
        // SK12 奥术爆轰: instant magic damage per victim.
        arcane_damage: vec![vec![], vec![], vec![], vec![], vec![], vec![], vec![], vec![60.0, 90.0, 120.0]],
        // SK12 贯穿箭: damage per pierced enemy, bolt length; width below.
        pierce_damage: vec![vec![], vec![], vec![], vec![], vec![], vec![], vec![], vec![], vec![35.0, 50.0, 65.0]],
        pierce_length: vec![vec![], vec![], vec![], vec![], vec![], vec![], vec![], vec![], vec![10.0, 12.0, 14.0]],
        pierce_width: 1.5,
        // SK12 震荡冲击: instant physical damage; slow to 60% for 2.5s.
        concussion_damage: vec![vec![], vec![], vec![], vec![], vec![], vec![], vec![], vec![], vec![], vec![30.0, 45.0, 60.0]],
        concussion_slow_factor: 0.6,
        concussion_slow_duration: 2.5,
        // SK10 paid kit upgrade: 30g for 1→2, 60g for 2→3.
        upgrade_costs: vec![30.0, 60.0],
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

// --- SK2: meteor rain ------------------------------------------------------------

/// One confirmed meteor-rain cast: a rule-layer scheduler entity dropping
/// the remaining meteors at `point`, spaced by the balance table's interval.
#[derive(Component)]
pub struct MeteorVolley {
    pub point: Vec3,
    pub shots_left: u32,
    pub timer: f32,
}

/// Presentation-only (循环 C, the future FxStyle table): meteors spawn this
/// far above the point and fall at PROJECTILE_SPEED — visible flight time,
/// not a balance number.
pub const METEOR_FALL_HEIGHT: f32 = 10.0;

/// Release the pending meteor drops (SK2): every interval (SkillBalance)
/// one rule-layer projectile launches from above the point, flying straight
/// down to it — the fixed-point origin seam of `Projectile` (§17.2
/// 发起方=技能). Each impact resolves the table's MAGIC damage in its radius
/// (armor per victim: magic pierces physical armor, §6).
pub fn release_meteor_shots(
    time: Res<Time>,
    balance: Res<SkillBalance>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut volleys: Query<(Entity, &mut MeteorVolley)>,
    commanders: Query<Entity, With<Commander>>,
) {
    let commander = commanders.single().ok().unwrap_or(Entity::PLACEHOLDER);
    let def = SKILL_METEOR_RAIN;
    let total = balance.meteor_count(def, 1);
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
                damage: balance.meteor_damage(def, 1),
                attack_type: AttackType::Magic,
                source: crate::systems::damage::DamageOrigin::Skill {
                    caster: commander,
                    skill: crate::systems::damage::skill_row(SKILL_METEOR_RAIN),
                },
                crit: false,
                aoe_radius: balance.radius(def, 1),
                slow_factor: 0.0,
                slow_duration: 0.0,
                speed: PROJECTILE_SPEED,
                last_target_pos: volley.point,
                pierced: vec![],
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
        volley.timer = balance.meteor_interval(def, 1);
        info!(
            "[sk] meteor {} of {total} away -> ({:.1}, {:.1})",
            total - volley.shots_left,
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
        if let Some(def) = defs.get(&slot.skill) {
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
                        // Skip slots whose def is not registered — tests
                        // override the registry wholesale; the panel
                        // self-heals like it does on zone rebuilds.
                        let (Some(def), Some(skill_id)) =
                            (defs.get(&slot.skill), SkillId::from_def_id(&slot.skill))
                        else {
                            continue;
                        };
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
                                Text::new(def.badge.map(|b| b.to_string()).unwrap_or_default()),
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

// --- Generic skill framework (phase 2, ADR-0011; card SK5; base types WX2) ----------
//
// Owner-agnostic lifecycle for per-entity skills: validation, cooldown,
// point aiming, learning/leveling. Everything here reads the base
// `SkillCaster` and the `SkillDefs`/`SkillBalance` tables — never a
// specific owner type. Concrete skill EFFECTS are separate systems that
// read `SkillCast` and match on the def id; the framework never knows what
// a skill does. (`CastSkill`/`SkillCast` are the base messages, re-exported
// at the top of this file.)

/// Learn (or re-learn) a skill on a caster at `level` (SK5 API — content
/// cards SK8/SK9 and tests call it). Pure data, no side effects: starts
/// ready, autocast toggle OFF. Re-learning keeps the current cooldown.
pub fn learn_skill(caster: &mut SkillCaster, def: &str, level: u8) {
    if let Some(slot) = caster.slots.iter_mut().find(|s| s.skill == def) {
        slot.level = level;
    } else {
        caster.slots.push(SkillSlot {
            skill: def.to_string(),
            level,
            cooldown: 0.0,
            autocast_on: false,
        });
    }
}

/// Raise a slot one level, clamped at the def's `max_level` (SK5 API — SK9
/// wires this to tower invest). Returns the (possibly unchanged) level.
pub fn level_up(defs: &SkillDefs, caster: &mut SkillCaster, slot: usize) -> u8 {
    let max = defs
        .get(&caster.slots[slot].skill)
        .expect("skill def registered")
        .max_level
        .max(1);
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
///
/// Unlike the base's `process_casts` (immediate confirm, def-embedded
/// cooldown), warden reads the per-(id, level) cooldown from SkillBalance
/// and defers Point skills to the aiming layer — the WX2 semantics bridge.
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
        let (skill_id, level, cooldown) = (slot.skill.clone(), slot.level, slot.cooldown);
        let Some(def) = defs.get(&skill_id) else {
            continue;
        };
        if cooldown > 0.0 || def.kind == SkillKind::Passive {
            continue;
        }
        // A cast owns the next click: disarm any pending build.
        build.armed = false;
        match def.targeting {
            SkillTargetMode::Point => {
                aiming.0 = Some((req.caster, req.slot));
                info!("[sk] {} entering point aiming", def.id);
            }
            SkillTargetMode::None | SkillTargetMode::Unit => {
                let cd = balance.cooldown(&skill_id, level);
                caster.slots[req.slot].cooldown = cd;
                confirmed.write(SkillCast {
                    caster: req.caster,
                    skill: skill_id,
                    level,
                    point: None,
                });
                info!("[sk] {} cast (lv {level}), cd {cd:.0}s", def.id);
            }
        }
    }
}

/// The commander's skill effects (SK8): dispatches confirmed `SkillCast`s of
/// the three commander defs to their SK2/SK3/SK4 behaviour. Other defs are
/// not ours (SK9's archer effects system owns those).
pub fn commander_skill_effects(
    mut casts: MessageReader<SkillCast>,
    balance: Res<SkillBalance>,
    mut commands: Commands,
    // Without<SkillIndicatorRing> keeps this read disjoint from the aiming
    // ring's Transform writes (B0001; an enemy never carries the marker).
    enemies: Query<(Entity, &Transform), Without<SkillIndicatorRing>>,
    mut frenzy: ResMut<WarFrenzy>,
) {
    for cast in casts.read() {
        match cast.skill.as_str() {
            SKILL_METEOR_RAIN => {
                // SK2: schedule the volley at the confirmed point.
                let point = cast.point.unwrap_or_default();
                commands.spawn(MeteorVolley {
                    point,
                    shots_left: balance.meteor_count(SKILL_METEOR_RAIN, cast.level),
                    timer: 0.0,
                });
            }
            SKILL_FROST_NOVA => {
                // SK3: everything within the nova radius (XZ distance —
                // ground effects) takes the slow via the shared merge path.
                let point = cast.point.unwrap_or_default();
                let radius = balance.radius(SKILL_FROST_NOVA, cast.level);
                let mut hits = 0u32;
                for (eid, tf) in enemies.iter() {
                    let dx = tf.translation.x - point.x;
                    let dz = tf.translation.z - point.z;
                    if dx * dx + dz * dz <= radius * radius {
                        crate::systems::tower::apply_slow(
                            &mut commands,
                            eid,
                            balance.nova_slow_factor,
                            balance.nova_slow_duration,
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
                frenzy.remaining = balance.frenzy_duration;
                info!(
                    "[sk] war frenzy: all towers x{:.1} for {:.0}s",
                    balance.frenzy_speed_mult, balance.frenzy_duration
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
    mut cache: Local<Option<(String, Entity)>>, // (aimed def id, ring entity)
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
                (caster, slot, s.skill.clone(), s.level)
            })
    } else {
        None
    };
    // Ring lifecycle: one ring while aiming, none otherwise; respawn when
    // the aimed def (radius) changes.
    let aimed_def = aimed.as_ref().map(|(_, _, def, _)| def.clone());
    if cache.as_ref().map_or(true, |(d, _)| Some(d) != aimed_def.as_ref()) {
        if let Some((_, ring)) = *cache {
            commands.entity(ring).despawn();
        }
        if let Some(def) = &aimed_def {
            // The ring shows the skill's effect radius — SkillBalance is the
            // single source (point skills are level-1 today).
            let ring = commands
                .spawn((
                    SkillIndicatorRing,
                    Mesh3d(meshes.add(Circle::new(balance.radius(def, 1)))),
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
            *cache = Some((def.clone(), ring));
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
            let cd = balance.cooldown(&def_id, level);
            if let Ok(mut caster) = casters.get_mut(caster_e) {
                if let Some(slot) = caster.slots.get_mut(slot_idx) {
                    slot.cooldown = cd;
                }
            }
            confirmed.write(SkillCast {
                caster: caster_e,
                skill: def_id.clone(),
                level,
                point: Some(point),
            });
            aiming.0 = None;
            info!(
                "[sk] {} cast at ({:.1}, {:.1}), cd {cd:.0}s",
                defs.get(&def_id).map(|d| d.id.as_str()).unwrap_or("?"),
                point.x,
                point.z
            );
        }
    }
}
