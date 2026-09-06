//! HUD: a single live status line (gold / base HP / wave / phase / selection).
//! Dynamic UI is the observation channel for the closed loop.

use bevy::prelude::*;

use crate::resources::{BaseHp, Economy, Hand, SelectedTower, TowerDefs, WaveChoice, WaveState};

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
) {
    let sel_name = defs
        .list
        .get(selected.tower_index)
        .map(|d| d.name)
        .unwrap_or("?");
    let mut text = format!(
        "gold={} | base={}/{} | wave={} | {:?} | sel={}({}) owned=[{}]  — WASD move · 1-4 select · E place · F fuse · Space next wave",
        economy.gold,
        base.hp,
        base.max_hp,
        wave.current + 1,
        wave.phase,
        selected.tower_index,
        sel_name,
        hand.owned_towers.iter().map(|i| i.to_string()).collect::<Vec<_>>().join(","),
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
    for mut t in &mut q {
        t.0 = text.clone();
    }
}
