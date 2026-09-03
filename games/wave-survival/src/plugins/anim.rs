//! Card 33 / ADR-0007: table-driven animation state machine.
//!
//! The hand-written `sync_walk_playback` match/if-else in presentation.rs is
//! replaced by a *table* of states (`AnimStateTable`) + one generic driver
//! (`drive_anim_states`). Adding a state = adding one table row; the driver's
//! control flow stays constant regardless of how many states exist. This is the
//! industry-standard shape (Unreal transition rules, Unity Animator condition
//! parameters), and it keeps playback on Bevy's built-in
//! `AnimationTransitions`/`AnimationPlayer` (no self-built engine).
//!
//! The only place with *logic* is `derive_next_state`, a pure function
//! (no ECS, no side effects) so it is directly unit-testable — that is the
//! heart of this card's value: changing a state (or adding one) is data-driven
//! and AI/human safe.
//!
//! Layout:
//!   AnimStateTable (Resource) : Vec<AnimState> — the whole topology
//!   AnimState                  : one state's clip/repeat/rate + transitions
//!   Transition                 : "when to enter this state" conditions
//!   derive_next_state(...)     : pure fn, current + inputs -> next state
//!
//! This module is kept separate from presentation.rs (which owns skinning,
//! weapon attach, feel, flash). It holds only the state-machine logic so the
//! animation logic is testable and isolated. Kept `pub(crate)`-visible to
//! presentation.rs.

use std::time::Duration;

use bevy::prelude::*;

use crate::states::GameState;

// --- constants (kept in one place; migrated from presentation.rs consts) -----

/// Ground speed at/above which the hero plays the run clip.
pub const RUN_SPEED_THRESHOLD: f32 = 3.0;
/// Anti-slide authored speeds (walk / run).
pub const WALK_CLIP_AUTHORED_SPEED: f32 = 1.6;
pub const RUN_CLIP_AUTHORED_SPEED: f32 = 2.8;
/// Anti-slide playback-rate clamp.
pub const RATE_CLAMP_MIN: f32 = 0.5;
pub const RATE_CLAMP_MAX: f32 = 4.0;
/// Combat one-shot display windows.
pub const HERO_ATTACK_WINDOW: f32 = 0.6;
pub const HERO_HIT_WINDOW: f32 = 0.4;
pub const MONSTER_ATTACK_WINDOW: f32 = 0.38;
pub const MONSTER_HIT_WINDOW: f32 = 0.29;
/// Cross-fade between clips.
pub const BLEND: Duration = Duration::from_millis(200);

// --- state identity ----------------------------------------------------------

/// The set of animation states. Adding a variant = adding one state; the driver
/// never matches on this exhaustively (it reads the table), so the growth is
/// data not control flow.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, Default)]
pub enum AnimStateId {
    #[default]
    Idle,
    Walk,
    Run,
    Attack,
    Hit,
    /// Added as the card-33 extensibility demo: a whole new state is just one
    /// enum variant + one table row; `drive_anim_states` (its control flow) is
    /// untouched.
    Death,
}

impl AnimStateId {
    /// The one-shot window for combat states, if any (None = looping state).
    pub fn one_shot_window(self, is_hero: bool) -> Option<f32> {
        match self {
            AnimStateId::Attack => {
                Some(if is_hero { HERO_ATTACK_WINDOW } else { MONSTER_ATTACK_WINDOW })
            }
            AnimStateId::Hit => {
                Some(if is_hero { HERO_HIT_WINDOW } else { MONSTER_HIT_WINDOW })
            }
            _ => None,
        }
    }
}

// --- playback attributes -----------------------------------------------------

/// Loop mode: state clips loop; one-shots play once then exit.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LoopMode {
    Forever,
    Once,
}

/// Playback-rate mode (anti-slide).
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum RateMode {
    /// Play at the clip's native speed (idle moves at 1.0).
    Native,
    /// Anti-slide: speed / authored_speed, clamped.
    AntiSlide { authored: f32 },
}

// --- transitions (the "when to enter this state" logic, data-driven) ---------

/// Conditions that trigger entering a state. An owner enters `target` when any
/// condition in the state row's `transitions` list matches (first-wins), unless
/// it is already in that state (guarded in `derive_next_state`).
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Transition {
    /// Ground speed >= threshold (walk<->run gating).
    SpeedAtLeast { threshold: f32 },
    /// Ground speed < threshold (run->walk gating).
    SpeedBelow { threshold: f32 },
    /// Whether the owner is moving this frame.
    Moving { want: bool },
    /// Combat edge: melee cooldown just reset (slash fired).
    CooldownEdge,
    /// Combat edge: hit-flash just rose (was hit).
    FlashEdge,
    /// Distance to the player <= range (monster bite wind-up).
    InRange { range: f32 },
    /// Hit points ratio <= threshold (card-33 extensibility demo: Death).
    HpBelow { threshold: f32 },
    /// Always true (initial/fallback).
    Always,
}

// --- one state row -----------------------------------------------------------

/// One state's full definition. A `Vec<AnimState>` is the whole topology.
#[derive(Clone, Debug)]
pub struct AnimState {
    pub id: AnimStateId,
    /// Which clip index to play (Animat ionNodeIndex in the model's graph).
    pub clip: usize,
    /// Loop mode (Forever for state clips, Once for one-shots).
    pub loop_mode: LoopMode,
    /// Rate mode (anti-slide for movement clips, native otherwise).
    pub rate: RateMode,
    /// Conditions to enter this state (from the owner's *previous* state).
    pub transitions: Vec<Transition>,
    /// Where a one-shot returns after its window (None = keep looping state).
    pub on_finish: Option<AnimStateId>,
}

// --- the table --------------------------------------------------------------

/// The whole state machine topology. Added as a Resource in build_app.
#[derive(Resource, Clone, Debug)]
pub struct AnimStateTable {
    pub states: Vec<AnimState>,
}

impl AnimStateTable {
    /// Look up a state square by its id (for the driver + tests).
    pub fn get(&self, id: AnimStateId) -> Option<&AnimState> {
        self.states.iter().find(|s| s.id == id)
    }
}

// --- pure decision logic -----------------------------------------------------

/// The per-frame logic inputs (read from logical components, fed in by the
/// driver). This is the only place state-transition *logic* lives.
#[derive(Clone, Copy, Debug, Default)]
pub struct AnimInputs {
    pub game: GameState,
    pub moving: bool,
    pub ground_speed: f32,
    pub cooldown_edge: bool,
    pub flash_edge: bool,
    pub dist_to_player: Option<f32>,
    /// Hit-points ratio (0..1) — used by the Death state demo.
    pub hp_ratio: f32,
}

/// Decide the next state, given the current state and this frame's inputs.
/// Pure: no ECS, no side effects — directly testable.
///
/// Meaning table (mirrors the hand-written `sync_walk_playback`):
///   - one-shot in progress? stay in it (window handled by the driver via
///     elapsed time, not here); Attack/Hit return on `on_finish` after window.
///   - combat edge -> Attack (cooldown) or Hit (flash), Attack wins.
///   - idle if not moving; walk if moving below threshold; run if at/above.
pub fn derive_next_state(current: AnimStateId, inputs: &AnimInputs, table: &AnimStateTable) -> AnimStateId {
    // While a one-shot (Attack/Hit) is active, hold it — the driver closes the
    // window and moves to `on_finish` based on elapsed time. (If more one-shot
    // states are added, extend this.)
    if matches!(current, AnimStateId::Attack | AnimStateId::Hit) {
        return current;
    }
    // Death is a held terminal state: once entered, never switch away (the
    // hero is dead; animation stays on the death clip).
    if current == AnimStateId::Death {
        return current;
    }

    let in_game = inputs.game == GameState::Playing;

    // Not in-game: freeze — return current (driver freezes all clips). We keep
    // whatever state we were in; the driver handles pause semantics.
    if !in_game {
        return current;
    }

    // Table-driven: scan every state in the table, in order, and enter the
    // first one whose full transition-condition set is satisfied by this
    // frame's inputs (skipping the state we're already in, so we don't
    // re-trigger it). A state's `transitions` are AND-combined (all must hold);
    // priority is by table order (earlier rows win). This is the whole "logic"
    // of the state machine; adding/removing states is purely a table edit, and
    // control flow never changes.
    for state in &table.states {
        if state.id == current {
            continue;
        }
        if state
            .transitions
            .iter()
            .all(|t| transition_matches(t, inputs))
        {
            return state.id;
        }
    }
    current
}

/// Evaluate one transition condition against the frame's inputs.
fn transition_matches(t: &Transition, inputs: &AnimInputs) -> bool {
    match t {
        Transition::SpeedAtLeast { threshold } => inputs.moving && inputs.ground_speed >= *threshold,
        Transition::SpeedBelow { threshold } => inputs.moving && inputs.ground_speed < *threshold,
        Transition::Moving { want } => inputs.moving == *want,
        Transition::CooldownEdge => inputs.cooldown_edge,
        Transition::FlashEdge => inputs.flash_edge,
        Transition::InRange { range } => inputs.dist_to_player.is_some_and(|d| d <= *range),
        Transition::HpBelow { threshold } => inputs.hp_ratio <= *threshold,
        Transition::Always => true,
    }
}

// --- clip-index resolution (helper shared with the driver) -------------------

/// Which clip index a state maps to, given hero vs monster. Hero and monster
/// clip layouts differ, so the table stores hero indices and the driver
/// resolves the monster ones (monsters only have walk/attack/hit).
pub fn state_clip_index(id: AnimStateId, is_hero: bool) -> Option<usize> {
    if is_hero {
        match id {
            AnimStateId::Attack => Some(0),
            AnimStateId::Hit => Some(1),
            AnimStateId::Idle => Some(2),
            AnimStateId::Run => Some(3),
            AnimStateId::Walk => Some(4),
            // Death demo: no dedicated clip yet; use Idle as a stand-in so the
            // extensibility demo compiles without a new model asset.
            AnimStateId::Death => Some(2),
        }
    } else {
        // monsters: attack=0, hit=3, walk=7 (from MonsterKind::*_clip())
        match id {
            AnimStateId::Attack => Some(0),
            AnimStateId::Hit => Some(3),
            AnimStateId::Walk => Some(7),
            AnimStateId::Idle | AnimStateId::Run | AnimStateId::Death => None,
        }
    }
}

// --- state tables (the actual topology, one per owner class) -----------------

/// Hero state table. Order matters: one-shots first (they win over movement
/// gating on the same frame), then run before walk (speed threshold decides).
pub fn hero_state_table() -> AnimStateTable {
    AnimStateTable {
        states: vec![
            // Death is a held terminal state — checked first so it wins over
            // everything once hp is low (card-33 extensibility demo).
            AnimState { id: AnimStateId::Death, clip: 2, loop_mode: LoopMode::Forever, rate: RateMode::Native, transitions: vec![Transition::HpBelow { threshold: 0.3 }], on_finish: None },
            AnimState { id: AnimStateId::Attack, clip: 0, loop_mode: LoopMode::Once, rate: RateMode::Native, transitions: vec![Transition::CooldownEdge], on_finish: Some(AnimStateId::Idle) },
            AnimState { id: AnimStateId::Hit, clip: 1, loop_mode: LoopMode::Once, rate: RateMode::Native, transitions: vec![Transition::FlashEdge], on_finish: Some(AnimStateId::Idle) },
            AnimState { id: AnimStateId::Run, clip: 3, loop_mode: LoopMode::Forever, rate: RateMode::AntiSlide { authored: RUN_CLIP_AUTHORED_SPEED }, transitions: vec![Transition::Moving { want: true }, Transition::SpeedAtLeast { threshold: RUN_SPEED_THRESHOLD }], on_finish: None },
            AnimState { id: AnimStateId::Walk, clip: 4, loop_mode: LoopMode::Forever, rate: RateMode::AntiSlide { authored: WALK_CLIP_AUTHORED_SPEED }, transitions: vec![Transition::Moving { want: true }, Transition::SpeedBelow { threshold: RUN_SPEED_THRESHOLD }], on_finish: None },
            AnimState { id: AnimStateId::Idle, clip: 2, loop_mode: LoopMode::Forever, rate: RateMode::Native, transitions: vec![Transition::Moving { want: false }], on_finish: None },
        ],
    }
}

/// Monster state table (the four monster kinds share one clip layout; they only
/// walk and have attack/hit one-shots — no run/idle). `InRange` drives the bite
/// wind-up; `FlashEdge` drives the hit flinch.
pub fn monster_state_table(range: f32) -> AnimStateTable {
    AnimStateTable {
        states: vec![
            AnimState { id: AnimStateId::Attack, clip: 0, loop_mode: LoopMode::Once, rate: RateMode::Native, transitions: vec![Transition::InRange { range }], on_finish: Some(AnimStateId::Walk) },
            AnimState { id: AnimStateId::Hit, clip: 3, loop_mode: LoopMode::Once, rate: RateMode::Native, transitions: vec![Transition::FlashEdge], on_finish: Some(AnimStateId::Walk) },
            AnimState { id: AnimStateId::Walk, clip: 7, loop_mode: LoopMode::Forever, rate: RateMode::AntiSlide { authored: WALK_CLIP_AUTHORED_SPEED }, transitions: vec![Transition::Moving { want: true }], on_finish: None },
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // The hero state table, with the "enter a state when its condition holds"
    // semantics. Order matters: combat one-shots come first (they win over
    // movement gating on the same frame).
    fn hero_table() -> AnimStateTable {
        AnimStateTable {
            states: vec![
                // Death (held terminal) — first, wins over everything when hp low.
                AnimState { id: AnimStateId::Death, clip: 2, loop_mode: LoopMode::Forever, rate: RateMode::Native, transitions: vec![Transition::HpBelow { threshold: 0.3 }], on_finish: None },
                // One-shots (cooldown/flash edges) — high priority.
                AnimState { id: AnimStateId::Attack, clip: 0, loop_mode: LoopMode::Once, rate: RateMode::Native, transitions: vec![Transition::CooldownEdge], on_finish: Some(AnimStateId::Idle) },
                AnimState { id: AnimStateId::Hit, clip: 1, loop_mode: LoopMode::Once, rate: RateMode::Native, transitions: vec![Transition::FlashEdge], on_finish: Some(AnimStateId::Idle) },
                // Movement states (gating) — lower priority.
                AnimState { id: AnimStateId::Run, clip: 3, loop_mode: LoopMode::Forever, rate: RateMode::AntiSlide { authored: RUN_CLIP_AUTHORED_SPEED }, transitions: vec![Transition::Moving { want: true }, Transition::SpeedAtLeast { threshold: RUN_SPEED_THRESHOLD }], on_finish: None },
                AnimState { id: AnimStateId::Walk, clip: 4, loop_mode: LoopMode::Forever, rate: RateMode::AntiSlide { authored: WALK_CLIP_AUTHORED_SPEED }, transitions: vec![Transition::Moving { want: true }, Transition::SpeedBelow { threshold: RUN_SPEED_THRESHOLD }], on_finish: None },
                AnimState { id: AnimStateId::Idle, clip: 2, loop_mode: LoopMode::Forever, rate: RateMode::Native, transitions: vec![Transition::Moving { want: false }], on_finish: None },
            ],
        }
    }

    fn inputs(game: GameState, moving: bool, speed: f32, cd: bool, flash: bool) -> AnimInputs {
        AnimInputs { game, moving, ground_speed: speed, cooldown_edge: cd, flash_edge: flash, dist_to_player: None, hp_ratio: 1.0 }
    }

    #[test]
    fn idle_when_still() {
        let t = hero_table();
        let i = inputs(GameState::Playing, false, 0.0, false, false);
        assert_eq!(derive_next_state(AnimStateId::Walk, &i, &t), AnimStateId::Idle);
    }

    #[test]
    fn walk_below_threshold() {
        let t = hero_table();
        let i = inputs(GameState::Playing, true, 1.6, false, false);
        assert_eq!(derive_next_state(AnimStateId::Idle, &i, &t), AnimStateId::Walk);
    }

    #[test]
    fn run_at_threshold() {
        let t = hero_table();
        let i = inputs(GameState::Playing, true, 5.0, false, false);
        assert_eq!(derive_next_state(AnimStateId::Idle, &i, &t), AnimStateId::Run);
    }

    #[test]
    fn attack_on_cooldown_edge() {
        let t = hero_table();
        let i = inputs(GameState::Playing, true, 1.6, true, false);
        assert_eq!(derive_next_state(AnimStateId::Idle, &i, &t), AnimStateId::Attack);
    }

    #[test]
    fn hit_on_flash_edge() {
        let t = hero_table();
        let i = inputs(GameState::Playing, false, 0.0, false, true);
        assert_eq!(derive_next_state(AnimStateId::Idle, &i, &t), AnimStateId::Hit);
    }

    #[test]
    fn attack_wins_over_hit() {
        let t = hero_table();
        let i = inputs(GameState::Playing, true, 1.6, true, true);
        assert_eq!(derive_next_state(AnimStateId::Idle, &i, &t), AnimStateId::Attack);
    }

    #[test]
    fn freezes_outside_playing() {
        let t = hero_table();
        let i = inputs(GameState::Paused, false, 0.0, true, true);
        // Not in-game: combat edges ignored, current state preserved.
        assert_eq!(derive_next_state(AnimStateId::Idle, &i, &t), AnimStateId::Idle);
    }

    // --- card-33 extensibility demo: a whole new state (Death) is one enum
    // variant + one table row; drive_anim_states control flow is untouched.

    #[test]
    fn enters_death_when_hp_low() {
        let t = hero_table();
        let mut i = inputs(GameState::Playing, true, 1.6, false, false);
        i.hp_ratio = 0.2;
        assert_eq!(derive_next_state(AnimStateId::Walk, &i, &t), AnimStateId::Death);
    }

    #[test]
    fn death_is_a_held_terminal_state() {
        let t = hero_table();
        let mut i = inputs(GameState::Playing, true, 5.0, false, false);
        i.hp_ratio = 0.8; // hp recovers, but a dead hero stays dead
        assert_eq!(derive_next_state(AnimStateId::Death, &i, &t), AnimStateId::Death);
    }

    #[test]
    fn not_death_when_hp_healthy() {
        let t = hero_table();
        let i = inputs(GameState::Playing, false, 0.0, false, false);
        let _ = i;
        assert_ne!(derive_next_state(AnimStateId::Idle, &i, &t), AnimStateId::Death);
    }
}
