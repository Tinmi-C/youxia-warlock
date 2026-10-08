//! Minimal HUD: one text line (resources / stock / wave / selection /
//! outcome). Cache-key compare before write — the macOS CJK-reorder pitfall.

use bevy::prelude::*;
use warbase::prelude::*;

use crate::systems::gameplay::SelectedTower;

#[derive(Component)]
pub struct HudText;

#[derive(PartialEq)]
struct HudKey {
    gold: u32,
    wood: u32,
    pop: u8,
    cap: u8,
    stock: u32,
    wave: usize,
    spawning: bool,
    selected: String,
    outcome: String,
}

pub fn spawn_hud(mut commands: Commands) {
    commands.spawn((
        HudText,
        Text::new(""),
        TextFont { font_size: FontSize::Px(18.0), ..default() },
        TextColor(Color::WHITE),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(10.0),
            left: Val::Px(12.0),
            ..default()
        },
    ));
}

pub fn refresh_hud(
    res: Res<Resources>,
    pop: Res<Population>,
    stock: Res<StockCount>,
    director: Res<WaveDirector>,
    selected: Res<SelectedTower>,
    flow: Res<MatchFlow>,
    mut q: Query<&mut Text, With<HudText>>,
) {
    let key = HudKey {
        gold: res.gold,
        wood: res.wood,
        pop: pop.used,
        cap: pop.cap,
        stock: stock.0,
        wave: director.running.map(|w| w + 1).unwrap_or(director.next_wave + 1),
        spawning: matches!(director.phase, DirectorPhase::Spawning),
        selected: selected.0.clone(),
        outcome: match &*flow {
            MatchFlow::Running => String::new(),
            MatchFlow::Won(r) => format!("WIN: {r}"),
            MatchFlow::Lost(r) => format!("LOSE: {r}"),
        },
    };
    // Write-before-compare: skip identical rewrites (per-frame CJK reflow).
    if let Ok(mut text) = q.single_mut() {
        let next = format!(
            "金{} 木{}/{} 人口{}/{} 存量{} 波次{}{} [{}] WASD移动 1-4选塔 空格放置 X出售{}",
            key.gold,
            key.wood,
            key.cap,
            key.pop,
            key.cap,
            key.stock,
            key.wave,
            if key.spawning { "(进行中)" } else { "" },
            key.selected,
            if key.outcome.is_empty() { String::new() } else { format!(" —— {}", key.outcome) },
        );
        if **text != next {
            *text = Text::new(next);
        }
    }
}
