//! Minimal HUD: one top text line (round/phase placeholder + the four
//! wallets). Cache-key compare before write — the macOS CJK-reorder pitfall
//! (D9 greybox).

use bevy::prelude::*;
use warbase::prelude::*;

use crate::resources::Purse;

#[derive(Component)]
pub struct HudText;

#[derive(PartialEq)]
struct HudKey {
    gold: [u32; 4],
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

pub fn refresh_hud(purse: Res<Purse>, flow: Res<MatchFlow>, mut q: Query<&mut Text, With<HudText>>) {
    let key = HudKey { gold: purse.0 };
    if let Ok(mut text) = q.single_mut() {
        let outcome = match &*flow {
            MatchFlow::Running => String::new(),
            MatchFlow::Won(r) => format!("  ——  胜利：{r}"),
            MatchFlow::Lost(r) => format!("  ——  失败：{r}"),
        };
        let next = format!(
            "P0 金{}  P1 金{}  P2 金{}  P3 金{}{}",
            key.gold[0], key.gold[1], key.gold[2], key.gold[3], outcome,
        );
        if **text != next {
            *text = Text::new(next);
        }
    }
}
