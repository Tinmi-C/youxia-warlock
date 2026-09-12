//! Waves: spawn a wave's enemies over time, and settle the wave (reward) when
//! all are gone. Also owns the lose/win transition.
//! Capability cards: WA1 (config/spawn), WA2 (reward + intermission), WA3 (lose/win),
//! AC4 (three-choose-one).

use bevy::prelude::*;
use rand::RngExt;

use crate::resources::{
    BaseHp, ChoiceKind, ChoiceOption, Economy, EnemyDefs, Hand, PathInfo, RunRng, StatKind,
    TowerDefs, WaveChoice, WavePhase, WaveSchedule, WaveState,
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
    defs: Res<TowerDefs>,
    mut economy: ResMut<Economy>,
    hand: Res<Hand>,
    mut choice: ResMut<WaveChoice>,
    mut rng: ResMut<RunRng>,
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
        generate_choices(&mut *choice, &*defs, &*hand, &mut rng);
    }
}

/// The categories a "三选一" option can offer (AC4 polish). `Stat` is one of
/// three stat boosts; `Gold` is kill-gold; `Get` is a free tower type.
#[derive(Clone, Copy, PartialEq)]
enum Category {
    Stat(StatKind),
    Gold,
    Get,
}

impl Category {
    /// The `ChoiceKind` this category produces for a given tower type.
    fn kind(&self, tower_type: usize) -> ChoiceKind {
        match self {
            Category::Stat(stat) => ChoiceKind::StatBoost {
                tower_type,
                stat: *stat,
            },
            Category::Gold => ChoiceKind::GoldBoost,
            Category::Get => ChoiceKind::GetTower { tower_type },
        }
    }
}

/// Build a randomized 3-option choice (AC4 polish). Samples 3 *distinct*
/// categories from {stat-damage, stat-speed, stat-range, gold, get-tower} via
/// `RunRng` (Fisher-Yates on a category pool, then take the first 3), and rolls
/// the target tower type for stat/get options. The get-tower target avoids a
/// type the player already owns when a free type exists.
/// `pub(crate)` so the debug "choice" scenario (DB2) deals a REAL pending
/// choice (same generator) instead of a hand-built fake.
pub(crate) fn generate_choices(
    choice: &mut WaveChoice,
    defs: &TowerDefs,
    hand: &Hand,
    rng: &mut RunRng,
) {
    let mut pool = vec![
        Category::Stat(StatKind::Damage),
        Category::Stat(StatKind::AttackSpeed),
        Category::Stat(StatKind::Range),
        Category::Gold,
        Category::Get,
    ];
    // In-place Fisher-Yates so the first 3 are three distinct categories.
    for i in (1..pool.len()).rev() {
        let j = rng.0.random_range(0..=i);
        pool.swap(i, j);
    }
    let mut get_type = rng.0.random_range(0..4);
    if hand.owned_towers.contains(&get_type) {
        if let Some(free) = (0..4).find(|i| !hand.owned_towers.contains(i)) {
            get_type = free;
        }
    }
    choice.options = pool[..3]
        .iter()
        .map(|cat| {
            // Build the kind first, then derive both language labels from the
            // same mapping (l10n::choice_label is the single source of truth).
            let kind = match cat {
                Category::Stat(_) => {
                    let t = rng.0.random_range(0..4);
                    cat.kind(t)
                }
                Category::Gold => cat.kind(0),
                Category::Get => cat.kind(get_type),
            };
            ChoiceOption {
                label: crate::systems::l10n::choice_label(&kind, defs, crate::resources::Lang::Zh),
                label_en: crate::systems::l10n::choice_label(&kind, defs, crate::resources::Lang::En),
                kind,
            }
        })
        .collect();
    choice.pending = true;
    info!("[wave] three-choose-one ready (randomized)");
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
