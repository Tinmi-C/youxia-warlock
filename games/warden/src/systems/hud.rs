//! HUD（ui-polish 改版）：晴昼要塞的「指挥甲板」——
//!   顶匾（白灰石）：金币 / 基地 / 波次 三格，标签小字 + 大数字，瞄一眼就够；
//!   军报（羊皮纸）：备战期的下一波军情（高光时刻，开波即收起）、
//!     技能瞄准提示、三选一提示、终局结算文案——`HudText` 的现代身份。
//! 数值写入只在变化时落笔（icu 重排教训）。设计基线见 docs/design.md。
//! Capability cards: UI4 (HUD), UI2 (中文文案), UI6 (bundled font),
//! UI8 (next-wave preview), ME1 (settle hints)。

use bevy::prelude::*;
use bevy::text::FontSource;

use crate::resources::{
    Economy, EnemyDefs, Lang, WaveChoice, WavePhase, WaveSchedule, WaveState, UPGRADE1_COST,
    UPGRADE2_COST,
};
use crate::states::GameState;
use crate::systems::l10n;

/// 顶匾金币格标签（随语言切换重写）。
#[derive(Component)]
pub struct GoldLabel;

/// 顶匾金币数值（update_hud 对表改写）。
#[derive(Component)]
pub struct GoldValue;

/// 顶匾基地格标签。
#[derive(Component)]
pub struct ShieldLabel;

/// 顶匾基地血量数值。
#[derive(Component)]
pub struct ShieldValue;

/// 顶匾波次格数值。
#[derive(Component)]
pub struct WaveValue;

/// 军报羊皮纸的正文（历史名 HudText——测试与 l10n 的契约点）。
#[derive(Component)]
pub struct HudText;

/// 军报眉题（随语言切换重写）。
#[derive(Component)]
pub struct ReportHeader;

/// 军报面板根（可见性开关在面板上）。
#[derive(Component)]
pub struct FieldReport;

/// The bundled open-source CJK font (UI6): Noto Sans SC under SIL OFL 1.1
/// (license at the workspace-root `assets/fonts/OFL.txt`, SEAM3b shared
/// single copy — both games read the same file). Shipping the file makes
/// rendering identical on every machine (Windows used Microsoft YaHei, macOS
/// fell back to PingFang) and keeps distribution licensing clean. Loaded by
/// BYTES from the shared root (AssetServer roots are per-crate), so the
/// path works windowed and headless alike; bare headless tests have no
/// `Assets<Font>` store and fall back to `FontSource::Default`.
#[derive(Resource)]
pub struct UiFont(pub FontSource);

/// File name of the shared font inside the workspace-root `assets/fonts/`.
pub const UI_FONT_FILE: &str = "NotoSansSC-VariableFont_wght.ttf";

/// Absolute path of the shared font file (workspace root, single copy).
pub fn font_file() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../assets/fonts")
        .join(UI_FONT_FILE)
}

/// Startup: resolve the UI font once; runs before any UI-building system.
pub fn setup_ui_font(
    mut commands: Commands,
    mut fonts: Option<ResMut<Assets<Font>>>,
) {
    let source = if let Some(fonts) = fonts.as_mut() {
        match std::fs::read(font_file()) {
            Ok(bytes) => FontSource::Handle(fonts.add(Font::from_bytes(bytes))),
            Err(e) => {
                info!("[ui6] shared ui font unreadable ({e}) — default fallback");
                FontSource::default()
            }
        }
    } else {
        FontSource::default()
    };
    commands.insert_resource(UiFont(source));
    info!(
        "[ui6] ui font ready ({})",
        if fonts.is_some() { "shared asset" } else { "default fallback" }
    );
}

/// 顶匾的一格：小标签在上、大数字在下。
fn plaque_cell(
    parent: &mut ChildSpawner<'_>,
    font: &UiFont,
    ui: &warbase::ui::UiTheme,
    label: &str,
    label_marker: Option<impl Component>,
    value_marker: impl Component,
    value_color: Color,
) {
    parent
        .spawn(Node {
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            row_gap: Val::Px(2.0),
            padding: UiRect::px(12.0, 16.0, 6.0, 6.0),
            ..default()
        })
        .with_children(|cell| {
            // 标签实体：带语言刷新标记（None = 静态标签）。
            if let Some(m) = label_marker {
                cell.spawn((
                    m,
                    Text::new(label.to_string()),
                    TextFont {
                        font: font.0.clone(),
                        font_size: FontSize::Px(ui.typography.font_caption),
                        ..default()
                    },
                    TextColor(ui.palette.text_secondary.color()),
                ));
            } else {
                cell.spawn((
                    Text::new(label.to_string()),
                    TextFont {
                        font: font.0.clone(),
                        font_size: FontSize::Px(ui.typography.font_caption),
                        ..default()
                    },
                    TextColor(ui.palette.text_secondary.color()),
                ));
            }
            cell.spawn((
                value_marker,
                Text::new(String::new()),
                TextFont {
                    font: font.0.clone(),
                    font_size: FontSize::Px(ui.typography.font_value),
                    ..default()
                },
                TextColor(value_color),
            ));
        });
}

/// 全宽透明锚条：内容水平居中的载体（position absolute 的居中容器）。
fn center_strip(extra: Node) -> Node {
    Node {
        position_type: PositionType::Absolute,
        left: Val::Px(0.0),
        right: Val::Px(0.0),
        justify_content: JustifyContent::Center,
        ..extra
    }
}

/// Spawn the top plaque (three cells) and the field-report parchment.
/// WARDEN-SKIN: colors/sizes/radii read `UiTheme` inside the deferred spawn
/// (the base's UI-DSGN contract: the system signature stays Commands-only).
pub fn spawn_hud(mut commands: Commands, font: Res<UiFont>, lang: Res<Lang>) {
    let font = font.0.clone();
    let lang = *lang;
    commands.queue(move |world: &mut World| {
        let ui = world.resource::<warbase::ui::UiTheme>().clone();
        // --- 顶匾：一块面板底座，三格并排，悬于屏幕顶部居中 ---
        world
            .spawn(center_strip(Node {
                top: Val::Px(8.0),
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Stretch,
                ..default()
            }))
            .with_children(|strip| {
                strip
                    .spawn((
                        BackgroundColor(ui.palette.panel_bg.color()),
                        BorderColor::all(ui.palette.panel_border.color()),
                        Node {
                            border: UiRect::all(Val::Px(ui.spacing.panel_border_w)),
                            border_radius: BorderRadius::all(Val::Px(ui.radius.panel)),
                            ..default()
                        },
                    ))
                    .with_children(|plaque| {
                        // 顶缘金饰线（铁金条约：面板顶缘的金 trim）。
                        plaque.spawn((
                            Node {
                                position_type: PositionType::Absolute,
                                top: Val::Px(0.0),
                                left: Val::Px(0.0),
                                right: Val::Px(0.0),
                                height: Val::Px(2.0),
                                ..default()
                            },
                            BackgroundColor(ui.palette.accent.color()),
                        ));
                        let f = UiFont(font.clone());
                        plaque_cell(
                            plaque,
                            &f,
                            &ui,
                            l10n::GOLD_EQ.pick(lang),
                            Some(GoldLabel),
                            GoldValue,
                            ui.state_colors.gold.color(),
                        );
                        plaque_cell(
                            plaque,
                            &f,
                            &ui,
                            l10n::BASE_EQ.pick(lang),
                            Some(ShieldLabel),
                            ShieldValue,
                            ui.palette.text_primary.color(),
                        );
                        plaque_cell(
                            plaque,
                            &f,
                            &ui,
                            "",
                            None::<WaveValue>,
                            WaveValue,
                            ui.palette.text_primary.color(),
                        );
                    });
            });

        // --- 军报：顶匾之下居中拉开一掌，备战期揭示，开波即收起 ---
        world
            .spawn(center_strip(Node {
                top: Val::Px(88.0),
                ..default()
            }))
            .with_children(|strip| {
                strip
                    .spawn((
                        FieldReport,
                        Visibility::Hidden,
                        BackgroundColor(ui.palette.panel_raised.color()),
                        BorderColor::all(ui.palette.panel_border.color()),
                        Node {
                            padding: UiRect::px(28.0, 36.0, 12.0, 12.0),
                            flex_direction: FlexDirection::Column,
                            align_items: AlignItems::Center,
                            row_gap: Val::Px(6.0),
                            border: UiRect::all(Val::Px(ui.spacing.panel_border_w)),
                            border_radius: BorderRadius::all(Val::Px(ui.radius.panel)),
                            ..default()
                        },
                    ))
                    .with_children(|report| {
                        // 眉题（哨卡的印章语言）：金色小字。
                        report.spawn((
                            ReportHeader,
                            Text::new(l10n::REPORT_HEADER.pick(lang).to_string()),
                            TextFont {
                                font: font.clone(),
                                font_size: FontSize::Px(ui.typography.font_caption),
                                ..default()
                            },
                            TextColor(ui.palette.accent.color()),
                        ));
                        report.spawn((
                            HudText,
                            Text::new(String::new()),
                            TextFont {
                                font: font.clone(),
                                font_size: FontSize::Px(ui.typography.font_title),
                                ..default()
                            },
                            TextColor(ui.palette.text_primary.color()),
                            TextLayout::justify(Justify::Center),
                        ));
                    });
            });
    });
}

/// 顶匾标签随语言切换重写（独立小系统，免 update_hud 破参数墙）。
pub fn update_plaque_labels(
    mut gold_label: Query<&mut Text, (With<GoldLabel>, Without<ShieldLabel>, Without<ReportHeader>)>,
    mut shield_label: Query<&mut Text, (With<ShieldLabel>, Without<GoldLabel>, Without<ReportHeader>)>,
    mut report_header: Query<
        &mut Text,
        (
            With<ReportHeader>,
            Without<GoldLabel>,
            Without<ShieldLabel>,
            Without<HudText>,
        ),
    >,
    lang: Res<Lang>,
) {
    let lang = *lang;
    for mut t in &mut gold_label {
        if t.0 != l10n::GOLD_EQ.pick(lang) {
            t.0 = l10n::GOLD_EQ.pick(lang).to_string();
        }
    }
    for mut t in &mut shield_label {
        if t.0 != l10n::BASE_EQ.pick(lang) {
            t.0 = l10n::BASE_EQ.pick(lang).to_string();
        }
    }
    for mut t in &mut report_header {
        if t.0 != l10n::REPORT_HEADER.pick(lang) {
            t.0 = l10n::REPORT_HEADER.pick(lang).to_string();
        }
    }
}

pub fn update_hud(
    mut gold: Query<&mut Text, With<GoldValue>>,
    mut shield: Query<(&mut Text, &mut TextColor), (With<ShieldValue>, Without<GoldValue>)>,
    mut wave: Query<
        &mut Text,
        (
            With<WaveValue>,
            Without<GoldValue>,
            Without<ShieldValue>,
        ),
    >,
    mut report_text: Query<
        &mut Text,
        (
            With<HudText>,
            Without<GoldValue>,
            Without<ShieldValue>,
            Without<WaveValue>,
            Without<ReportHeader>,
        ),
    >,
    mut report_vis: Query<&mut Visibility, With<FieldReport>>,
    economy: Res<Economy>,
    lives: Res<warbase::prelude::LivesLeft>,
    rules: Res<warbase::prelude::MatchRules>,
    wave_state: Res<WaveState>,
    schedule: Res<WaveSchedule>,
    enemy_defs: Res<EnemyDefs>,
    choice: Res<WaveChoice>,
    state: Res<State<GameState>>,
    aiming: Res<crate::resources::SkillAiming>,
    lang: Res<Lang>,
    // WARDEN-SKIN: read-only style read (low-base warning color). The only
    // signature concession in this increment — the per-frame recolor needs
    // live tokens and the system has no Commands to defer through.
    theme: Res<warbase::ui::UiTheme>,
) {
    let lang = *lang;
    // 顶匾数值：变化时才写（CJK 重排一帧一行的教训）。
    let set = |t: &mut Text, want: String| {
        if t.0 != want {
            t.0 = want;
        }
    };
    let max_lives = match &rules.leak {
        warbase::prelude::LeakPolicy::Lives { lives } => *lives,
        _ => 0,
    };
    let phase_label = match wave_state.phase {
        WavePhase::Intermission => l10n::PHASE_BUILD.pick(lang),
        WavePhase::Combat => l10n::PHASE_COMBAT.pick(lang),
    };
    for mut t in &mut gold {
        set(&mut t, economy.gold.to_string());
    }
    for (mut t, mut c) in &mut shield {
        set(&mut t, format!("{}/{}", lives.0, max_lives));
        // 低血量转警色（低于开局三成即亮红示警）。
        let want = if max_lives > 0 && lives.0 * 3 <= max_lives {
            theme.state_colors.health_low.color()
        } else {
            theme.palette.text_primary.color()
        };
        if c.0 != want {
            c.0 = want;
        }
    }
    for mut t in &mut wave {
        set(
            &mut t,
            l10n::hud_wave(
                lang,
                (wave_state.current + 1) as usize,
                schedule.waves.len(),
                phase_label,
            ),
        );
    }

    // 军报：终局结算 > 瞄准提示 > 三选一 > 备战军情（UI8）。
    let targeting = aiming.0.is_some();
    let mut text = String::new();
    let mut show = false;
    if matches!(state.get(), GameState::GameOver | GameState::Win) {
        text.push_str(&l10n::hud_settle(lang, UPGRADE1_COST, UPGRADE2_COST));
        show = true;
    } else if *state.get() == GameState::Playing {
        if targeting {
            text.push_str(l10n::SKILL_TARGET_HINT.pick(lang));
            show = true;
        } else if choice.pending {
            text.push_str(l10n::CHOICE.pick(lang));
            show = true;
        } else if wave_state.phase == WavePhase::Intermission {
            if let Some(next) = schedule.waves.get(wave_state.current as usize) {
                let composition = next
                    .entries
                    .iter()
                    .filter_map(|e| {
                        enemy_defs
                            .list
                            .get(e.enemy_index)
                            .map(|d| format!("{}×{}", l10n::enemy_label(d, lang), e.count))
                    })
                    .collect::<Vec<_>>()
                    .join(" ");
                text.push_str(&l10n::next_wave_preview(lang, &composition, next.reward));
                show = true;
            }
        }
    }
    for mut t in &mut report_text {
        set(&mut t, text.clone());
    }
    for mut v in &mut report_vis {
        let want = if show { Visibility::Visible } else { Visibility::Hidden };
        if *v != want {
            *v = want;
        }
    }
}
