//! HUD: a Chinese status line + a static help line (UI2). Dynamic UI is the
//! observation channel for the closed loop. The status line also shows the
//! meta purchase hints on the GameOver/Win screens (ME1).
//! Capability cards: UI4 (HUD), UI2 (panel + Chinese copy), UI6 (bundled font).

use bevy::prelude::*;
use bevy::text::FontSource;

use crate::resources::{
    BaseHp, Economy, Hand, Lang, MetaState, SelectedTower, TowerDefs, UPGRADE1_COST, UPGRADE2_COST,
    WaveChoice, WavePhase, WaveSchedule, WaveState,
};
use crate::states::GameState;
use crate::systems::l10n;

/// Marker on the live status line (updated every frame by `update_hud`).
#[derive(Component)]
pub struct HudText;

/// Marker on the static help line (rewritten only when the language changes).
#[derive(Component)]
pub struct HelpText;

/// The bundled open-source CJK font (UI6): Noto Sans SC under SIL OFL 1.1
/// (license at `assets/fonts/OFL.txt`). Shipping the file makes rendering
/// identical on every machine (Windows used Microsoft YaHei, macOS fell back
/// to PingFang) and keeps distribution licensing clean. Headless tests have
/// no AssetServer, so `setup_ui_font` falls back to `FontSource::Default`.
#[derive(Resource)]
pub struct UiFont(pub FontSource);

/// Asset path of the bundled font, relative to the `assets/` root.
pub const UI_FONT_PATH: &str = "fonts/NotoSansSC-VariableFont_wght.ttf";

/// Startup: resolve the UI font once; runs before any UI-building system.
pub fn setup_ui_font(mut commands: Commands, server: Option<Res<AssetServer>>) {
    let source = match &server {
        Some(s) => FontSource::Handle(s.load(UI_FONT_PATH)),
        None => FontSource::default(),
    };
    commands.insert_resource(UiFont(source));
    info!(
        "[ui6] ui font ready ({})",
        if server.is_some() { "bundled asset" } else { "default fallback" }
    );
}

pub fn spawn_hud(mut commands: Commands, font: Res<UiFont>, lang: Res<Lang>) {
    commands.spawn((
        HudText,
        Text::new(""),
        TextFont {
            font: font.0.clone(),
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
    // The text is re-picked in `update_hud` when the language toggles (UI7).
    commands.spawn((
        HelpText,
        Text::new(l10n::HELP_LINE.pick(*lang)),
        TextFont {
            font: font.0.clone(),
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
    lang: Res<Lang>,
    mut help: Query<&mut Text, (With<HelpText>, Without<HudText>)>,
) {
    let lang = *lang;
    let sel_name = defs
        .list
        .get(selected.tower_index)
        .map(|d| l10n::tower_label(d, lang))
        .unwrap_or("?");
    let phase_label = match wave.phase {
        WavePhase::Intermission => l10n::PHASE_BUILD.pick(lang),
        WavePhase::Combat => l10n::PHASE_COMBAT.pick(lang),
    };
    let wave_seg = l10n::hud_wave(
        lang,
        (wave.current + 1) as usize,
        schedule.waves.len(),
        phase_label,
    );
    let hand_seg = hand
        .owned_towers
        .iter()
        .map(|i| l10n::tower_label(&defs.list[*i], lang))
        .collect::<Vec<_>>()
        .join(",");
    let mut text = format!(
        "{}{} · {}{}/{} · {} · {}{} · {}[{}] · {}{}",
        l10n::GOLD_EQ.pick(lang),
        economy.gold,
        l10n::BASE_EQ.pick(lang),
        base.hp,
        base.max_hp,
        wave_seg,
        l10n::SEL.pick(lang),
        sel_name,
        l10n::HAND.pick(lang),
        hand_seg,
        l10n::COINS_EQ.pick(lang),
        meta.coins,
    );
    if choice.pending {
        let opts = choice
            .options
            .iter()
            .enumerate()
            .map(|(i, o)| {
                format!(
                    "[{}]{}",
                    i + 1,
                    match lang {
                        Lang::Zh => o.label.as_str(),
                        Lang::En => o.label_en.as_str(),
                    }
                )
            })
            .collect::<Vec<_>>()
            .join("  ");
        text.push_str(&format!("  | {}{opts}", l10n::CHOICE.pick(lang)));
    }
    if matches!(state.get(), GameState::GameOver | GameState::Win) {
        text.push_str("  | ");
        text.push_str(&l10n::hud_settle(lang, UPGRADE1_COST, UPGRADE2_COST));
    }
    for mut t in &mut q {
        t.0 = text.clone();
    }
    // The help line only rewrites when the language actually changed (UI7).
    for mut h in &mut help {
        let want = l10n::HELP_LINE.pick(lang);
        if h.0 != want {
            h.0 = want.to_string();
        }
    }
}
