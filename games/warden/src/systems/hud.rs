//! HUD: a single live status line (gold / base HP / wave / phase / selection
//! / meta). Dynamic UI is the observation channel for the closed loop. On the
//! GameOver/Win screens it also shows the meta purchase keys (ME1).

use bevy::prelude::*;

use crate::resources::{
    BaseHp, Economy, Hand, MetaState, SelectedTower, TowerDefs, UPGRADE1_COST, UPGRADE2_COST,
    WaveChoice, WaveState,
};
use crate::states::GameState;

#[derive(Component)]
pub struct HudText;

pub fn spawn_hud(mut commands: Commands) {
    commands.spawn((
        HudText,
        Text::new(""),
        TextFont {
            font_size: FontSize::Px(20.0),
            ..default()
        },
        TextColor(Color::WHITE),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(12.0),
            left: Val::Px(12.0),
            ..default()
        },
    ));
}

pub fn update_hud(
    mut q: Query<&mut Text, With<HudText>>,
    economy: Res<Economy>,
    base: Res<BaseHp>,
    wave: Res<WaveState>,
    selected: Res<SelectedTower>,
    defs: Res<TowerDefs>,
    hand: Res<Hand>,
    choice: Res<WaveChoice>,
    meta: Res<MetaState>,
    state: Res<State<GameState>>,
) {
    let sel_name = defs
        .list
        .get(selected.tower_index)
        .map(|d| d.name)
        .unwrap_or("?");
    let mut text = format!(
        "gold={} | base={}/{} | wave={} | {:?} | sel={}({}) owned=[{}] | meta={} — WASD move · 1-4 select · E place · F fuse · Space next wave",
        economy.gold,
        base.hp,
        base.max_hp,
        wave.current + 1,
        wave.phase,
        selected.tower_index,
        sel_name,
        hand.owned_towers.iter().map(|i| i.to_string()).collect::<Vec<_>>().join(","),
        meta.coins,
    );
    if choice.pending {
        let opts = choice
            .options
            .iter()
            .enumerate()
            .map(|(i, o)| format!("[{}]{}", i + 1, o.label))
            .collect::<Vec<_>>()
            .join("  ");
        text.push_str(&format!("  | CHOOSE: {opts}"));
    }
    if matches!(state.get(), GameState::GameOver | GameState::Win) {
        text.push_str(&format!(
            "  | META: [1] archer-in-hand {}c [2] +20 start gold {}c (press P to retry)",
            UPGRADE1_COST, UPGRADE2_COST
        ));
    }
    for mut t in &mut q {
        t.0 = text.clone();
    }
}
