//! Minimal HUD: one bottom text line (gold / population / selection /
//! outcome). Consumes the base SelectionPanelData projection. Cache-key
//! compare before write — the macOS CJK-reorder pitfall (D9 greybox).

use bevy::prelude::*;
use warbase::prelude::*;

#[derive(Component)]
pub struct HudText;

#[derive(PartialEq)]
struct HudKey {
    gold: u32,
    used: u8,
    cap: u8,
    selected: usize,
    detail: Option<(f32, f32)>,
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
            bottom: Val::Px(10.0),
            left: Val::Px(12.0),
            ..default()
        },
    ));
}

pub fn refresh_hud(
    res: Res<Resources>,
    pop: Res<Population>,
    panel: Res<SelectionPanelData>,
    flow: Res<MatchFlow>,
    mut q: Query<&mut Text, With<HudText>>,
) {
    let hp = panel.detail.as_ref().and_then(|d| d.hp);
    let key = HudKey {
        gold: res.gold,
        used: pop.used,
        cap: pop.cap,
        selected: panel.names.len(),
        detail: hp,
        outcome: match &*flow {
            MatchFlow::Running => String::new(),
            MatchFlow::Won(r) => format!("胜利 —— {r}"),
            MatchFlow::Lost(r) => format!("失败 —— {r}"),
        },
    };
    // Write-before-compare: skip identical rewrites (per-frame CJK reflow).
    if let Ok(mut text) = q.single_mut() {
        let names = if panel.names.is_empty() {
            "无".to_string()
        } else if panel.names.len() == 1 {
            panel.names[0].clone()
        } else {
            format!("{} 个单位", panel.names.len())
        };
        let hp_text = match key.detail {
            Some((cur, max)) => format!(" hp{cur:.0}/{max:.0}"),
            None => String::new(),
        };
        let next = format!(
            "金 {}  人口 {}/{}  |  选中：{}{}  |  右键移动 · A 攻击移动 · P 巡逻 · T/F 训练(选建筑) · Q 圣光 · R 科研 · 1 用药 · B 买药{}",
            key.gold,
            key.used,
            key.cap,
            names,
            hp_text,
            if key.outcome.is_empty() { String::new() } else { format!("  ——  {}", key.outcome) },
        );
        if **text != next {
            *text = Text::new(next);
        }
    }
}
