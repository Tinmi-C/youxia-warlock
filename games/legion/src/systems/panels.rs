//! LG3 panels: the Plan-A ("Iron & Gold", 铁金条约) HUD for legion — the six
//! proposal §3 LG3 surfaces, all rendered from the `UiTheme` tokens with the
//! shared CJK font, pure bevy native (no textures), pattern-copied from
//! loop-td's `systems/panels.rs` (the mature Plan-A implementation; the copy
//! cost is the LG3 KPI measurement — see docs/kpi.md):
//!
//! ① HUD 资源条  — 本家金 / 人口 / 回合数 / 建造期倒计时（帧→秒换算）
//! ② 命数板      — 四家命数，队色区分（P0P2 队0 蓝 / P1P3 队1 红），出局置灰
//! ③ 雇佣兵商店卡 — 建造期命令卡：6 兵种 价格/人口/攻甲型，Custom action
//!                  点击购买（点击与数字键共用一条 HireCommand 派发缝）
//! ④ 布阵格高亮  — 悬停商店卡时，本家空闲布阵格点亮（世界空间金色发光片）
//! ⑤ 发兵入口    — 本路对家目标 + 递增价预览（本发/下一发/再下发）+ 发送钮
//! ⑥ 回合横幅    — 建造期/战斗期/结算三态中文（+ 终局横幅：胜负/平局）
//!
//! Architecture (mirrors loop-td / warbase's command panel): one root node
//! per panel (`PanelRoot`), children rebuilt ONLY when a per-panel cache key
//! changes (data + theme revision — the CJK-reorder pitfall needs
//! write-on-change). Everything is headless-safe: Node/Text exist without a
//! renderer, the font falls back to default without an asset store, the 3D
//! glow quads guard on the mesh asset stores.
//!
//! Layout: absolute px on the 1280x720 canvas, edge anchors only.

use std::collections::HashMap;

use bevy::prelude::*;
use bevy::text::FontSource;
use warbase::prelude::*;
use warbase::ui::{CommandAction, CommandSlotButton, UiTheme};

use crate::components::{DeploySlot, Merc};
use crate::resources::{
    LivesBoard, Lanes, PendingSends, Phase, PopLedger, Purse, RoundFlow, SendLedger, BUILD_FRAMES,
};
use crate::systems::font::UiFont;
use crate::systems::gameplay::MERCENARY_DEFS;
use crate::systems::round::{
    pop_cost, send_cost, HireCommand, RoundSettled, SendCommand, LOCAL_PLAYER,
};

// ---------------------------------------------------------------------------
// Layout rects (Plan A console vocabulary)
// ---------------------------------------------------------------------------

/// Absolute px rect on the 1280x720 design canvas.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

/// Design canvas: the panels lay out in absolute px on the 1280x720 window
/// (asserted in tests; not read by the runtime layout itself).
#[cfg_attr(not(test), allow(dead_code))]
const CANVAS: (f32, f32) = (1280.0, 720.0);

/// TL: HUD resource plaque (本家金/人口/回合/倒计时).
pub const RES_RECT: Rect = Rect { x: 8.0, y: 8.0, w: 212.0, h: 132.0 };
/// TR: four-player lives board.
pub const LIVES_RECT: Rect = Rect { x: 1060.0, y: 8.0, w: 212.0, h: 122.0 };
/// TC: round banner (build / battle / settle / ended).
pub const BANNER_RECT: Rect = Rect { x: 490.0, y: 8.0, w: 300.0, h: 34.0 };
/// Bottom-center: mercenary hire shop card (build-phase command card).
pub const SHOP_RECT: Rect = Rect { x: 296.0, y: 540.0, w: 276.0, h: 172.0 };
/// Bottom-right: send entry (target + escalating price preview).
pub const SEND_RECT: Rect = Rect { x: 588.0, y: 540.0, w: 284.0, h: 172.0 };

// ---------------------------------------------------------------------------
// Panel registry
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PanelKind {
    Resource,
    Lives,
    Banner,
    Shop,
    Send,
}

#[derive(Component, Debug)]
pub struct PanelRoot(pub PanelKind);

/// Screenshot seam: `PanelsSolo(Some(k))` keeps only panel `k` visible (the
/// LG3 offscreen close-up shots); `None` (the normal game) shows everything.
#[derive(Resource, Debug, Default)]
pub struct PanelsSolo(pub Option<PanelKind>);

/// Last-built cache key per panel (children rebuild only on change).
#[derive(Resource, Debug, Default)]
struct PanelKeys(HashMap<PanelKind, String>);

/// ④: marker on the deploy slots lit by a hovered shop card (world-space).
#[derive(Component, Debug)]
pub struct DeployGlow {
    pub def: String,
}

/// Marker on the 3D glow quad child (visual pass, render stores only).
#[derive(Component, Debug)]
pub struct GlowQuad;

/// ⑥: the settle flash — settlement is instant in the round layer, so the
/// banner holds its 结算 state for this many frames after RoundSettled.
pub const SETTLE_FLASH_FRAMES: u32 = 90;

#[derive(Resource, Debug, Clone)]
pub struct SettleFlash {
    pub round: u32,
    pub income: u32,
    pub frames_left: u32,
}

pub struct PanelsPlugin;

impl Plugin for PanelsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PanelKeys>()
            .init_resource::<PanelsSolo>()
            .init_resource::<ShopCursor>()
            .add_systems(
                Startup,
                (crate::systems::font::setup_ui_font, spawn_panel_roots).chain(),
            )
            .add_systems(
                Update,
                (
                    track_settle_flash,
                    refresh_panels,
                    refresh_deploy_glow,
                    shop_dispatch,
                )
                    .chain(),
            );
    }
}

fn spawn_panel_roots(world: &mut World) {
    let theme = world.resource::<UiTheme>().clone();
    for (kind, rect) in [
        (PanelKind::Resource, RES_RECT),
        (PanelKind::Lives, LIVES_RECT),
        (PanelKind::Banner, BANNER_RECT),
        (PanelKind::Shop, SHOP_RECT),
        (PanelKind::Send, SEND_RECT),
    ] {
        let (node, bg, border) = panel_node(&theme, rect);
        world.spawn((PanelRoot(kind), node, bg, border));
    }
}

// ---------------------------------------------------------------------------
// Shared builders (theme-token styled; the loop-td mock's proven vocabulary)
// ---------------------------------------------------------------------------

fn panel_node(theme: &UiTheme, r: Rect) -> (Node, BackgroundColor, BorderColor) {
    let s = &theme.spacing;
    (
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(r.x),
            top: Val::Px(r.y),
            width: Val::Px(r.w),
            height: Val::Px(r.h),
            padding: UiRect::all(Val::Px((s.panel_gap + s.panel_border_w).max(8.0))),
            border: UiRect::all(Val::Px(s.panel_border_w)),
            display: Display::Flex,
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(s.panel_gap),
            border_radius: BorderRadius::all(Val::Px(theme.radius.panel)),
            ..default()
        },
        BackgroundColor(theme.palette.panel_bg.color()),
        BorderColor::all(theme.palette.panel_border.color()),
    )
}

/// Text carrying the shared CJK font (every panel text goes through here).
fn font_text(font: &UiFont, s: &str, size: f32, color: Color) -> (Text, TextFont, TextColor) {
    (
        Text::new(s),
        TextFont { font: font.0.clone(), font_size: FontSize::Px(size), ..default() },
        TextColor(color),
    )
}

/// One geometric icon glyph (circle / square / pill).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Glyph {
    Circle,
    Square,
    Pill,
}

fn icon_node(color: Color, size: f32, glyph: Glyph) -> (Node, BackgroundColor) {
    let r = match glyph {
        Glyph::Circle => Val::Percent(50.0),
        Glyph::Square => Val::Px(0.0),
        Glyph::Pill => Val::Percent(30.0),
    };
    (
        Node {
            width: Val::Px(size),
            height: Val::Px(size),
            border_radius: BorderRadius::all(r),
            ..default()
        },
        BackgroundColor(color),
    )
}

fn row_node(gap: f32) -> Node {
    Node {
        display: Display::Flex,
        flex_direction: FlexDirection::Row,
        align_items: AlignItems::Center,
        column_gap: Val::Px(gap),
        ..default()
    }
}

// --- display vocabularies (CJK labels) --------------------------------------

/// Mercenary display names (maps/arena.ron ids -> WC3-style Chinese).
pub fn merc_name(def: &str) -> &str {
    match def {
        "merc_cheap" => "步兵",
        "merc_swarm" => "疾行兽",
        "merc_tank" => "重装盾卫",
        "merc_ranged" => "穿刺射手",
        "merc_aoe" => "攻城炮手",
        "merc_support" => "医官",
        _ => "雇佣兵",
    }
}

fn attack_label(t: AttackType) -> &'static str {
    match t {
        AttackType::Normal => "普通",
        AttackType::Piercing => "穿刺",
        AttackType::Siege => "攻城",
        AttackType::Magic => "魔法",
        AttackType::Hero => "英雄",
    }
}

fn armor_label(t: ArmorType) -> &'static str {
    match t {
        ArmorType::Light => "轻甲",
        ArmorType::Medium => "中甲",
        ArmorType::Heavy => "重甲",
        ArmorType::Fortified => "城甲",
        ArmorType::Hero => "英雄甲",
        ArmorType::Divine => "神甲",
    }
}

/// 建造期倒计时：帧钟 → 剩余整秒（ceil — 第 1799 帧仍显示 1s）。验收锚点：
/// frame 0 → 30，frame 1799 → 1（1800 帧即转战斗，不再显示 0）。
pub fn build_seconds(frame: u32) -> u32 {
    (BUILD_FRAMES.saturating_sub(frame) as f32 / 60.0).ceil() as u32
}

/// Team color for a player slot (P0P2 team0 blue / P1P3 team1 red — the
/// lane-stripe grammar, as theme tokens so a re-skin retints the board).
fn team_color(theme: &UiTheme, team: TeamId) -> Color {
    if team == TeamId(0) {
        theme.state_colors.mana.color()
    } else {
        theme.state_colors.enemy.color()
    }
}

// ---------------------------------------------------------------------------
// Refresh driver: per-panel cache keys + rebuilds
// ---------------------------------------------------------------------------

fn refresh_panels(world: &mut World) {
    let mut keys = world.remove_resource::<PanelKeys>().unwrap_or_default();
    let theme = world.resource::<UiTheme>().clone();
    let font = UiFont(
        world
            .get_resource::<UiFont>()
            .map(|f| f.0.clone())
            .unwrap_or(FontSource::default()),
    );

    refresh_resource(world, &mut keys, &theme, &font);
    refresh_lives(world, &mut keys, &theme, &font);
    refresh_banner(world, &mut keys, &theme, &font);
    refresh_shop(world, &mut keys, &theme, &font);
    refresh_send(world, &mut keys, &theme, &font);

    // Screenshot seam: a solo override wins over normal visibility.
    if let Some(solo) = world.resource::<PanelsSolo>().0 {
        let mut q = world.query_filtered::<(&PanelRoot, &mut Visibility), ()>();
        for (root, mut vis) in q.iter_mut(world) {
            *vis = if solo == root.0 { Visibility::Visible } else { Visibility::Hidden };
        }
    }
    world.insert_resource(keys);
}

fn panel_root(world: &mut World, kind: PanelKind) -> Entity {
    let mut q = world.query_filtered::<Entity, With<PanelRoot>>();
    q.iter(world)
        .find(|&e| world.get::<PanelRoot>(e).map(|r| r.0 == kind).unwrap_or(false))
        .expect("panel roots spawned at startup")
}

/// Rebuild a panel's children when `key` changed; always fixes visibility.
#[allow(clippy::too_many_arguments)]
fn rebuild(
    world: &mut World,
    keys: &mut PanelKeys,
    kind: PanelKind,
    key: String,
    visible: bool,
    theme: &UiTheme,
    font: &UiFont,
    build: impl FnOnce(&mut ChildSpawner, &UiTheme, &UiFont),
) {
    if keys.0.get(&kind) != Some(&key) {
        keys.0.insert(kind, key);
        let root = panel_root(world, kind);
        // Drop the previous subtree (Texts are grandchildren).
        let children: Vec<Entity> =
            world.get::<Children>(root).map(|c| c.to_vec()).unwrap_or_default();
        for c in children {
            despawn_subtree(world, c);
        }
        // Re-skin the root itself (theme swap reaches every surface).
        {
            let (.., bg, border) = panel_node(theme, rect_of(kind));
            let mut e = world.get_entity_mut(root).unwrap();
            if let Some(mut b) = e.get_mut::<BackgroundColor>() {
                *b = bg;
            }
            if let Some(mut b) = e.get_mut::<BorderColor>() {
                *b = border;
            }
        }
        world.get_entity_mut(root).unwrap().with_children(|p| build(p, theme, font));
    }
    set_visible(world, kind, visible);
}

fn set_visible(world: &mut World, kind: PanelKind, visible: bool) {
    let root = panel_root(world, kind);
    if let Some(mut v) = world.get_mut::<Visibility>(root) {
        let want = if visible { Visibility::Visible } else { Visibility::Hidden };
        if *v != want {
            *v = want;
        }
    }
}

fn rect_of(kind: PanelKind) -> Rect {
    match kind {
        PanelKind::Resource => RES_RECT,
        PanelKind::Lives => LIVES_RECT,
        PanelKind::Banner => BANNER_RECT,
        PanelKind::Shop => SHOP_RECT,
        PanelKind::Send => SEND_RECT,
    }
}

fn despawn_subtree(world: &mut World, root: Entity) {
    let children: Vec<Entity> = world.get::<Children>(root).map(|c| c.to_vec()).unwrap_or_default();
    for c in children {
        despawn_subtree(world, c);
    }
    let _ = world.entity_mut(root).despawn();
}

// --- ① TL: HUD resource plaque ------------------------------------------------

fn refresh_resource(world: &mut World, keys: &mut PanelKeys, theme: &UiTheme, font: &UiFont) {
    let gold = world.resource::<Purse>().0[LOCAL_PLAYER as usize];
    let (used, cap) = {
        let p = world.resource::<PopLedger>();
        (p.used[LOCAL_PLAYER as usize], p.cap[LOCAL_PLAYER as usize])
    };
    let flow = world.resource::<RoundFlow>().clone();
    let secs = build_seconds(flow.frame);
    let key = format!(
        "res:{gold}:{used}/{cap}:{}:{}:{}:{rev}",
        flow.round,
        flow.phase == Phase::Build,
        secs,
        rev = theme.revision
    );
    rebuild(world, keys, PanelKind::Resource, key, true, theme, font, |p, theme, font| {
        let t = &theme.typography;
        let sc = &theme.state_colors;
        let gap = theme.spacing.panel_gap + 2.0;
        let rows: Vec<(Glyph, Color, String, &str)> = vec![
            (Glyph::Circle, sc.gold.color(), gold.to_string(), "金"),
            (Glyph::Pill, theme.palette.accent.color(), format!("{used}/{cap}"), "人口"),
            (Glyph::Square, theme.palette.text_secondary.color(), flow.round.to_string(), "回合"),
        ];
        for (glyph, color, value, label) in rows {
            p.spawn(row_node(gap)).with_children(|row| {
                row.spawn(icon_node(color, theme.spacing.icon_size, glyph));
                let (vt, vf, vc) =
                    font_text(font, &value, t.font_value, theme.palette.text_primary.color());
                row.spawn((vt, vf, vc));
                let (lt, lf, lc) =
                    font_text(font, label, t.font_caption, theme.palette.text_secondary.color());
                row.spawn((lt, lf, lc));
            });
        }
        // Build countdown (the row only exists in build phase; battle reads
        // off the banner instead).
        if flow.phase == Phase::Build {
            p.spawn(row_node(gap)).with_children(|row| {
                row.spawn(icon_node(
                    theme.state_colors.warning.color(),
                    theme.spacing.icon_size,
                    Glyph::Circle,
                ));
                let (vt, vf, vc) =
                    font_text(font, &format!("{secs}s"), t.font_value, sc.warning.color());
                row.spawn((vt, vf, vc));
                let (lt, lf, lc) = font_text(
                    font,
                    "建造期倒计时",
                    t.font_caption,
                    theme.palette.text_secondary.color(),
                );
                row.spawn((lt, lf, lc));
            });
        }
    });
}

// --- ② TR: four-player lives board ----------------------------------------------

fn refresh_lives(world: &mut World, keys: &mut PanelKeys, theme: &UiTheme, font: &UiFont) {
    let lives = world.resource::<LivesBoard>().0;
    let teams: Vec<TeamId> = {
        let dip = world.resource::<MatchDiplomacy>();
        (0..4)
            .map(|p| {
                dip.players
                    .iter()
                    .find(|(pid, _)| pid.0 == p)
                    .map(|(_, t)| *t)
                    .unwrap_or(TeamId(p as u8))
            })
            .collect()
    };
    let key = format!(
        "lives:{}:{rev}",
        lives.map(|l| l.to_string()).join("/"),
        rev = theme.revision
    );
    rebuild(world, keys, PanelKind::Lives, key, true, theme, font, |p, theme, font| {
        let t = &theme.typography;
        let (ht, hf, hc) = font_text(font, "命数板", t.font_title, theme.palette.accent.color());
        p.spawn((ht, hf, hc));
        for player in 0..4u8 {
            let color = team_color(theme, teams[player as usize]);
            let out = lives[player as usize] == 0;
            p.spawn(row_node(theme.spacing.panel_gap + 2.0)).with_children(|row| {
                row.spawn(icon_node(
                    if out { theme.palette.text_dim.color() } else { color },
                    theme.spacing.icon_size,
                    Glyph::Square,
                ));
                let (nt, nf, nc) = font_text(
                    font,
                    &format!("P{player}"),
                    t.font_label,
                    if out { theme.palette.text_dim.color() } else { theme.palette.text_primary.color() },
                );
                row.spawn((nt, nf, nc));
                let value = if out { "出局".to_string() } else { lives[player as usize].to_string() };
                let (vt, vf, vc) = font_text(
                    font,
                    &value,
                    t.font_value,
                    if out { theme.palette.text_dim.color() } else { color },
                );
                row.spawn((vt, vf, vc));
            });
        }
    });
}

// --- ⑥ TC: round banner -----------------------------------------------------------

/// Banner display state (build / battle / settle / ended).
#[derive(Clone, Debug, PartialEq)]
pub enum BannerState {
    Build { round: u32, secs: u32 },
    Battle { round: u32 },
    Settle { round: u32, income: u32 },
    Ended { draw: bool, reason: String },
}

fn banner_state(world: &World) -> BannerState {
    if let Some(flash) = world.get_resource::<SettleFlash>() {
        return BannerState::Settle { round: flash.round, income: flash.income };
    }
    match world.resource::<MatchFlow>() {
        MatchFlow::Won(reason) => {
            return BannerState::Ended { draw: reason.contains("draw"), reason: reason.clone() }
        }
        MatchFlow::Lost(reason) => {
            return BannerState::Ended { draw: false, reason: reason.clone() }
        }
        MatchFlow::Running => {}
    }
    let flow = world.resource::<RoundFlow>();
    match flow.phase {
        Phase::Build => BannerState::Build { round: flow.round, secs: build_seconds(flow.frame) },
        Phase::Battle => BannerState::Battle { round: flow.round },
    }
}

fn banner_text(state: &BannerState) -> String {
    match state {
        BannerState::Build { round, secs } => {
            format!("第 {round} 回合 · 建造期 · {secs}s")
        }
        BannerState::Battle { round } => format!("第 {round} 回合 · 战斗期"),
        BannerState::Settle { round, income } => {
            format!("第 {round} 回合结算 · +{income} 金")
        }
        BannerState::Ended { draw, reason } => {
            if *draw {
                format!("平局 —— {reason}")
            } else {
                format!("终局 —— {reason}")
            }
        }
    }
}

fn refresh_banner(world: &mut World, keys: &mut PanelKeys, theme: &UiTheme, font: &UiFont) {
    let state = banner_state(world);
    let key = format!("banner:{}:{rev}", banner_text(&state), rev = theme.revision);
    rebuild(world, keys, PanelKind::Banner, key, true, theme, font, |p, theme, font| {
        let (label, color) = match &state {
            BannerState::Build { .. } => (banner_text(&state), theme.palette.text_primary.color()),
            BannerState::Battle { .. } => (banner_text(&state), theme.state_colors.enemy.color()),
            BannerState::Settle { .. } => (banner_text(&state), theme.state_colors.gold.color()),
            BannerState::Ended { draw, .. } => (
                banner_text(&state),
                if *draw { theme.state_colors.warning.color() } else { theme.state_colors.gold.color() },
            ),
        };
        let (bt, bf, bc) = font_text(font, &label, theme.typography.font_title, color);
        p.spawn((bt, bf, bc));
    });
}

/// Hold the settle flash: light on RoundSettled, decay one frame per update,
/// gone at zero (the banner falls back to the phase states).
fn track_settle_flash(
    mut settled: MessageReader<RoundSettled>,
    mut commands: Commands,
    mut flash: Option<ResMut<SettleFlash>>,
) {
    if let Some(s) = settled.read().last() {
        commands.insert_resource(SettleFlash {
            round: s.round,
            income: s.income,
            frames_left: SETTLE_FLASH_FRAMES,
        });
        return; // fresh flash: no decay the frame it lights
    }
    if let Some(f) = flash.as_deref_mut() {
        if f.frames_left <= 1 {
            commands.remove_resource::<SettleFlash>();
        } else {
            f.frames_left -= 1;
        }
    }
}

// --- ③ BC: mercenary hire shop card -------------------------------------------------

/// One shop entry's display row data (registry lookups off the rebuild path).
struct ShopEntry {
    def: &'static str,
    name: &'static str,
    gold: u32,
    pop: u8,
    type_line: String,
    afford: bool,
    badge: String,
}

fn shop_entries(world: &World, build: bool) -> Vec<ShopEntry> {
    let gold = world.resource::<Purse>().0[LOCAL_PLAYER as usize];
    let registry = world.resource::<UnitRegistry>();
    MERCENARY_DEFS
        .iter()
        .enumerate()
        .filter_map(|(i, def)| {
            let d = registry.get(*def)?;
            Some(ShopEntry {
                def,
                name: merc_name(def),
                gold: d.cost_gold,
                pop: pop_cost(def)?,
                type_line: format!(
                    "{}攻 · {}",
                    attack_label(d.attack.as_ref()?.attack_type),
                    armor_label(d.armor_type)
                ),
                afford: build && gold >= d.cost_gold,
                badge: (i + 1).to_string(),
            })
        })
        .collect()
}

fn refresh_shop(world: &mut World, keys: &mut PanelKeys, theme: &UiTheme, font: &UiFont) {
    let build = world.resource::<RoundFlow>().phase == Phase::Build;
    let entries = shop_entries(world, build);
    let afford_bits: String = entries.iter().map(|e| if e.afford { '1' } else { '0' }).collect();
    let key = format!("shop:{build}:{afford_bits}:{}:{rev}", entries.len(), rev = theme.revision);
    rebuild(world, keys, PanelKind::Shop, key, true, theme, font, |p, theme, font| {
        let t = &theme.typography;
        let s = &theme.spacing;
        let (ht, hf, hc) = font_text(font, "雇佣兵商店", t.font_title, theme.palette.accent.color());
        p.spawn((ht, hf, hc));
        // 3 x 2 grid of command slots (wrap row).
        p.spawn((
            Node {
                display: Display::Flex,
                flex_direction: FlexDirection::Row,
                flex_wrap: FlexWrap::Wrap,
                column_gap: Val::Px(s.panel_gap),
                row_gap: Val::Px(s.panel_gap),
                align_content: AlignContent::FlexStart,
                ..default()
            },
        ))
        .with_children(|grid| {
            for e in &entries {
                let (bg, fg) = if e.afford {
                    (theme.palette.panel_raised.color(), theme.palette.text_primary.color())
                } else {
                    (theme.state_colors.slot_disabled_bg.color(), theme.palette.text_dim.color())
                };
                grid.spawn((
                    Button,
                    Interaction::None,
                    CommandSlotButton {
                        action: CommandAction::Custom(format!("hire:{}", e.def)),
                        enabled: e.afford,
                        tooltip: format!(
                            "雇佣{}：{}金 · 人口{} · {}（点选自家布阵格后按 {} 或点击）",
                            e.name, e.gold, e.pop, e.type_line, e.badge
                        ),
                    },
                    Node {
                        width: Val::Px(s.slot_w),
                        height: Val::Px(s.slot_h + 14.0),
                        padding: UiRect::all(Val::Px(s.slot_pad)),
                        border: UiRect::all(Val::Px(s.slot_border_w.max(1.0))),
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        border_radius: BorderRadius::all(Val::Px(theme.radius.slot)),
                        ..default()
                    },
                    BackgroundColor(bg),
                    BorderColor::all(theme.palette.panel_border.color()),
                ))
                .with_children(|slot| {
                    slot.spawn((
                        Node {
                            display: Display::Flex,
                            flex_direction: FlexDirection::Column,
                            align_items: AlignItems::Center,
                            row_gap: Val::Px(1.0),
                            ..default()
                        },
                    ))
                    .with_children(|txt| {
                        let (nt, nf, nc) = font_text(font, e.name, t.font_label, fg);
                        txt.spawn((nt, nf, nc));
                        let price = format!("{}金 · 人口{}", e.gold, e.pop);
                        let (ct, cf, cc) = font_text(
                            font,
                            &price,
                            t.font_caption,
                            if e.afford { theme.state_colors.gold.color() } else { theme.palette.text_dim.color() },
                        );
                        txt.spawn((ct, cf, cc));
                        let (tt, tf, tc) = font_text(
                            font,
                            &e.type_line,
                            t.font_caption,
                            theme.palette.text_secondary.color(),
                        );
                        txt.spawn((tt, tf, tc));
                    });
                    let (bt, bf, bc) =
                        font_text(font, &e.badge, t.font_badge, theme.palette.accent.color());
                    slot.spawn((bt, bf, bc, Node {
                        position_type: PositionType::Absolute,
                        top: Val::Px(s.badge_top),
                        right: Val::Px(s.badge_right),
                        ..default()
                    }));
                });
            }
        });
    });
}

// --- ⑤ BR: send entry -------------------------------------------------------------

fn refresh_send(world: &mut World, keys: &mut PanelKeys, theme: &UiTheme, font: &UiFont) {
    // Data-driven target list: every lane owned by the local player (the
    // shipped arena has exactly one — P0 -> P1).
    let lanes = world.resource::<Lanes>().0.clone();
    let build = world.resource::<RoundFlow>().phase == Phase::Build;
    let gold = world.resource::<Purse>().0[LOCAL_PLAYER as usize];
    let ledger = world.resource::<SendLedger>().clone();
    let queued: usize = world
        .resource::<PendingSends>()
        .0
        .iter()
        .filter(|s| s.sponsor == LOCAL_PLAYER)
        .count();
    let mut rows: Vec<(usize, u8, u32, u32, u32, bool)> = Vec::new();
    for lane in lanes.iter().filter(|l| l.owner == LOCAL_PLAYER as usize) {
        let sent = ledger.0[LOCAL_PLAYER as usize][lane.target];
        rows.push((
            lane.owner,
            lane.target as u8,
            send_cost(sent),
            send_cost(sent + 1),
            send_cost(sent + 2),
            build && gold >= send_cost(sent),
        ));
    }
    // The escalating prices ride the key: a ledger bump can land without
    // gold/build/queued moving (income refunds the send), and the preview
    // must still move to the next price step.
    let price_bits: String = rows
        .iter()
        .map(|(_, t, c, n1, n2, _)| format!("{t}:{c}/{n1}/{n2};"))
        .collect();
    let key = format!("send:{build}:{gold}:{queued}:{price_bits}:{rev}", rev = theme.revision);
    rebuild(world, keys, PanelKind::Send, key, true, theme, font, |p, theme, font| {
        let t = &theme.typography;
        let s = &theme.spacing;
        let (ht, hf, hc) = font_text(font, "发兵入口", t.font_title, theme.palette.accent.color());
        p.spawn((ht, hf, hc));
        if rows.is_empty() {
            let (nt, nf, nc) =
                font_text(font, "本图无本路对家 lane", t.font_caption, theme.palette.text_dim.color());
            p.spawn((nt, nf, nc));
            return;
        }
        for (_, target, cost, next1, next2, afford) in &rows {
            let name = merc_name("merc_swarm");
            let line = format!("目标 P{target}（本路对家） · 已排队 {queued}");
            let (lt, lf, lc) = font_text(font, &line, t.font_caption, theme.palette.text_secondary.color());
            p.spawn((lt, lf, lc));
            // Escalating price preview: this send / next / the one after.
            let preview = format!("本发 {name} {cost}金 · 下一发 {next1}金 · 再下 {next2}金");
            let (pt, pf, pc) = font_text(
                font,
                &preview,
                t.font_caption,
                if *afford { theme.state_colors.gold.color() } else { theme.palette.text_dim.color() },
            );
            p.spawn((pt, pf, pc));
            let (bg, fg) = if *afford {
                (theme.palette.panel_raised.color(), theme.palette.text_primary.color())
            } else {
                (theme.state_colors.slot_disabled_bg.color(), theme.palette.text_dim.color())
            };
            p.spawn((
                Button,
                Interaction::None,
                CommandSlotButton {
                    action: CommandAction::Custom("send:merc_swarm".into()),
                    enabled: *afford,
                    tooltip: format!(
                        "给 P{target} 注入下一回合的 {name}（G 键同款；击穿 +{} 金赏金）",
                        crate::systems::round::BREACH_BOUNTY
                    ),
                },
                Node {
                    width: Val::Px(s.slot_w * 2.0 + s.panel_gap),
                    height: Val::Px(s.slot_h),
                    padding: UiRect::all(Val::Px(s.slot_pad)),
                    border: UiRect::all(Val::Px(s.slot_border_w.max(1.0))),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    border_radius: BorderRadius::all(Val::Px(theme.radius.slot)),
                    ..default()
                },
                BackgroundColor(bg),
                BorderColor::all(if *afford {
                    theme.palette.accent.color()
                } else {
                    theme.palette.panel_border.color()
                }),
            ))
            .with_children(|btn| {
                let label = format!("[G] 发送 {name} · {cost}金");
                let (bt, bf, bc) = font_text(font, &label, t.font_label, fg);
                btn.spawn((bt, bf, bc));
            });
        }
    });
}

// ---------------------------------------------------------------------------
// ③ dispatch: shop card clicks ride the base CommandSlotButton seam — the
// base command_clicks turns a press into CommandCardAction, this system maps
// the game-owned Custom ids onto the SAME HireCommand/SendCommand the digit
// and G input bridges write (one command path, three entries).
// ---------------------------------------------------------------------------

/// Persistent message cursor for the shop dispatcher — a fresh cursor would
/// re-read the previous frame's click actions (the double-send pitfall).
#[derive(Resource, Default)]
struct ShopCursor(bevy::ecs::message::MessageCursor<warbase::ui::CommandCardAction>);

fn shop_dispatch(world: &mut World) {
    let events: Vec<warbase::ui::CommandCardAction> = {
        let mut cursor = world.remove_resource::<ShopCursor>().unwrap_or_default();
        let msgs = world.resource::<bevy::ecs::message::Messages<warbase::ui::CommandCardAction>>();
        let out = cursor.0.read(msgs).cloned().collect();
        world.insert_resource(cursor);
        out
    };
    for event in events {
        let warbase::ui::CommandAction::Custom(id) = event.action else { continue };
        let Some((kind, def)) = id.split_once(':') else { continue };
        match kind {
            "hire" => {
                // Same slot-resolution as hire_input: the first OWN deploy
                // cell in the base selection.
                let selection = world.resource::<SelectionState>().entities.clone();
                let Some(slot) = selection.iter().copied().find(|&e| {
                    world.get::<DeploySlot>(e).is_some_and(|s| s.player == LOCAL_PLAYER)
                }) else {
                    info!("[legion] {def}: select one of your deploy cells first");
                    continue;
                };
                world.write_message(HireCommand { player: LOCAL_PLAYER, def: def.into(), slot });
            }
            "send" => {
                let Some(lane) = world
                    .resource::<Lanes>()
                    .0
                    .iter()
                    .find(|l| l.owner == LOCAL_PLAYER as usize)
                else {
                    continue;
                };
                world.write_message(SendCommand {
                    player: LOCAL_PLAYER,
                    target: lane.target as u8,
                    def: def.into(),
                });
            }
            other => info!("[legion] unknown custom action '{other}:{def}'"),
        }
    }
}

// ---------------------------------------------------------------------------
// ④ deploy-slot highlight: hovering a shop card lights the local player's
// FREE deploy cells (the placement preview). Data layer = DeployGlow markers
// (headless-testable); the visual pass adds a gold quad per marker, guarded
// on the render asset stores.
// ---------------------------------------------------------------------------

fn refresh_deploy_glow(world: &mut World) {
    let build = world.resource::<RoundFlow>().phase == Phase::Build;
    // The hovered def, if a shop hire slot is hovered and enabled.
    let hovered: Option<String> = {
        let mut q = world.query_filtered::<(&Interaction, &CommandSlotButton), With<Button>>();
        q.iter(world)
            .filter(|(i, b)| **i == Interaction::Hovered && b.enabled)
            .filter_map(|(_, b)| match &b.action {
                CommandAction::Custom(id) => id.strip_prefix("hire:").map(|d| d.to_string()),
                _ => None,
            })
            .next()
    };
    let armed = if build { hovered } else { None };

    // Free own slots (occupied cells cannot take the hire).
    let occupied: Vec<Entity> = {
        let mut q = world.query::<&Merc>();
        q.iter(world).map(|m| m.slot).collect()
    };
    let own_free: Vec<Entity> = {
        let mut q = world.query_filtered::<Entity, With<DeploySlot>>();
        q.iter(world)
            .filter(|&e| {
                world.get::<DeploySlot>(e).is_some_and(|s| s.player == LOCAL_PLAYER)
                    && !occupied.contains(&e)
            })
            .collect()
    };

    // Clear every stale marker first (cheap: 48 slots max).
    let stale: Vec<Entity> = {
        let mut q = world.query_filtered::<Entity, With<DeployGlow>>();
        q.iter(world).collect()
    };
    for e in stale {
        if let Ok(mut em) = world.get_entity_mut(e) {
            em.remove::<DeployGlow>();
        }
    }

    if let Some(def) = armed {
        for slot in own_free {
            if let Ok(mut em) = world.get_entity_mut(slot) {
                em.insert(DeployGlow { def: def.clone() });
            }
        }
    }

    // Visual pass: one gold quad per glowing slot (render stores only).
    if !world.contains_resource::<Assets<Mesh>>() || !world.contains_resource::<Assets<StandardMaterial>>()
    {
        return; // headless: markers only (the data-layer contract)
    }
    // Drop quads whose slot lost its glow, spawn quads for new glows.
    let dead_quads: Vec<Entity> = {
        let mut q = world.query::<(Entity, &GlowQuad)>();
        q.iter(world)
            .filter(|(quad, _)| {
                let parent = world
                    .get::<ChildOf>(*quad)
                    .and_then(|p| world.get_entity(p.0).ok());
                parent.is_none_or(|p| world.get::<DeployGlow>(p.id()).is_none())
            })
            .map(|(quad, _)| quad)
            .collect()
    };
    for quad in dead_quads {
        let _ = world.entity_mut(quad).despawn();
    }
    let mesh = world.resource_mut::<Assets<Mesh>>().add(Cuboid::new(0.92, 0.02, 0.92));
    let material = world
        .resource_mut::<Assets<StandardMaterial>>()
        .add(StandardMaterial {
            base_color: Color::srgb(1.0, 0.8, 0.28),
            emissive: LinearRgba::rgb(0.85, 0.62, 0.12),
            ..default()
        });
    let glowing: Vec<Entity> = {
        let mut q = world.query_filtered::<Entity, With<DeployGlow>>();
        q.iter(world).collect()
    };
    for slot in glowing {
        let has_quad = world
            .get::<Children>(slot)
            .is_some_and(|c| c.iter().any(|k| world.get::<GlowQuad>(k).is_some()));
        if !has_quad {
            if let Ok(mut em) = world.get_entity_mut(slot) {
                em.with_child((
                    GlowQuad,
                    Mesh3d(mesh.clone()),
                    MeshMaterial3d(material.clone()),
                    Transform::from_xyz(0.0, 0.05, 0.0),
                ));
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tests: layout fits + the pure display vocabulary (panel/data tests live in
// tests/ui_panels.rs, which runs the full game composition).
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Every panel rect fits the 1280x720 canvas.
    #[test]
    fn all_panel_rects_fit_canvas() {
        for (name, r) in [
            ("res", RES_RECT),
            ("lives", LIVES_RECT),
            ("banner", BANNER_RECT),
            ("shop", SHOP_RECT),
            ("send", SEND_RECT),
        ] {
            assert!(
                r.x >= 0.0 && r.y >= 0.0 && r.x + r.w <= CANVAS.0 && r.y + r.h <= CANVAS.1,
                "{name} off-canvas: {r:?}"
            );
        }
    }

    /// The build countdown converts the frame clock to whole seconds:
    /// fresh phase 30s, last frame 1s (acceptance anchor).
    #[test]
    fn build_seconds_converts_the_frame_clock() {
        assert_eq!(build_seconds(0), 30);
        assert_eq!(build_seconds(60), 29);
        assert_eq!(build_seconds(1799), 1);
    }

    /// The merc vocabulary covers the shipped table.
    #[test]
    fn merc_names_cover_the_table() {
        for def in MERCENARY_DEFS {
            assert_ne!(merc_name(def), "雇佣兵", "{def} must have a CJK name");
        }
    }

    /// Banner text uses the three Chinese states (plus the ended variant).
    #[test]
    fn banner_text_vocabulary() {
        assert_eq!(
            banner_text(&BannerState::Build { round: 3, secs: 12 }),
            "第 3 回合 · 建造期 · 12s"
        );
        assert_eq!(banner_text(&BannerState::Battle { round: 4 }), "第 4 回合 · 战斗期");
        assert_eq!(
            banner_text(&BannerState::Settle { round: 4, income: 50 }),
            "第 4 回合结算 · +50 金"
        );
        assert!(banner_text(&BannerState::Ended {
            draw: true,
            reason: "round cap 20: draw on lives".into()
        })
        .starts_with("平局"));
    }
}
