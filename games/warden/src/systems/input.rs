//! Player input for the closed loop. One-card workhorse:
//!   - 1/2/3 picks the pending "三选一" option (AC4)
//!   - Space starts the next wave (Intermission only)
//!   - F fuses the first matching pair (TO5 legacy trigger; retires with UI11)
//! Tower selection/build moved to the command card (UI10, requirements §16):
//! the old WASD cursor and the 1-4/E keyboard flow are retired.

use std::collections::VecDeque;

use bevy::prelude::*;

use crate::resources::{
    Boosts, ChoiceKind, Hand, StatKind, WaveChoice, WavePhase, WaveSchedule, WaveState,
};
use crate::states::GameState;

/// 1/2/3 picks the pending "三选一" option (AC4) and applies its effect.
pub fn choose_choice(
    keys: Res<ButtonInput<KeyCode>>,
    panel: Res<crate::resources::DebugPanelCapture>,
    mut choice: ResMut<WaveChoice>,
    mut boosts: ResMut<Boosts>,
    mut hand: ResMut<Hand>,
) {
    if !choice.pending {
        return;
    }
    // Typing into a debug-panel field must not pick options (DB1).
    if panel.keyboard {
        return;
    }
    let idx = if keys.just_pressed(KeyCode::Digit1) {
        Some(0)
    } else if keys.just_pressed(KeyCode::Digit2) {
        Some(1)
    } else if keys.just_pressed(KeyCode::Digit3) {
        Some(2)
    } else {
        None
    };
    let Some(i) = idx else {
        return;
    };
    apply_choice(&mut choice, &mut boosts, &mut hand, i);
}

/// Shared choice application for the digit keys and the UI choice cards (UI2).
pub fn apply_choice(
    choice: &mut WaveChoice,
    boosts: &mut Boosts,
    hand: &mut Hand,
    i: usize,
) {
    let Some(opt) = choice.options.get(i) else {
        return;
    };
    // Clone the kind out so we can mutate `choice` and still log it.
    let kind = opt.kind.clone();
    match &kind {
        ChoiceKind::StatBoost { tower_type, stat } => match stat {
            StatKind::Damage => boosts.damage_mult[*tower_type] *= 1.2,
            StatKind::AttackSpeed => boosts.attack_speed_mult[*tower_type] *= 1.2,
            StatKind::Range => boosts.range_mult[*tower_type] *= 1.15,
        },
        ChoiceKind::GoldBoost => {
            boosts.kill_mult *= 1.2;
        }
        ChoiceKind::GetTower { tower_type } => {
            if !hand.owned_towers.contains(tower_type) {
                hand.owned_towers.push(*tower_type);
            }
        }
    }
    choice.pending = false;
    info!("[input] chose {:?}", kind);
}

/// Space begins the next wave (only from Intermission). Delegates to the shared
/// `try_start_wave` so the on-screen button (UI2) behaves identically.
pub fn start_next_wave(
    keys: Res<ButtonInput<KeyCode>>,
    panel: Res<crate::resources::DebugPanelCapture>,
    mut wave: ResMut<WaveState>,
    schedule: Res<WaveSchedule>,
    choice: Res<WaveChoice>,
    state: Res<State<GameState>>,
) {
    // Typing (space) into a debug-panel field must not start waves (DB1).
    if panel.keyboard {
        return;
    }
    if !keys.just_pressed(KeyCode::Space) {
        return;
    }
    try_start_wave(&mut wave, &schedule, &choice, &state);
}

/// Shared wave-start logic for the Space key and the "开始下一波" button (UI2).
/// Returns true when the wave actually started.
pub fn try_start_wave(
    wave: &mut WaveState,
    schedule: &WaveSchedule,
    choice: &WaveChoice,
    state: &State<GameState>,
) -> bool {
    if wave.phase != WavePhase::Intermission
        || choice.pending
        || *state.get() != GameState::Playing
    {
        return false;
    }
    if (wave.current as usize) >= schedule.waves.len() {
        return false;
    }
    let wave_def = &schedule.waves[wave.current as usize];
    let mut q = VecDeque::new();
    for entry in &wave_def.entries {
        for _ in 0..entry.count {
            q.push_back(entry.enemy_index);
        }
    }
    wave.spawn_queue = q;
    wave.phase = WavePhase::Combat;
    wave.active = 0;
    wave.spawn_timer = 0.0;
    info!("[wave] wave {} started", wave.current + 1);
    true
}
