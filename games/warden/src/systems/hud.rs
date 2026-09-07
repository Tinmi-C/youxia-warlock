//! HUD: a Chinese status line + a static help line (UI2). Dynamic UI is the
//! observation channel for the closed loop. The status line also shows the
//! meta purchase hints on the GameOver/Win screens (ME1).
//! Capability cards: UI4 (HUD), UI2 (panel + Chinese copy).

use bevy::prelude::*;
use bevy::text::FontSource;

use crate::resources::{
    BaseHp, Economy, Hand, MetaState, SelectedTower, TowerDefs, UPGRADE1_COST, UPGRADE2_COST,
    WaveChoice, WavePhase, WaveSchedule, WaveState,
};
use crate::states::GameState;

/// Marker on the live status line (updated every frame by `update_hud`).
#[derive(Component)]
pub struct HudText;

/// The UI font: resolve the system "Microsoft YaHei" family via
/// `bevy_text/system_font_discovery` so Chinese text renders (parley fontique
/// falls back automatically if the family is missing). Ships no font file;
/// bundling an open CJK font for distribution is a follow-up card.
pub fn ui_font() -> FontSource {
    FontSource::Family("Microsoft YaHei".into())
}

pub fn spawn_hud(mut commands: Commands) {
    commands.spawn((
        HudText,
        Text::new(""),
        TextFont {
            font: ui_font(),
            font_size: FontSize::Px(14.0),
            ..default()
        },
        TextColor(Color::WHITE),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(10.0),
            left: Val::Px(10.0),
            ..default()
        },
    ));
    // Static operation help (UI2): pure-mouse flow first, keys as fallback.
    commands.spawn((
        Text::new(
            "点底部卡片选塔 → 点绿格放置 · 右键取消 · 点两座已放塔尝试融合 · 右下按钮或空格 开下一波",
        ),
        TextFont {
            font: ui_font(),
            font_size: FontSize::Px(12.0),
            ..default()
        },
        TextColor(Color::srgb(0.75, 0.75, 0.75)),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(32.0),
            left: Val::Px(10.0),
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
    schedule: Res<WaveSchedule>,
    state: Res<State<GameState>>,
) {
    let sel_name = defs
        .list
        .get(selected.tower_index)
        .map(|d| d.label)
        .unwrap_or("?");
    let phase_label = match wave.phase {
        WavePhase::Intermission => "建造期",
        WavePhase::Combat => "战斗期",
    };
    let mut text = format!(
        "金币={} · 基地={}/{} · 第{}波/共{}（{}） · 选中:{} · 手牌:[{}] · 仓库币={}",
        economy.gold,
        base.hp,
        base.max_hp,
        wave.current + 1,
        schedule.waves.len(),
        phase_label,
        sel_name,
        hand.owned_towers
            .iter()
            .map(|i| defs.list[*i].label)
            .collect::<Vec<_>>()
            .join(","),
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
        text.push_str(&format!("  | 三选一：{opts}"));
    }
    if matches!(state.get(), GameState::GameOver | GameState::Win) {
        text.push_str(&format!(
            "  | 结算：按1 开局必含弓箭手塔（{}币） · 按2 开局+20金（{}币） · P 再来一局",
            UPGRADE1_COST, UPGRADE2_COST
        ));
    }
    for mut t in &mut q {
        t.0 = text.clone();
    }
}
