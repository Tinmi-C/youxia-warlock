//! Waves (WX6-6d): the BASE's WaveDirector is the state machine — the wave
//! table ships as a base `WaveTable` (built from the warden schedule by the
//! code bridge; warden.ron takes the content over with WX6-6f), spawning is
//! `run_director`, settling/reward/advance is the director's cleared path.
//! What stays warden-side:
//!   - `sync_wave_state`: WaveState as the derived HUD/gating mirror
//!     (phase/current/active; written ONLY on change so `refresh_shop`'s
//!     change-detection hook fires once per transition).
//!   - `on_wave_cleared`: the game hooks — 三选一 generation (AC4) on a
//!     non-final clear, GameState::Win on the final one. The reward itself
//!     is the director's `reward_gold` granted through EconCommand.
//! Capability cards: WA1/WA2 (director), WA3 (check_lose until WX6-6e),
//! AC4 (three-choose-one).

use bevy::prelude::*;
use rand::RngExt;

use crate::components::Enemy;
use crate::resources::{
    ChoiceKind, ChoiceOption, EnemyDefs, Hand, StatKind, TowerDefs, WaveChoice, WavePhase,
    WaveSchedule, WaveState,
};
use crate::states::GameState;
use crate::systems::enemy::RangeDummy;

/// The warden schedule as a BASE WaveTable (WX6-6d code bridge): enemy
/// indices map to def names through EnemyDefs, the global 0.5 s spawn
/// interval becomes each spec's interval, rewards ride `reward_gold`
/// (granted by the director via EconCommand). Manual advance: waves wait
/// for StartWave, exactly warden's dual-speed rhythm.
pub fn base_wave_table(schedule: &WaveSchedule, defs: &EnemyDefs) -> warbase::prelude::WaveTable {
    warbase::prelude::WaveTable {
        waves: schedule
            .waves
            .iter()
            .map(|w| warbase::prelude::WaveEntry {
                spawns: w
                    .entries
                    .iter()
                    .filter_map(|e| {
                        let name = defs.list.get(e.enemy_index).map(|d| d.name)?;
                        Some(warbase::prelude::SpawnSpec {
                            unit: name.to_string(),
                            count: e.count,
                            interval: 0.5,
                            path: 0,
                            delay: 0.0,
                            hp_scale: 1.0,
                        })
                    })
                    .collect(),
                boss_timeout: None,
                preview: String::new(),
                reward_gold: w.reward,
            })
            .collect(),
        advance: warbase::prelude::Advance::Manual,
        early_call_bonus: None,
    }
}

/// Mirror the base director into warden's WaveState (HUD + gating reads it;
/// tests assert on it). Writes only on actual change so Res::is_changed
/// consumers (the AC2 shop refresh) fire once per transition, not per frame.
pub fn sync_wave_state(
    director: Res<warbase::prelude::WaveDirector>,
    mut wave: ResMut<WaveState>,
    enemies: Query<(), (With<Enemy>, Without<RangeDummy>)>,
) {
    let phase = if matches!(
        director.phase,
        warbase::prelude::DirectorPhase::Spawning
    ) {
        WavePhase::Combat
    } else {
        WavePhase::Intermission
    };
    let current = director.next_wave as u32;
    let active = enemies.iter().count() as u32;
    if wave.phase != phase {
        wave.phase = phase;
    }
    if wave.current != current {
        wave.current = current;
    }
    if wave.active != active {
        wave.active = active;
    }
}

/// Game hooks on the director's cleared path (WA2/AC4): the reward already
/// landed through the director's EconCommand; warden adds the 三选一 after
/// non-final waves. The Win transition itself is the base rules domain's
/// (SurviveWaves -> MatchEnded -> `apply_match_ended`) since WX6-6e.
pub fn on_wave_cleared(
    mut cleared: MessageReader<warbase::prelude::WaveCleared>,
    schedule: Res<WaveSchedule>,
    defs: Res<TowerDefs>,
    mut choice: ResMut<WaveChoice>,
    hand: Res<Hand>,
    mut rng: ResMut<crate::resources::RunRng>,
) {
    for c in cleared.read() {
        if c.0 + 1 < schedule.waves.len() {
            generate_choices(&mut *choice, &*defs, &*hand, &mut rng);
        }
    }
}

/// WA3 (WX6-6e): the base's MatchEnded drives the warden state machine —
/// Won -> Win, Lost -> GameOver. The single outcome channel (rules_tick:
/// lives out / survived all waves).
pub fn apply_match_ended(
    mut ended: MessageReader<warbase::prelude::MatchEnded>,
    state: Res<State<GameState>>,
    mut next: ResMut<NextState<GameState>>,
) {
    for e in ended.read() {
        if *state.get() != GameState::Playing {
            continue;
        }
        match &e.outcome {
            Ok(reason) => {
                next.set(GameState::Win);
                info!("[rule] match won ({reason}) -> Win");
            }
            Err(reason) => {
                next.set(GameState::GameOver);
                info!("[rule] match lost ({reason}) -> GameOver");
            }
        }
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
    rng: &mut crate::resources::RunRng,
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

