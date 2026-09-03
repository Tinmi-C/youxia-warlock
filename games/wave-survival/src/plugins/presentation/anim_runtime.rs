//! Presentation domain — table-driven animation runtime. The state *table*
//! lives in `crate::plugins::anim` (pure logic); this module drives the Bevy
//! `AnimationPlayer` from that table (card 21 / 22 / 33) and shows the F2
//! egui monitor (card 33 phase 1).

use bevy::{animation::RepeatAnimation, prelude::*};
use bevy_egui::{egui, EguiContexts, EguiPrimaryContextPass};

use crate::components::{Attack, Hp, Player, Visual, WalkCycle};
use crate::plugins::anim;
use crate::states::GameState;
use crate::systems::contact::CONTACT_DIST;

use super::feel::FeelState;
use super::PresentationSet;

/// A monster is "attacking" when the player is within bite range
/// (contact distance + a small wind-up margin).
const MONSTER_ATTACK_RANGE: f32 = CONTACT_DIST + 0.15;

// --- components ------------------------------------------------------------

/// Per-owner animation link carried on the wrapper child; the ready observers
/// and the sync system read it to address graph/nodes/clips.
/// Card 33: the "which state" decision is no longer a hand-written match but
/// the table-driven `anim::derive_next_state`; the link still carries the
/// per-clip node indices (resolved from the asset at skin time) plus the
/// *current* state id.
#[derive(Component)]
pub(crate) struct AnimLink {
    pub(crate) graph_handle: Handle<AnimationGraph>,
    pub(crate) walk: AnimationNodeIndex,
    /// Hero run clip (card 25); None for monsters (they only walk).
    pub(crate) run: Option<AnimationNodeIndex>,
    pub(crate) idle: Option<AnimationNodeIndex>,
    pub(crate) attack: Option<AnimationNodeIndex>,
    pub(crate) hit: Option<AnimationNodeIndex>,
    /// Last state commanded (None = never, so the first sync frame after
    /// binding always issues an initial command).
    pub(crate) current: Option<anim::AnimStateId>,
    /// Active one-shot: (state, world elapsed at fire, display seconds).
    pub(crate) one_shot: Option<(anim::AnimStateId, f32, f32)>,
    /// One-shot queued by a combat edge this frame, started by the node loop
    /// (attack wins over hit if both edges land on the same frame).
    pub(crate) pending: Option<anim::AnimStateId>,
}

/// Combat-edge observation state (card 12 observer pattern: read-only view of
/// logic components). Edges are detected as increases because cooldown only
/// decreases and flash only decays otherwise. Card 22: every animated root
/// carries one — the player additionally watches cooldown (slash fires),
/// monsters additionally watch their distance to the player (bite wind-up).
#[derive(Component, Default)]
pub(crate) struct FxWatch {
    pub(crate) last_cooldown: f32,
    pub(crate) last_flash: f32,
}

/// Whether the animation monitor panel is shown (starts closed).
#[derive(Resource, Default)]
struct AnimPanelOpen {
    open: bool,
}

// --- plugin ----------------------------------------------------------------

/// Animation runtime domain plugin.
pub struct AnimRuntimePlugin;

impl Plugin for AnimRuntimePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<AnimPanelOpen>()
            .insert_resource(anim::hero_state_table())
            .add_systems(Update, drive_anim_states.in_set(PresentationSet::Anim))
            .add_systems(EguiPrimaryContextPass, anim_monitor_panel);
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
#[allow(clippy::too_many_arguments)]
fn drive_anim_states(
    state: Res<State<GameState>>,
    time: Res<Time>,
    table: Res<anim::AnimStateTable>,
    mut roots: Query<(
        Entity,
        &Transform,
        &WalkCycle,
        Option<&Attack>,
        Option<&Visual>,
        Option<&Hp>,
        Option<&mut FxWatch>,
        &Children,
    )>,
    player_pos: Query<&Transform, With<Player>>,
    mut links: Query<&mut AnimLink>,
    children: Query<&Children>,
    feels: Query<&FeelState>,
    mut players: Query<&mut AnimationPlayer>,
    mut transitions: Query<&mut AnimationTransitions>,
) {
    let in_game = *state.get() == GameState::Playing;
    let now = time.elapsed_secs();
    let player_at = player_pos.single().ok().map(|t| t.translation);

    for (_root, root_tf, walk, attack, visual, hp, watch, owner_children) in &mut roots {
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
        // hp_ratio (0..1) drives the Death state demo; default 1.0 (healthy) so
        // a missing Hp component never falsely triggers death.
        let hp_ratio = hp.map(|h| (h.hp / h.max).clamp(0.0, 1.0)).unwrap_or(1.0);

        // Anti-slide calibration (card 21 feedback #1): movement clips play at
        // ground speed / authored speed so feet plant instead of skating.
        // Card 25 fix: the two movement clips each use their OWN authored
        // speed, and the idle clip is always left at native 1.0.
        // Card 26 feedback: ground speed is the MEASURED displacement (written
        // by locomotion_feel last frame), not the static Player.speed.
        let ground_speed = feels.get(wrapper).ok().and_then(|f| f.speed).unwrap_or(0.0);
        let walk_rate = (ground_speed / anim::WALK_CLIP_AUTHORED_SPEED).clamp(0.5, 4.0);
        let run_rate = (ground_speed / anim::RUN_CLIP_AUTHORED_SPEED).clamp(0.5, 4.0);

        // Combat edges + one-shot expiry (Playing only).
        let mut cooldown_edge = false;
        let mut flash_edge = false;
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
                cooldown_edge = fired;
                flash_edge = bitten;
                if link.one_shot.is_none() && link.pending.is_none() {
                    if fired {
                        link.pending = Some(anim::AnimStateId::Attack);
                    } else if bitten {
                        link.pending = Some(anim::AnimStateId::Hit);
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
                    anim::AnimStateId::Attack => {
                        let window = if is_hero {
                            anim::HERO_ATTACK_WINDOW
                        } else {
                            anim::MONSTER_ATTACK_WINDOW
                        };
                        (link.attack.expect("owner carries attack"), window)
                    }
                    anim::AnimStateId::Hit => {
                        let window = if is_hero {
                            anim::HERO_HIT_WINDOW
                        } else {
                            anim::MONSTER_HIT_WINDOW
                        };
                        (link.hit.expect("owner carries hit"), window)
                    }
                    _ => unreachable!("one-shots are never state clips"),
                };
                trans
                    .play(&mut player, idx, anim::BLEND)
                    .set_repeat(RepeatAnimation::Never);
                link.one_shot = Some((clip, now, window));
                link.current = Some(clip);
            } else if link.one_shot.is_none() {
                // State clips — decided by the table-driven pure function. Use
                // the hero table, or the per-kind monster table.
                let inputs = anim::AnimInputs {
                    game: *state.get(),
                    moving,
                    ground_speed,
                    cooldown_edge,
                    flash_edge,
                    hp_ratio,
                    dist_to_player: player_at.map(|p| {
                        Vec2::new(root_tf.translation.x - p.x, root_tf.translation.z - p.z).length()
                    }),
                };
                let current = link.current.unwrap_or(anim::AnimStateId::Idle);
                let desired = if is_hero {
                    anim::derive_next_state(current, &inputs, &table)
                } else {
                    let monster_table = anim::monster_state_table(MONSTER_ATTACK_RANGE);
                    anim::derive_next_state(current, &inputs, &monster_table)
                };
                if link.current != Some(desired) {
                    let idx = match desired {
                        anim::AnimStateId::Walk => link.walk,
                        anim::AnimStateId::Run => link.run.expect("hero carries run"),
                        anim::AnimStateId::Idle => link.idle.expect("hero carries idle"),
                        _ => unreachable!("state clips are never one-shots"),
                    };
                    trans
                        .play(&mut player, idx, anim::BLEND)
                        .set_repeat(RepeatAnimation::Forever);
                    // each movement clip gets its own rate; idle stays native
                    match desired {
                        anim::AnimStateId::Walk => {
                            if let Some(active) = player.animation_mut(idx) {
                                active.set_speed(walk_rate);
                            }
                        }
                        anim::AnimStateId::Run => {
                            if let Some(active) = player.animation_mut(idx) {
                                active.set_speed(run_rate);
                            }
                        }
                        anim::AnimStateId::Idle => {
                            if let Some(active) = player.animation_mut(idx) {
                                active.set_speed(1.0);
                            }
                        }
                        _ => {}
                    }
                    link.current = Some(desired);
                    // card 25 acceptance instrument: prove the state machine
                    // actually switches (visual feedback loop).
                    // Card 30 feedback #3: rate added so playback-speed
                    // mis-calibration is visible in logs, not just on eyes.
                    let rate = match desired {
                        anim::AnimStateId::Walk => walk_rate,
                        anim::AnimStateId::Run => run_rate,
                        _ => 1.0,
                    };
                    info!(
                        "[presentation] hero clip -> {desired:?} (speed {ground_speed:.2}, rate {rate:.2})"
                    );
                }
            }
        }
    }
}

// --- card 33 phase-1: egui monitor (F2) ------------------------------------

/// F2 toggles a light egui panel that shows, live, the animation state machine:
///   * the whole topology (from the AnimStateTable resource),
///   * each owner's current state + measured speed + last transition.
fn anim_monitor_panel(
    keys: Res<ButtonInput<KeyCode>>,
    mut ctxs: EguiContexts,
    mut panel: ResMut<AnimPanelOpen>,
    table: Res<anim::AnimStateTable>,
    links: Query<(&AnimLink, &FeelState)>,
) {
    if keys.just_pressed(KeyCode::F2) {
        panel.open = !panel.open;
    }
    if !panel.open {
        return;
    }
    let Ok(ctx) = ctxs.ctx_mut() else {
        return;
    };

    egui::Window::new("🎞 Animation Monitor (card 33)").show(ctx, |ui| {
        // Topology: list every state + its playback attributes.
        ui.heading("State topology");
        for state in &table.states {
            ui.label(format!(
                "{:?}  (clip {}, loop {:?}, rate {:?})",
                state.id, state.clip, state.loop_mode, state.rate
            ));
        }
        ui.separator();

        // Per-owner runtime state (live).
        ui.heading("Owners (current state)");
        for (link, feel) in &links {
            let speed = feel.speed.unwrap_or(0.0);
            ui.label(format!(
                "state = {:?} (speed {speed:.2})",
                link.current.unwrap_or_default()
            ));
        }
    });
}
