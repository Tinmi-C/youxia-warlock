//! Waves: spawn a wave's enemies over time, and settle the wave (reward) when
//! all are gone. Also owns the lose/win transition.
//! Capability cards: WA1 (config/spawn), WA2 (reward + intermission), WA3 (lose/win).

use bevy::prelude::*;

use crate::resources::{
    BaseHp, ChoiceKind, ChoiceOption, Economy, EnemyDefs, Hand, PathInfo, WaveChoice, WavePhase,
    WaveSchedule, WaveState,
};
use crate::states::GameState;
use crate::systems::enemy::spawn_enemy;

/// Spawn the current wave's enemies, spaced by `WaveState::spawn_interval`.
pub fn wave_spawn(
    time: Res<Time>,
    mut wave: ResMut<WaveState>,
    defs: Res<EnemyDefs>,
    path: Res<PathInfo>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    if wave.phase != WavePhase::Combat || wave.spawn_queue.is_empty() {
        return;
    }
    wave.spawn_timer -= time.delta_secs();
    if wave.spawn_timer <= 0.0 {
        if let Some(index) = wave.spawn_queue.pop_front() {
            spawn_enemy(
                &mut commands,
                &*defs,
                &*path,
                index,
                &mut *meshes,
                &mut *materials,
            );
            wave.active += 1;
        }
        wave.spawn_timer = wave.spawn_interval;
    }
}

/// When a combat wave has fully spawned and every enemy is resolved, award the
/// reward, return to Intermission, advance — or Win on the last wave. On a
/// non-final wave it also deals a "三选一" choice (AC4).
pub fn wave_resolve(
    mut wave: ResMut<WaveState>,
    schedule: Res<WaveSchedule>,
    mut economy: ResMut<Economy>,
    hand: Res<Hand>,
    mut choice: ResMut<WaveChoice>,
    mut next: ResMut<NextState<GameState>>,
) {
    if wave.phase != WavePhase::Combat || !wave.spawn_queue.is_empty() || wave.active != 0 {
        return;
    }
    let Some(wave_def) = schedule.waves.get(wave.current as usize) else {
        return;
    };
    economy.gold += wave_def.reward;
    wave.phase = WavePhase::Intermission;
    wave.current += 1;
    info!("[wave] {} cleared, +{} gold -> intermission", wave.current, wave_def.reward);

    if wave.current as usize >= schedule.waves.len() {
        next.set(GameState::Win);
        info!("[wave] all waves cleared -> Win");
    } else {
        generate_choices(&mut *choice, &*hand, wave.current);
    }
}

/// Build a deterministic 3-option choice (AC4). Full randomness is a polish card;
/// this guarantees 3 distinct options and a get-tower that avoids owned towers.
fn generate_choices(choice: &mut WaveChoice, hand: &Hand, salt: u32) {
    let stat_type = base_type(salt, 0);
    let mut get_type = base_type(salt, 2);
    // Avoid handing out a tower the player already owns.
    if hand.owned_towers.contains(&get_type) {
        if let Some(free) = (0..4).find(|i| !hand.owned_towers.contains(i)) {
            get_type = free;
        }
    }
    choice.options = vec![
        ChoiceOption {
            label: "Tower damage +20%",
            kind: ChoiceKind::StatBoost { tower_type: stat_type },
        },
        ChoiceOption {
            label: "Kill gold +20%",
            kind: ChoiceKind::GoldBoost,
        },
        ChoiceOption {
            label: "Get a tower",
            kind: ChoiceKind::GetTower { tower_type: get_type },
        },
    ];
    choice.pending = true;
    info!("[wave] three-choose-one ready");
}

/// Deterministic base-tower-type pick (placeholder for random selection).
fn base_type(salt: u32, offset: u32) -> usize {
    ((salt + offset) % 4) as usize
}

/// Lose condition: base HP reached 0 while playing.
pub fn check_lose(
    base: Res<BaseHp>,
    state: Res<State<GameState>>,
    mut next: ResMut<NextState<GameState>>,
) {
    if base.hp == 0 && *state.get() == GameState::Playing {
        next.set(GameState::GameOver);
        info!("[wave] base destroyed -> GameOver");
    }
}
