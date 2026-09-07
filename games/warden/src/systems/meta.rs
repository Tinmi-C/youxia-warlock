//! Meta (ME1): cross-run currency + one upgrade chain (requirements §13).
//! Persistence = tiny key=value file at `MetaSavePath`; a missing/corrupt
//! file falls back to defaults. Purchase keys (1/2) are active on the
//! GameOver / Win screens.

use bevy::prelude::*;

use crate::resources::{
    Economy, MetaSavePath, MetaState, META_REWARD_DEATH, META_REWARD_WIN, UPGRADE1_COST,
    UPGRADE2_COST,
};
use crate::states::GameState;

/// Startup: load the save file if it exists (missing/corrupt -> defaults).
pub fn load_meta(mut meta: ResMut<MetaState>, path: Res<MetaSavePath>) {
    let Ok(text) = std::fs::read_to_string(&path.0) else {
        info!("[meta] no save at {}, starting fresh", path.0.display());
        return;
    };
    for line in text.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        match (key.trim(), value.trim()) {
            ("coins", v) => meta.coins = v.parse().unwrap_or(meta.coins),
            ("upgrade1", v) => meta.upgrade1 = v == "1",
            ("upgrade2", v) => meta.upgrade2 = v == "1",
            _ => {}
        }
    }
    info!(
        "[meta] loaded coins={} upgrade1={} upgrade2={}",
        meta.coins, meta.upgrade1, meta.upgrade2
    );
}

/// Startup (after load, before the opening hand is dealt): upgrade 2 grants
/// +20 starting gold for this run (requirements §13).
pub fn apply_meta_on_run_start(meta: Res<MetaState>, mut economy: ResMut<Economy>) {
    if meta.upgrade2 {
        economy.gold += 20;
        info!("[meta] upgrade2 -> starting gold {}", economy.gold);
    }
}

/// GameOver: +15 meta coins (requirements §13), persisted immediately.
pub fn award_on_death(mut meta: ResMut<MetaState>, path: Res<MetaSavePath>) {
    meta.coins += META_REWARD_DEATH;
    save(&meta, &path);
    info!("[meta] death reward, coins={}", meta.coins);
}

/// Win: +30 meta coins (requirements §13), persisted immediately.
pub fn award_on_win(mut meta: ResMut<MetaState>, path: Res<MetaSavePath>) {
    meta.coins += META_REWARD_WIN;
    save(&meta, &path);
    info!("[meta] win reward, coins={}", meta.coins);
}

/// On the GameOver/Win screens, 1/2 spend meta coins on the two upgrades.
pub fn buy_upgrades(
    keys: Res<ButtonInput<KeyCode>>,
    state: Res<State<GameState>>,
    mut meta: ResMut<MetaState>,
    path: Res<MetaSavePath>,
) {
    if !matches!(state.get(), GameState::GameOver | GameState::Win) {
        return;
    }
    if keys.just_pressed(KeyCode::Digit1) {
        buy_upgrade(&mut meta, &path, 1);
    }
    if keys.just_pressed(KeyCode::Digit2) {
        buy_upgrade(&mut meta, &path, 2);
    }
}

/// Shared upgrade purchase for the digit keys and the UI5 buttons. Spends the
/// cost if the upgrade is not owned and affordable, then persists. Returns true
/// if the upgrade was applied. `which` is 1 or 2.
pub fn buy_upgrade(meta: &mut MetaState, path: &MetaSavePath, which: usize) -> bool {
    let bought = match which {
        1 if !meta.upgrade1 && meta.coins >= UPGRADE1_COST => {
            meta.coins -= UPGRADE1_COST;
            meta.upgrade1 = true;
            true
        }
        2 if !meta.upgrade2 && meta.coins >= UPGRADE2_COST => {
            meta.coins -= UPGRADE2_COST;
            meta.upgrade2 = true;
            true
        }
        _ => false,
    };
    if bought {
        save(meta, path);
        info!("[meta] bought upgrade{which}");
    }
    bought
}

/// Write the tiny key=value save file. I/O errors are logged, never fatal.
fn save(meta: &MetaState, path: &MetaSavePath) {
    let text = format!(
        "coins={}\nupgrade1={}\nupgrade2={}\n",
        meta.coins,
        meta.upgrade1 as u8,
        meta.upgrade2 as u8
    );
    if let Err(e) = std::fs::write(&path.0, text) {
        warn!("[meta] save failed: {e}");
    }
}
