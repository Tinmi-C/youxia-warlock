//! Minimal HUD: one top text line (round/phase + the four wallets and life
//! totals). Cache-key compare before write — the macOS CJK-reorder pitfall
//! (D9 greybox).

use bevy::prelude::*;
use warbase::prelude::*;

use crate::resources::{LivesBoard, Phase, Purse, RoundFlow};

#[derive(Component)]
pub struct HudText;

#[derive(PartialEq)]
struct HudKey {
    round: u32,
    build: bool,
    gold: [u32; 4],
    lives: [u32; 4],
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
    purse: Res<Purse>,
    lives: Res<LivesBoard>,
    flow: Res<RoundFlow>,
    outcome: Res<MatchFlow>,
    mut q: Query<&mut Text, With<HudText>>,
) {
    let key = HudKey {
        round: flow.round,
        build: flow.phase == Phase::Build,
        gold: purse.0,
        lives: lives.0,
        outcome: match &*outcome {
            MatchFlow::Running => String::new(),
            MatchFlow::Won(r) => format!("胜利——{r}"),
            MatchFlow::Lost(r) => format!("失败——{r}"),
        },
    };
    if let Ok(mut text) = q.single_mut() {
        let phase = if key.build { "建造期" } else { "战斗期" };
        let next = format!(
            "回合 {} · {}  |  P0 金{} 命{}  P1 金{} 命{}  P2 金{} 命{}  P3 金{} 命{}{}",
            key.round,
            phase,
            key.gold[0],
            key.lives[0],
            key.gold[1],
            key.lives[1],
            key.gold[2],
            key.lives[2],
            key.gold[3],
            key.lives[3],
            if key.outcome.is_empty() { String::new() } else { format!("  ——  {}", key.outcome) },
        );
        if **text != next {
            *text = Text::new(next);
        }
    }
}
