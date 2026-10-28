//! LT2-UI: the Plan-A ("Iron & Gold", 铁金条约) HUD for loop-td v2 — the six
//! RPG-depth panels (equipment rack / artifact quest plaque / challenge-wave
//! entry / treasure-dig entry / bond chips / hero panel with affix lines)
//! plus the boarded base suite (resource plaque, wave banner, minimap,
//! command console), all rendered from the `UiTheme` tokens with the bundled
//! CJK font, pure bevy native (no textures). Design authority:
//! engine/warbase/docs/ui-design-brief.md 拍板记录 (2026-10-27) +
//! ui-proposals/plan-A/ screenshots.
//!
//! Architecture (mirrors warbase's command panel): one root node per panel
//! (`PanelRoot`), children rebuilt ONLY when a per-panel cache key changes
//! (data + theme revision — the CJK-reorder pitfall needs write-on-change).
//! Panels read REAL game state; the `--ui-mock v2` scene stages that state.
//! Everything is headless-safe: Node/Text exist without a renderer, the 3D
//! artifact-aura rings guard on the mesh asset stores, the font falls back
//! to default without an asset source.
//!
//! Command console: the warbase `CommandPanel` renders a single unwrapped
//! row pinned bottom-left — it collides with the BL hero panel and cannot
//! wrap 8 slots into the boarded 2x4 BC console (its layout is engine-side,
//! frozen this increment). This panel re-faces the SAME `CommandCards` data
//! as clickable `CommandSlotButton`s (warbase's click/hotkey/tooltip systems
//! dispatch them unchanged — one seam), and the engine panel is hidden.
//!
//! Layout: absolute px on the 1280x720 canvas, edge anchors only — R1 (the
//! central 50%x50% battlefield core stays clear) is asserted in tests.

use std::collections::HashMap;

use bevy::prelude::*;
use bevy::text::FontSource;
use warbase::prelude::*;
use warbase::ui::{
    active_card_key, art_revision, art_surface, ArtMode, CommandCards, CommandSlotButton,
    Hotkey, PanelVersion, Selection, SelectionMode, UiTextures, UiTheme, UI_TEX_PANEL,
    UI_TEX_SLOT,
};

use crate::systems::equip::{EquipSlots, EQUIP_SLOTS};
use crate::systems::font::UiFont;

// ---------------------------------------------------------------------------
// Layout rects (Plan A console — the boarded plan-A mock coordinates)
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

/// TL column: resource plaque, artifact plaque, dig entry.
pub const RES_RECT: Rect = Rect { x: 8.0, y: 8.0, w: 196.0, h: 118.0 };
pub const ARTIFACT_RECT: Rect = Rect { x: 8.0, y: 130.0, w: 196.0, h: 78.0 };
pub const DIG_RECT: Rect = Rect { x: 8.0, y: 214.0, w: 196.0, h: 48.0 };
/// TC stack: wave banner + challenge entry.
pub const WAVE_RECT: Rect = Rect { x: 540.0, y: 8.0, w: 200.0, h: 32.0 };
pub const CHALLENGE_RECT: Rect = Rect { x: 528.0, y: 44.0, w: 224.0, h: 46.0 };
/// TR: minimap, right edge: bonds below it.
pub const MINIMAP_RECT: Rect = Rect { x: 1096.0, y: 8.0, w: 176.0, h: 176.0 };
pub const BONDS_RECT: Rect = Rect { x: 1000.0, y: 192.0, w: 272.0, h: 132.0 };
/// Bottom console: hero panel BL, command card BC, equip rack BR.
pub const HERO_RECT: Rect = Rect { x: 8.0, y: 572.0, w: 210.0, h: 140.0 };
pub const COMMAND_RECT: Rect = Rect { x: 226.0, y: 572.0, w: 382.0, h: 140.0 };
pub const EQUIP_RECT: Rect = Rect { x: 616.0, y: 572.0, w: 232.0, h: 140.0 };

// ---------------------------------------------------------------------------
// Panel registry
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PanelKind {
    Resource,
    Wave,
    Minimap,
    Artifact,
    Challenge,
    Dig,
    Bonds,
    Hero,
    Equip,
    Command,
}

#[derive(Component, Debug)]
pub struct PanelRoot(pub PanelKind);

/// Mock/screenshot seams: `PanelsHidden` forces every root hidden (the
/// plan-A/B/C proposal mocks own the screen), `PanelsSolo(Some(k))` keeps
/// only panel `k` visible (v2 close-up shots). Neither resource exists in
/// the normal game.
#[derive(Resource, Debug)]
pub struct PanelsHidden;
#[derive(Resource, Debug, Default)]
pub struct PanelsSolo(pub Option<PanelKind>);

/// Last-built cache key per panel (children rebuild only on change).
#[derive(Resource, Debug, Default)]
struct PanelKeys(HashMap<PanelKind, String>);

/// UITEX inputs for one refresh pass: the engine seam's two resources
/// cloned once per frame (a handful of handle clones), so the per-panel
/// build closures — which only get a `ChildSpawner` — can resolve art
/// surfaces. Greybox (mode off) keeps every surface exactly the legacy
/// BackgroundColor+BorderColor (no ImageNode at all — zero drift).
#[derive(Clone, Default)]
struct ArtSkin {
    mode: ArtMode,
    textures: UiTextures,
}

impl ArtSkin {
    fn of(world: &World) -> Self {
        Self {
            mode: *world.resource::<ArtMode>(),
            textures: world.resource::<UiTextures>().clone(),
        }
    }

    /// Folded into every panel's rebuild key: a texture reload or a mode
    /// flip re-skins (same packing as the engine's PanelKey seam).
    fn key_rev(&self) -> u64 {
        art_revision(&self.textures, &self.mode)
    }

    /// One panel-base surface (root face or slot face).
    fn surface(&self, slot: &str, fallback: Color, border_w: f32) -> Option<ImageNode> {
        art_surface(&self.mode, &self.textures, slot, fallback, border_w)
    }
}

/// Marker over pooled minimap enemy dots (positions refreshed per frame).
#[derive(Component, Debug)]
pub struct MinimapEnemyDot;

/// Marker on the 3D gold rings that mark every tower once the artifact is
/// claimed (领取后全队光环标记). Spawned as a CHILD of the tower (dies with
/// it); the tower carries `HasAuraRing` so the pass stays idempotent.
#[derive(Component, Debug)]
pub struct AuraRing;
#[derive(Component, Debug)]
pub struct HasAuraRing;

/// Run-once flags for the Update-frame retire passes.
#[derive(Resource, Debug)]
struct LegacyHudRetired;
#[derive(Resource, Debug)]
struct WarbasePanelDocked;

pub struct PanelsPlugin;

impl Plugin for PanelsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PanelKeys>()
            .init_resource::<PanelsSolo>()
            .add_systems(
                Startup,
                (
                    crate::systems::font::setup_ui_font,
                    spawn_panel_roots,
                )
                    .chain(),
            )
            .add_systems(
                Update,
                (
                    retire_legacy_hud,
                    dock_warbase_panel,
                    refresh_panels,
                    refresh_minimap_dots,
                    refresh_aura_rings,
                )
                    .chain(),
            );
    }
}

/// The single-line debug HUD (systems/hud.rs) retires with the Plan-A panels
/// (R2: no bare text over the battlefield). Update-frame despawn (Startup
/// ordering vs hud::spawn_hud is ambiguous; Update always runs after).
fn retire_legacy_hud(world: &mut World) {
    if world.contains_resource::<LegacyHudRetired>() {
        return;
    }
    world.insert_resource(LegacyHudRetired);
    let roots: Vec<Entity> = world
        .query_filtered::<Entity, With<crate::systems::hud::HudText>>()
        .iter(world)
        .collect();
    for e in roots {
        let _ = world.entity_mut(e).despawn();
    }
}

/// Hide the warbase-rendered `CommandPanel` (bottom-left strip — collides
/// with the BL hero panel; the BC console here re-faces its card data).
fn dock_warbase_panel(world: &mut World) {
    if world.contains_resource::<WarbasePanelDocked>() {
        return;
    }
    world.insert_resource(WarbasePanelDocked);
    let panels: Vec<Entity> = world
        .query_filtered::<Entity, With<warbase::ui::CommandPanel>>()
        .iter(world)
        .collect();
    for e in panels {
        if let Ok(mut em) = world.get_entity_mut(e) {
            em.insert(Visibility::Hidden);
        }
    }
}

fn spawn_panel_roots(world: &mut World) {
    let theme = world.resource::<UiTheme>().clone();
    let art = ArtSkin::of(world);
    for (kind, rect) in [
        (PanelKind::Resource, RES_RECT),
        (PanelKind::Wave, WAVE_RECT),
        (PanelKind::Minimap, MINIMAP_RECT),
        (PanelKind::Artifact, ARTIFACT_RECT),
        (PanelKind::Challenge, CHALLENGE_RECT),
        (PanelKind::Dig, DIG_RECT),
        (PanelKind::Bonds, BONDS_RECT),
        (PanelKind::Hero, HERO_RECT),
        (PanelKind::Equip, EQUIP_RECT),
        (PanelKind::Command, COMMAND_RECT),
    ] {
        let (node, bg, border, image) = panel_node(&theme, rect, &art);
        let mut root = world.spawn((PanelRoot(kind), node, bg, border));
        // Option<T> is not a Bundle — the art face joins only in art mode.
        if let Some(img) = image {
            root.insert(img);
        }
    }
}

// ---------------------------------------------------------------------------
// Shared builders (theme-token styled; the mock's proven vocabulary)
// ---------------------------------------------------------------------------

fn panel_node(
    theme: &UiTheme,
    r: Rect,
    art: &ArtSkin,
) -> (Node, BackgroundColor, BorderColor, Option<ImageNode>) {
    let s = &theme.spacing;
    // UITEX: the panel base joins the art seam — nine-sliced texture in
    // art mode (insets from panel_border_w), solid token color when the
    // slot has no handle, absent entirely while greybox (zero drift).
    let image = art.surface(UI_TEX_PANEL, theme.palette.panel_bg.color(), s.panel_border_w);
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
        image,
    )
}

/// Text carrying the bundled CJK font (every panel text goes through here).
fn font_text(font: &UiFont, s: &str, size: f32, color: Color) -> (Text, TextFont, TextColor) {
    (
        Text::new(s),
        TextFont { font: font.0.clone(), font_size: FontSize::Px(size), ..default() },
        TextColor(color),
    )
}

/// One geometric icon glyph (circle / square / pill — R2: icon-led values).
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

/// Static progress bar (rebuilt with its panel; frac clamped 0..1).
fn bar(p: &mut ChildSpawner, theme: &UiTheme, w: f32, frac: f32, fill: Color) {
    let h = theme.spacing.bar_h.max(4.0);
    p.spawn((
        Node {
            width: Val::Px(w),
            height: Val::Px(h),
            border: UiRect::all(Val::Px(1.0)),
            border_radius: BorderRadius::all(Val::Px(h / 2.0)),
            ..default()
        },
        BackgroundColor(theme.palette.panel_raised.color().with_alpha(0.5)),
        BorderColor::all(theme.palette.panel_border.color()),
    ))
    .with_children(|trough| {
        trough.spawn((
            Node {
                width: Val::Percent((frac * 100.0).clamp(0.0, 100.0)),
                height: Val::Percent(100.0),
                border_radius: BorderRadius::all(Val::Px(h / 2.0)),
                ..default()
            },
            BackgroundColor(fill),
        ));
    });
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

fn col_node() -> Node {
    Node {
        display: Display::Flex,
        flex_direction: FlexDirection::Column,
        row_gap: Val::Px(1.0),
        ..default()
    }
}

/// A row-facing slot surface: row layout + padding + border + radius in one
/// Node (spawning `row_node(..)` next to a styled `Node` would duplicate
/// the component — bevy bundle IDs are unordered, the dup panics).
fn slot_row_node(gap: f32, pad: f32, border_w: f32, radius: f32) -> Node {
    Node {
        display: Display::Flex,
        flex_direction: FlexDirection::Row,
        align_items: AlignItems::Center,
        column_gap: Val::Px(gap),
        padding: UiRect::all(Val::Px(pad)),
        border: UiRect::all(Val::Px(border_w)),
        border_radius: BorderRadius::all(Val::Px(radius)),
        ..default()
    }
}

// --- display vocabularies (CJK labels for defs/bonds/affixes) ----------------

/// Tower/def display names (loop.ron ids -> WC3-style Chinese).
pub fn def_name(id: &str) -> &str {
    match id {
        "hunter" => "猎手",
        "arrow" => "箭塔",
        "cannon" => "炮塔",
        "mage" => "法师塔",
        "sentry" => "哨塔",
        "ice" => "冰塔",
        "drum" => "战鼓塔",
        "corrode" => "腐蚀塔",
        "boss" => "首领",
        "grunt" => "步兵",
        "fast" => "疾行兽",
        "tank" => "装甲兽",
        "flyer" => "飞兵",
        _ => "塔",
    }
}

/// Bond display names (maps/bonds.ron ids -> the table's Chinese names).
fn bond_name(id: &str) -> &str {
    match id {
        "arrow_ice" => "箭雨急冻",
        "cannon_sentry" => "机动哨卫",
        _ => "羁绊",
    }
}

/// One bond's effect line: "全体攻+10%" / "全体间-8%".
fn stat_label(kind: StatKind, amount: f32) -> String {
    match kind {
        StatKind::DamageMult => format!("全体攻{:+.0}%", amount * 100.0),
        StatKind::AttackIntervalMult => format!("全体间{:+.0}%", amount * 100.0),
        StatKind::MoveSpeedMult => format!("全体速{:+.0}%", amount * 100.0),
        StatKind::DamageTakenMult => format!("全体承伤{:+.0}%", amount * 100.0),
        StatKind::RangeMult => format!("全体射程{:+.0}%", amount * 100.0),
    }
}

/// One equipment piece's affix summary: 攻+12% / 间-10% / 射程+30% (the
/// proposal §2 vocabulary; range rides StatKind::RangeMult since SEAM3b).
pub fn affix_label(item_id: &str, def: &ItemDef) -> String {
    let _ = item_id;
    if let Some(p) = &def.passive {
        return match p.kind {
            StatKind::DamageMult => format!("攻{:+.0}%", p.amount * 100.0),
            StatKind::AttackIntervalMult => format!("间{:+.0}%", p.amount * 100.0),
            StatKind::MoveSpeedMult => format!("速{:+.0}%", p.amount * 100.0),
            StatKind::DamageTakenMult => format!("伤{:+.0}%", p.amount * 100.0),
            StatKind::RangeMult => format!("射程{:+.0}%", p.amount * 100.0),
        };
    }
    String::new()
}

/// Hotkey badge letter: Digit1 -> "1", KeyQ -> "Q".
fn hotkey_badge(h: &Hotkey) -> String {
    match h {
        Hotkey::None => String::new(),
        Hotkey::Key(k) => format!("{k:?}").trim_start_matches("Key").trim_start_matches("Digit").to_string(),
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
    let art = ArtSkin::of(world);

    refresh_resource(world, &mut keys, &theme, &font, &art);
    refresh_wave(world, &mut keys, &theme, &font, &art);
    refresh_minimap(world, &mut keys, &theme, &font, &art);
    refresh_artifact(world, &mut keys, &theme, &font, &art);
    refresh_challenge(world, &mut keys, &theme, &font, &art);
    refresh_dig(world, &mut keys, &theme, &font, &art);
    refresh_bonds(world, &mut keys, &theme, &font, &art);
    refresh_hero(world, &mut keys, &theme, &font, &art);
    refresh_equip(world, &mut keys, &theme, &font, &art);
    refresh_command(world, &mut keys, &theme, &font, &art);

    // Screenshot seams: solo/hidden overrides win over state visibility.
    let solo = world.resource::<PanelsSolo>().0;
    let hidden_all = world.contains_resource::<PanelsHidden>();
    if hidden_all || solo.is_some() {
        let mut q = world.query_filtered::<(&PanelRoot, &mut Visibility), ()>();
        for (root, mut vis) in q.iter_mut(world) {
            let show = !hidden_all && solo == Some(root.0);
            *vis = if show { Visibility::Visible } else { Visibility::Hidden };
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
/// The art revision is folded into every panel's key here (the single
/// chokepoint) — a texture reload or mode flip rebuilds all ten.
#[allow(clippy::too_many_arguments)]
fn rebuild(
    world: &mut World,
    keys: &mut PanelKeys,
    kind: PanelKind,
    key: String,
    visible: bool,
    theme: &UiTheme,
    font: &UiFont,
    art: &ArtSkin,
    build: impl FnOnce(&mut ChildSpawner, &UiTheme, &UiFont, &ArtSkin),
) {
    let full_key = format!("{key}:a{}", art.key_rev());
    if keys.0.get(&kind) != Some(&full_key) {
        keys.0.insert(kind, full_key);
        let root = panel_root(world, kind);
        // Drop the previous subtree (Texts are grandchildren).
        let children: Vec<Entity> =
            world.get::<Children>(root).map(|c| c.to_vec()).unwrap_or_default();
        for c in children {
            despawn_subtree(world, c);
        }
        // Re-skin the root itself (theme swap reaches every surface).
        {
            let (.., bg, border, image) = panel_node(theme, rect_of(kind), art);
            let mut e = world.get_entity_mut(root).unwrap();
            if let Some(mut b) = e.get_mut::<BackgroundColor>() {
                *b = bg;
            }
            if let Some(mut b) = e.get_mut::<BorderColor>() {
                *b = border;
            }
            // UITEX: the root face swaps/keeps its ImageNode with art mode
            // (greybox drops it entirely — the legacy pair renders alone).
            match image {
                Some(img) => {
                    if e.get::<ImageNode>().is_some() {
                        *e.get_mut::<ImageNode>().unwrap() = img;
                    } else {
                        e.insert(img);
                    }
                }
                None => {
                    e.remove::<ImageNode>();
                }
            }
        }
        world
            .get_entity_mut(root)
            .unwrap()
            .with_children(|p| build(p, theme, font, art));
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
        PanelKind::Wave => WAVE_RECT,
        PanelKind::Minimap => MINIMAP_RECT,
        PanelKind::Artifact => ARTIFACT_RECT,
        PanelKind::Challenge => CHALLENGE_RECT,
        PanelKind::Dig => DIG_RECT,
        PanelKind::Bonds => BONDS_RECT,
        PanelKind::Hero => HERO_RECT,
        PanelKind::Equip => EQUIP_RECT,
        PanelKind::Command => COMMAND_RECT,
    }
}

fn despawn_subtree(world: &mut World, root: Entity) {
    let children: Vec<Entity> = world.get::<Children>(root).map(|c| c.to_vec()).unwrap_or_default();
    for c in children {
        despawn_subtree(world, c);
    }
    let _ = world.entity_mut(root).despawn();
}

// --- TL: resource plaque -----------------------------------------------------

fn refresh_resource(
    world: &mut World,
    keys: &mut PanelKeys,
    theme: &UiTheme,
    font: &UiFont,
    _art: &ArtSkin,
) {
    let (gold, wood) = {
        let r = world.resource::<Resources>();
        (r.gold, r.wood)
    };
    let (used, pop_cap) = {
        let p = world.resource::<Population>();
        (p.used, p.cap)
    };
    let stock = world.resource::<StockCount>().0;
    let cap = match world.resource::<MatchRules>().leak {
        LeakPolicy::Stockpile { cap } => cap,
        _ => 50,
    };
    let frac = (stock as f32 / cap.max(1) as f32).clamp(0.0, 1.0);
    let stock_color = if frac >= 0.9 {
        theme.state_colors.health_low.color()
    } else if frac >= 0.7 {
        theme.state_colors.warning.color()
    } else {
        theme.state_colors.health.color()
    };
    let key = format!(
        "res:{}/{}/{}:{}:{}:{rev}",
        gold, wood, used, pop_cap, stock,
        rev = theme.revision
    );
    rebuild(world, keys, PanelKind::Resource, key, true, theme, font, _art, |p, theme, font, _art| {
        let t = &theme.typography;
        let sc = &theme.state_colors;
        let gap = theme.spacing.panel_gap + 2.0;
        // 金 / 木 / 人口 rows: icon-led values (R4).
        for (glyph, color, value, label) in [
            (Glyph::Circle, sc.gold.color(), gold.to_string(), "金"),
            (Glyph::Square, sc.wood.color(), wood.to_string(), "木"),
            (Glyph::Pill, theme.palette.accent.color(), format!("{used}/{pop_cap}"), "人口"),
        ] {
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
        // 存量 meter with the 90% red-line tick.
        let inner = RES_RECT.w - 2.0 * (theme.spacing.panel_gap + theme.spacing.panel_border_w).max(8.0) - 6.0;
        p.spawn(row_node(gap)).with_children(|row| {
            let (lt, lf, lc) = font_text(font, "存量", t.font_caption, theme.palette.text_secondary.color());
            row.spawn((lt, lf, lc));
            row.spawn((
                Node {
                    width: Val::Px(inner * 0.66),
                    height: Val::Px(theme.spacing.bar_h.max(4.0)),
                    border: UiRect::all(Val::Px(1.0)),
                    border_radius: BorderRadius::all(Val::Px(theme.spacing.bar_h / 2.0)),
                    ..default()
                },
                BackgroundColor(theme.palette.panel_raised.color().with_alpha(0.5)),
                BorderColor::all(theme.palette.panel_border.color()),
            ))
            .with_children(|trough| {
                trough.spawn((
                    Node {
                        width: Val::Percent(frac * 100.0),
                        height: Val::Percent(100.0),
                        border_radius: BorderRadius::all(Val::Px(theme.spacing.bar_h / 2.0)),
                        ..default()
                    },
                    BackgroundColor(stock_color),
                ));
                trough.spawn((
                    Node {
                        position_type: PositionType::Absolute,
                        left: Val::Percent(90.0),
                        top: Val::Px(0.0),
                        width: Val::Px(2.0),
                        height: Val::Percent(100.0),
                        ..default()
                    },
                    BackgroundColor(theme.state_colors.health_low.color()),
                ));
            });
            let label = format!("{stock}/{cap}");
            let (st, sf, sc2) = font_text(font, &label, t.font_caption, theme.palette.text_secondary.color());
            row.spawn((st, sf, sc2));
        });
    });
}

// --- TC: wave banner ---------------------------------------------------------

fn refresh_wave(
    world: &mut World,
    keys: &mut PanelKeys,
    theme: &UiTheme,
    font: &UiFont,
    _art: &ArtSkin,
) {
    let (label, hot) = {
        let d = world.resource::<WaveDirector>();
        match d.phase {
            DirectorPhase::Intermission => (
                format!("第 {} 波 · {}s 后来袭", d.next_wave + 1, d.phase_timer.max(0.0).ceil() as u32),
                false,
            ),
            DirectorPhase::Spawning => (format!("第 {} 波 · 来袭", d.running.unwrap_or(0) + 1), true),
            _ => (format!("第 {} 波", d.next_wave + 1), false),
        }
    };
    let key = format!("wave:{label}:{}", theme.revision);
    rebuild(world, keys, PanelKind::Wave, key, true, theme, font, _art, |p, theme, font, _art| {
        let (t, f, c) = font_text(
            font,
            &label,
            theme.typography.font_title,
            if hot {
                theme.state_colors.warning.color()
            } else {
                theme.palette.text_primary.color()
            },
        );
        p.spawn((t, f, c));
    });
}

// --- TR: minimap --------------------------------------------------------------

fn refresh_minimap(
    world: &mut World,
    keys: &mut PanelKeys,
    theme: &UiTheme,
    font: &UiFont,
    _art: &ArtSkin,
) {
    // Static frame rebuilds on wave/tower-count/theme change; live enemy dots
    // are a pooled per-frame pass (refresh_minimap_dots).
    let wave = {
        let d = world.resource::<WaveDirector>();
        d.running.map(|w| w + 1).unwrap_or(d.next_wave + 1)
    };
    let towers: Vec<(f32, f32)> = {
        let mut q = world.query_filtered::<(&Transform, &Allegiance), With<UnitInstance>>();
        q.iter(world)
            .filter(|(_, side)| **side == Allegiance::Defender)
            .map(|(tf, _)| (tf.translation.x, tf.translation.z))
            .collect()
    };
    let key = format!("map:{wave}:{}:{}", towers.len(), theme.revision);
    let dots = towers.clone();
    rebuild(world, keys, PanelKind::Minimap, key, true, theme, font, _art, |p, theme, font, _art| {
        // Ring track.
        p.spawn((
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(10.0),
                left: Val::Px(10.0),
                width: Val::Px(MINIMAP_RECT.w - 20.0),
                height: Val::Px(MINIMAP_RECT.h - 20.0),
                border: UiRect::all(Val::Px(2.0)),
                border_radius: BorderRadius::all(Val::Px(14.0)),
                ..default()
            },
            BackgroundColor(Color::NONE),
            BorderColor::all(theme.palette.panel_border.color()),
        ));
        // Tower dots (rebuild-static: towers do not move).
        for (x, z) in &dots {
            let (fx, fy) = world_to_minimap(*x, *z);
            p.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Percent(fx),
                    top: Val::Percent(fy),
                    width: Val::Px(4.0),
                    height: Val::Px(4.0),
                    ..default()
                },
                BackgroundColor(theme.state_colors.health.color()),
            ));
        }
        // Enemy dot pool (refreshed per frame).
        for _ in 0..12 {
            p.spawn((
                MinimapEnemyDot,
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Percent(50.0),
                    top: Val::Percent(50.0),
                    width: Val::Px(4.0),
                    height: Val::Px(4.0),
                    ..default()
                },
                BackgroundColor(theme.state_colors.enemy.color()),
                Visibility::Hidden,
            ));
        }
        // Wave badge corner.
        let badge = format!("W{wave}");
        let (wt, wf, wc) =
            font_text(font, &badge, theme.typography.font_caption, theme.palette.text_secondary.color());
        p.spawn((wt, wf, wc, Node {
            position_type: PositionType::Absolute,
            bottom: Val::Px(3.0),
            right: Val::Px(6.0),
            ..default()
        }));
    });
}

/// Loop-map grid spans x in [-14,14], z in [-9,9] (28x18 cells).
fn world_to_minimap(x: f32, z: f32) -> (f32, f32) {
    (
        ((x + 14.0) / 28.0 * 100.0).clamp(2.0, 97.0),
        ((9.0 - z) / 18.0 * 100.0).clamp(2.0, 97.0),
    )
}

/// Per-frame: map live attackers onto the pooled enemy dots.
fn refresh_minimap_dots(world: &mut World) {
    let dots: Vec<Entity> = world
        .query_filtered::<Entity, With<MinimapEnemyDot>>()
        .iter(world)
        .collect();
    if dots.is_empty() {
        return; // panel not built yet (or torn down) — nothing to do
    }
    let enemies: Vec<(f32, f32)> = {
        let mut q = world.query_filtered::<(&Transform, &Allegiance), With<UnitInstance>>();
        q.iter(world)
            .filter(|(_, side)| **side == Allegiance::Attacker)
            .map(|(tf, _)| (tf.translation.x, tf.translation.z))
            .collect()
    };
    for (i, dot) in dots.iter().enumerate() {
        let vis = match enemies.get(i) {
            Some((x, z)) => {
                let (fx, fy) = world_to_minimap(*x, *z);
                if let Some(mut node) = world.get_mut::<Node>(*dot) {
                    node.left = Val::Percent(fx);
                    node.top = Val::Percent(fy);
                }
                Visibility::Visible
            }
            None => Visibility::Hidden,
        };
        if let Some(mut v) = world.get_mut::<Visibility>(*dot) {
            *v = vis;
        }
    }
}

// --- LT2-2: artifact quest plaque ---------------------------------------------

fn refresh_artifact(
    world: &mut World,
    keys: &mut PanelKeys,
    theme: &UiTheme,
    font: &UiFont,
    _art: &ArtSkin,
) {
    let (waves, bosses) = {
        let q = world
            .get_resource::<crate::systems::artifact::ArtifactQuest>();
        (
            q.map(|q| q.waves_cleared).unwrap_or(0),
            q.map(|q| q.boss_kills).unwrap_or(0),
        )
    };
    let claimed = world.contains_resource::<crate::systems::artifact::ClaimedArtifact>();
    let waves_req = crate::systems::artifact::ARTIFACT_WAVES_REQUIRED;
    let bosses_req = crate::systems::artifact::ARTIFACT_BOSSES_REQUIRED;
    let complete = waves >= waves_req && bosses >= bosses_req;
    let state = if claimed { 2 } else if complete { 1 } else { 0 };
    let key = format!("art:{state}:{waves}/{bosses}:{}", theme.revision);
    rebuild(world, keys, PanelKind::Artifact, key, true, theme, font, _art, |p, theme, font, _art| {
        let t = &theme.typography;
        let gold = theme.state_colors.gold.color();
        let inner =
            ARTIFACT_RECT.w - 2.0 * (theme.spacing.panel_gap + theme.spacing.panel_border_w).max(8.0) - 6.0;
        match state {
            2 => {
                // Claimed: gold plaque + the team aura line.
                p.spawn(row_node(theme.spacing.panel_gap + 2.0)).with_children(|row| {
                    row.spawn(icon_node(gold, theme.spacing.icon_size, Glyph::Circle));
                    let (tt, tf, tc) = font_text(font, "神器 · 已认领", t.font_title, gold);
                    row.spawn((tt, tf, tc));
                });
                let amount = crate::systems::artifact::ARTIFACT_AURA_AMOUNT * 100.0;
                let line = format!("全队光环 · 攻击{amount:+.0}%");
                let (lt, lf, lc) = font_text(font, &line, t.font_caption, theme.palette.text_primary.color());
                p.spawn((lt, lf, lc));
            }
            1 => {
                // Claimable: gold highlight + the [Q] call-to-action.
                p.spawn(row_node(theme.spacing.panel_gap + 2.0)).with_children(|row| {
                    row.spawn(icon_node(gold, theme.spacing.icon_size, Glyph::Circle));
                    let (tt, tf, tc) = font_text(font, "神器 · 可领取 [Q]", t.font_title, gold);
                    row.spawn((tt, tf, tc));
                });
                let (lt, lf, lc) =
                    font_text(font, "全队攻击光环待领取", t.font_caption, theme.palette.text_secondary.color());
                p.spawn((lt, lf, lc));
            }
            _ => {
                let (tt, tf, tc) = font_text(font, "神器任务", t.font_title, theme.palette.text_primary.color());
                p.spawn((tt, tf, tc));
            }
        }
        if state < 2 {
            for (label, have, need, color) in [
                ("清波", waves, waves_req, gold),
                ("首领", bosses, bosses_req, theme.state_colors.enemy.color()),
            ] {
                p.spawn(row_node(theme.spacing.panel_gap)).with_children(|row| {
                    let (lt, lf, lc) = font_text(font, label, t.font_caption, theme.palette.text_secondary.color());
                    row.spawn((lt, lf, lc));
                    bar(&mut *row, theme, inner * 0.62, have as f32 / need as f32, color);
                    let vt = format!("{have}/{need}");
                    let done = have >= need;
                    let (vt2, vf, vc) = font_text(
                        font,
                        &vt,
                        t.font_caption,
                        if done { gold } else { theme.palette.text_secondary.color() },
                    );
                    row.spawn((vt2, vf, vc));
                });
            }
        }
    });
}

// --- LT2-3: challenge entry ----------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum ChallengeView {
    Hidden,
    Offered,
    Armed,
}

fn challenge_view(world: &World) -> ChallengeView {
    if *world.resource::<MatchFlow>() != MatchFlow::Running {
        return ChallengeView::Hidden;
    }
    let (intermission, next_wave) = {
        let d = world.resource::<WaveDirector>();
        (matches!(d.phase, DirectorPhase::Intermission), d.next_wave)
    };
    let armed = world
        .get_resource::<crate::systems::challenge::ChallengeState>()
        .map(|s| s.armed.is_some())
        .unwrap_or(false);
    if armed {
        return if intermission { ChallengeView::Armed } else { ChallengeView::Hidden };
    }
    let spent = world
        .get_resource::<crate::systems::challenge::ChallengeState>()
        .map(|s| s.offered.contains(&(next_wave / crate::systems::challenge::CHALLENGE_MILESTONE)))
        .unwrap_or(false);
    if intermission
        && next_wave > 0
        && next_wave % crate::systems::challenge::CHALLENGE_MILESTONE == 0
        && !spent
    {
        return ChallengeView::Offered;
    }
    ChallengeView::Hidden
}

fn refresh_challenge(
    world: &mut World,
    keys: &mut PanelKeys,
    theme: &UiTheme,
    font: &UiFont,
    _art: &ArtSkin,
) {
    let view = challenge_view(world);
    let key = format!("chg:{view:?}:{}", theme.revision);
    let visible = view != ChallengeView::Hidden;
    rebuild(world, keys, PanelKind::Challenge, key, visible, theme, font, _art, |p, theme, font, _art| {
        let t = &theme.typography;
        let gold = theme.state_colors.gold.color();
        let sub = format!(
            "血量x{} · 赏金x{}",
            crate::systems::challenge::CHALLENGE_HP_MULT as u32,
            crate::systems::challenge::CHALLENGE_REWARD_MULT
        );
        match view {
            ChallengeView::Offered => {
                // Gold command surface: dark iron text on gold (the warden
                // token mapping for text-on-gold).
                p.spawn((
                    Node {
                        display: Display::Flex,
                        flex_direction: FlexDirection::Row,
                        align_items: AlignItems::Center,
                        column_gap: Val::Px(theme.spacing.panel_gap + 2.0),
                        border: UiRect::all(Val::Px(theme.spacing.slot_border_w.max(1.0))),
                        border_radius: BorderRadius::all(Val::Px(theme.radius.slot)),
                        padding: UiRect::all(Val::Px(4.0)),
                        ..default()
                    },
                    BackgroundColor(gold),
                    BorderColor::all(theme.palette.accent.color()),
                ))
                .with_children(|row| {
                    let (tt, tf, tc) =
                        font_text(font, "挑战下一波 [C]", t.font_title, theme.palette.panel_bg.color());
                    row.spawn((tt, tf, tc));
                    let (st, sf, sc) = font_text(font, &sub, t.font_caption, theme.palette.panel_bg.color());
                    row.spawn((st, sf, sc));
                });
            }
            ChallengeView::Armed => {
                p.spawn(row_node(theme.spacing.panel_gap + 2.0)).with_children(|row| {
                    let (tt, tf, tc) = font_text(font, "挑战已备战!", t.font_title, gold);
                    row.spawn((tt, tf, tc));
                    let (st, sf, sc) =
                        font_text(font, &sub, t.font_caption, theme.palette.text_secondary.color());
                    row.spawn((st, sf, sc));
                });
            }
            ChallengeView::Hidden => {}
        }
    });
}

// --- LT2-4: dig entry -------------------------------------------------------------

fn refresh_dig(
    world: &mut World,
    keys: &mut PanelKeys,
    theme: &UiTheme,
    font: &UiFont,
    _art: &ArtSkin,
) {
    let intermission = matches!(world.resource::<WaveDirector>().phase, DirectorPhase::Intermission)
        && *world.resource::<MatchFlow>() == MatchFlow::Running;
    let afford = world.resource::<Resources>().gold >= crate::systems::dig::DIG_COST;
    let last = world
        .get_resource::<crate::systems::dig::DigStats>()
        .and_then(|s| s.last.clone())
        .map(|o| match o {
            crate::systems::dig::DigOutcome::Nothing => "空手".to_string(),
            crate::systems::dig::DigOutcome::Gold(g) => format!("+{g}金"),
            crate::systems::dig::DigOutcome::Equipment(_) => "装备!".to_string(),
        })
        .unwrap_or_default();
    let key = format!("dig:{intermission}:{afford}:{last}:{}", theme.revision);
    rebuild(world, keys, PanelKind::Dig, key, intermission, theme, font, _art, |p, theme, font, _art| {
        let t = &theme.typography;
        let title = format!("挖宝 [V] · {}金", crate::systems::dig::DIG_COST);
        let (bg, fg, border) = if afford {
            (
                theme.palette.panel_raised.color(),
                theme.palette.text_primary.color(),
                theme.palette.accent.color(),
            )
        } else {
            (
                theme.state_colors.slot_disabled_bg.color(),
                theme.palette.text_dim.color(),
                theme.palette.panel_border.color(),
            )
        };
        p.spawn((
            Node {
                display: Display::Flex,
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: Val::Px(theme.spacing.panel_gap + 2.0),
                padding: UiRect::all(Val::Px(4.0)),
                border: UiRect::all(Val::Px(theme.spacing.slot_border_w.max(1.0))),
                border_radius: BorderRadius::all(Val::Px(theme.radius.slot)),
                ..default()
            },
            BackgroundColor(bg),
            BorderColor::all(border),
        ))
        .with_children(|row| {
            let (tt, tf, tc) = font_text(font, &title, t.font_label, fg);
            row.spawn((tt, tf, tc));
            if !last.is_empty() {
                let (lt, lf, lc) =
                    font_text(font, &format!("上次:{last}"), t.font_caption, theme.palette.text_secondary.color());
                row.spawn((lt, lf, lc));
            }
        });
        let (ct, cf, cc) = font_text(
            font,
            "期望 +31.5金 · 50%空手 35%+80金 15%装备",
            t.font_caption,
            theme.palette.text_secondary.color(),
        );
        p.spawn((ct, cf, cc));
    });
}

// --- LT2-5: bond chips --------------------------------------------------------------

fn refresh_bonds(
    world: &mut World,
    keys: &mut PanelKeys,
    theme: &UiTheme,
    font: &UiFont,
    _art: &ArtSkin,
) {
    let Some(table) = world.get_resource::<crate::systems::bonds::BondTable>().cloned() else {
        set_visible(world, PanelKind::Bonds, false);
        return;
    };
    if table.bonds.is_empty() {
        set_visible(world, PanelKind::Bonds, false);
        return;
    }
    // Live per-def counts.
    let mut counts: HashMap<String, u32> = HashMap::new();
    {
        let mut q = world.query_filtered::<(&UnitInstance, &Allegiance), Without<Dead>>();
        for (inst, side) in q.iter(world) {
            if *side == Allegiance::Defender {
                *counts.entry(inst.def.clone()).or_default() += 1;
            }
        }
    }
    let mut key = format!("bonds:{}", theme.revision);
    for b in &table.bonds {
        let satisfied = b.requires.iter().all(|(def, need)| counts.get(def).copied().unwrap_or(0) >= *need);
        key.push_str(&format!(
            "[{}:{}:{satisfied}]",
            b.id,
            b.requires
                .iter()
                .map(|(d, n)| format!("{d}{}", counts.get(d).copied().unwrap_or(0).min(*n)))
                .collect::<Vec<_>>()
                .join("/")
        ));
    }
    rebuild(world, keys, PanelKind::Bonds, key, true, theme, font, _art, |p, theme, font, _art| {
        let t = &theme.typography;
        for b in &table.bonds {
            let satisfied =
                b.requires.iter().all(|(def, need)| counts.get(def).copied().unwrap_or(0) >= *need);
            let alpha = if satisfied { 1.0 } else { 0.62 };
            p.spawn((
                slot_row_node(
                    theme.spacing.panel_gap + 2.0,
                    theme.spacing.panel_gap,
                    if satisfied { theme.spacing.slot_border_w.max(1.0) } else { 0.0 },
                    theme.radius.pill.min(14.0),
                ),
                BackgroundColor(if satisfied {
                    theme.palette.panel_raised.color().with_alpha(alpha)
                } else {
                    theme.palette.panel_bg.color().with_alpha(alpha * 0.8)
                }),
                BorderColor::all(if satisfied {
                    theme.palette.accent.color()
                } else {
                    theme.palette.panel_border.color()
                }),
            ))
            .with_children(|chip| {
                chip.spawn(icon_node(
                    if satisfied { theme.palette.accent.color() } else { theme.palette.text_dim.color() },
                    theme.spacing.icon_size * 0.7,
                    Glyph::Square,
                ));
                chip.spawn(col_node()).with_children(|txt| {
                    let name_color = if satisfied {
                        theme.palette.text_primary.color()
                    } else {
                        theme.palette.text_dim.color()
                    };
                    let head = format!("{} · {}", bond_name(&b.id), stat_label(b.kind, b.amount));
                    let (nt, nf, nc) = font_text(font, &head, t.font_label, name_color);
                    txt.spawn((nt, nf, nc));
                    // Requirement line with live counts; on-field members lit.
                    let req: Vec<String> = b
                        .requires
                        .iter()
                        .map(|(def, need)| {
                            let have = counts.get(def).copied().unwrap_or(0);
                            format!("{}{have}/{}", def_name(def), need)
                        })
                        .collect();
                    let (dt, df, dc) =
                        font_text(font, &req.join(" + "), t.font_caption, theme.palette.text_secondary.color());
                    txt.spawn((dt, df, dc));
                });
            });
        }
    });
}

// --- ⑥ hero panel (with equip affix lines) ------------------------------------------

/// The currently selected hero tower (selection + hero kit), if any.
fn selected_hero(world: &World) -> Option<Entity> {
    match world.resource::<Selection>().mode {
        SelectionMode::Unit(e) => world.get::<XpHolder>(e).is_some().then_some(e),
        _ => None,
    }
}

/// Display snapshot for the hero/equip panels.
struct HeroView {
    entity: Entity,
    def: String,
    level: u8,
    hp: (f32, f32),
    mana: Option<(f32, f32)>,
    xp: (u32, u32),
    range: Option<f32>,
    equips: Vec<(String, String)>, // (item display name, affix)
}

fn hero_view(world: &World) -> Option<HeroView> {
    let e = selected_hero(world)?;
    let def = world.get::<UnitInstance>(e)?.def.clone();
    let xp = world.get::<XpHolder>(e)?;
    let level = xp.level;
    let hp = (
        world.get::<Hp>(e).map(|h| h.0).unwrap_or(0.0),
        world.get::<MaxHp>(e).map(|m| m.0).unwrap_or(1.0),
    );
    let mana = match (world.get::<Mana>(e), world.get::<MaxMana>(e)) {
        (Some(m), Some(max)) => Some((m.0, max.0)),
        _ => None,
    };
    let xp_pair = (xp.xp, xp.next_level_xp);
    let range = world.get::<AttackKit>(e).map(|k| k.def.range);
    let registry = world.resource::<ItemRegistry>();
    let equips: Vec<(String, String)> = world
        .get::<EquipSlots>(e)
        .map(|rack| {
            rack.slots
                .iter()
                .filter_map(|s| s.as_ref())
                .map(|it| {
                    let name = registry
                        .get(&it.def)
                        .map(|d| d.name.clone())
                        .unwrap_or_else(|| it.def.clone());
                    let affix = registry.get(&it.def).map(|d| affix_label(&it.def, d)).unwrap_or_default();
                    (name, affix)
                })
                .collect()
        })
        .unwrap_or_default();
    Some(HeroView { entity: e, def, level, hp, mana, xp: xp_pair, range, equips })
}

fn hero_key(v: &HeroView, rev: u64) -> String {
    format!(
        "hero:{}:{}:{}:{}/{}:{:?}:{}/{}:{:?}:{:?}:{rev}",
        v.entity.index(),
        v.def,
        v.level,
        v.hp.0.ceil() as i32,
        v.hp.1 as i32,
        v.mana.map(|(a, b)| (a.ceil() as i32, b as i32)),
        v.xp.0 as i32,
        v.xp.1 as i32,
        v.range.map(|r| r as i32),
        v.equips
    )
}

fn refresh_hero(
    world: &mut World,
    keys: &mut PanelKeys,
    theme: &UiTheme,
    font: &UiFont,
    _art: &ArtSkin,
) {
    let Some(v) = hero_view(world) else {
        keys.0.insert(PanelKind::Hero, "hero:none".into());
        set_visible(world, PanelKind::Hero, false);
        return;
    };
    let key = hero_key(&v, theme.revision);
    rebuild(world, keys, PanelKind::Hero, key, true, theme, font, _art, |p, theme, font, _art| {
        let t = &theme.typography;
        let sc = &theme.state_colors;
        let inner = HERO_RECT.w - 2.0 * (theme.spacing.panel_gap + theme.spacing.panel_border_w).max(8.0);
        // Everything rides one tight column (row_gap 2) so head + 3 bars +
        // affix lines fit the 140px console slot without clipping.
        p.spawn(Node {
            display: Display::Flex,
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(2.0),
            ..default()
        })
        .with_children(|col| {
            // Head: portrait (LV badge) + name.
            col.spawn(row_node(theme.spacing.panel_gap + 2.0)).with_children(|head| {
                head.spawn((
                    Node {
                        width: Val::Px(34.0),
                        height: Val::Px(34.0),
                        border: UiRect::all(Val::Px(theme.spacing.slot_border_w.max(1.0))),
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::FlexEnd,
                        padding: UiRect::all(Val::Px(1.0)),
                        border_radius: BorderRadius::all(Val::Px(theme.radius.slot)),
                        ..default()
                    },
                    BackgroundColor(theme.palette.panel_raised.color()),
                    BorderColor::all(theme.palette.accent.color()),
                ))
                .with_children(|port| {
                    port.spawn(icon_node(
                        theme.palette.accent.color().with_alpha(0.55),
                        12.0,
                        Glyph::Circle,
                    ));
                    let badge = format!("LV{}", v.level);
                    let (bt, bf, bc) = font_text(font, &badge, t.font_caption, theme.palette.accent.color());
                    port.spawn((bt, bf, bc));
                });
                head.spawn(col_node()).with_children(|name| {
                    let (nt, nf, nc) = font_text(font, def_name(&v.def), t.font_title, theme.palette.text_primary.color());
                    name.spawn((nt, nf, nc));
                    let sub = format!("英雄塔{}", v.range.map(|r| format!(" · 射程 {r:.1}")).unwrap_or_default());
                    let (st, sf, sc2) = font_text(font, &sub, t.font_caption, theme.palette.text_secondary.color());
                    name.spawn((st, sf, sc2));
                });
            });
            // HP / MP / XP bars (MP only when the hero has mana).
            let mut rows: Vec<(&str, (f32, f32), Color)> = vec![
                ("生命", v.hp, sc.health.color()),
                ("经验", (v.xp.0 as f32, v.xp.1 as f32), sc.xp.color()),
            ];
            if let Some(mp) = v.mana {
                rows.insert(1, ("魔法", mp, sc.mana.color()));
            }
            for (label, (cur, max), color) in rows {
                col.spawn(row_node(theme.spacing.panel_gap)).with_children(|row| {
                    let (lt, lf, lc) = font_text(font, label, t.font_caption, theme.palette.text_secondary.color());
                    row.spawn((lt, lf, lc));
                    bar(&mut *row, theme, inner * 0.52, cur / max.max(1.0), color);
                    let val = format!("{:.0}/{}", cur.max(0.0), max.max(1.0));
                    let (vt, vf, vc) = font_text(font, &val, t.font_caption, theme.palette.text_secondary.color());
                    row.spawn((vt, vf, vc));
                });
            }
            // ⑥ equip affix lines (one per occupied slot; empty slots live in
            // the equip rack panel, not here).
            for (name, affix) in &v.equips {
                let line = if affix.is_empty() { name.clone() } else { format!("{name} · {affix}") };
                let (lt, lf, lc) = font_text(font, &line, t.font_caption, theme.palette.text_secondary.color());
                col.spawn((lt, lf, lc));
            }
        });
    });
}

// --- ① equipment rack -------------------------------------------------------------

fn refresh_equip(
    world: &mut World,
    keys: &mut PanelKeys,
    theme: &UiTheme,
    font: &UiFont,
    art: &ArtSkin,
) {
    let Some(hero) = selected_hero(world) else {
        keys.0.insert(PanelKind::Equip, "equip:none".into());
        set_visible(world, PanelKind::Equip, false);
        return;
    };
    let rack = world.get::<EquipSlots>(hero).cloned();
    let registry = world.resource::<ItemRegistry>().clone();
    // Precompute display rows (registry lookups off the hot path).
    let rows: Vec<Option<(String, String)>> = rack
        .as_ref()
        .map(|r| {
            r.slots
                .iter()
                .map(|s| {
                    s.as_ref().map(|it| {
                        let name = registry
                            .get(&it.def)
                            .map(|d| d.name.clone())
                            .unwrap_or_else(|| it.def.clone());
                        let affix = registry.get(&it.def).map(|d| affix_label(&it.def, d)).unwrap_or_default();
                        (name, affix)
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    let key = format!(
        "equip:{}:{}:{}",
        hero.index(),
        rows.iter().map(|r| r.as_ref().map(|(n, _)| n.clone()).unwrap_or_default()).collect::<Vec<_>>().join(","),
        theme.revision
    );
    rebuild(world, keys, PanelKind::Equip, key, true, theme, font, art, |p, theme, font, art| {
        let t = &theme.typography;
        let (ht, hf, hc) = font_text(font, "装备", t.font_title, theme.palette.accent.color());
        p.spawn((ht, hf, hc));
        for slot in 0..EQUIP_SLOTS {
            match rows.get(slot).cloned().flatten() {
                Some((name, affix)) => {
                    // UITEX: the slot face rides the art seam (slicer
                    // insets from the slot border token — .max(1.0), the
                    // same width this face draws its border at).
                    let slot_art = art.surface(
                        UI_TEX_SLOT,
                        theme.palette.panel_raised.color(),
                        theme.spacing.slot_border_w.max(1.0),
                    );
                    let mut row_e = p.spawn((
                        slot_row_node(
                            theme.spacing.panel_gap + 2.0,
                            theme.spacing.slot_pad,
                            theme.spacing.slot_border_w.max(1.0),
                            theme.radius.slot,
                        ),
                        BackgroundColor(theme.palette.panel_raised.color()),
                        BorderColor::all(theme.palette.accent.color()),
                    ));
                    if let Some(img) = slot_art {
                        row_e.insert(img);
                    }
                    row_e
                    .with_children(|row| {
                        row.spawn(icon_node(theme.palette.accent.color(), theme.spacing.icon_size * 0.7, Glyph::Square));
                        let (nt, nf, nc) = font_text(font, &name, t.font_label, theme.palette.text_primary.color());
                        row.spawn((nt, nf, nc));
                        if !affix.is_empty() {
                            let (at, af, ac) =
                                font_text(font, &affix, t.font_caption, theme.state_colors.gold.color());
                            row.spawn((at, af, ac));
                        }
                    });
                }
                None => {
                    let slot_art = art.surface(
                        UI_TEX_SLOT,
                        theme.state_colors.slot_disabled_bg.color(),
                        theme.spacing.slot_border_w.max(1.0),
                    );
                    let mut row_e = p.spawn((
                        slot_row_node(
                            theme.spacing.panel_gap + 2.0,
                            theme.spacing.slot_pad,
                            theme.spacing.slot_border_w.max(1.0),
                            theme.radius.slot,
                        ),
                        BackgroundColor(theme.state_colors.slot_disabled_bg.color()),
                        BorderColor::all(theme.palette.panel_border.color()),
                    ));
                    if let Some(img) = slot_art {
                        row_e.insert(img);
                    }
                    row_e
                    .with_children(|row| {
                        row.spawn(icon_node(theme.palette.text_dim.color(), theme.spacing.icon_size * 0.7, Glyph::Square));
                        let (nt, nf, nc) = font_text(font, "空 · [E] 上装", t.font_label, theme.palette.text_dim.color());
                        row.spawn((nt, nf, nc));
                    });
                }
            }
        }
    });
}

// --- BC: command console -----------------------------------------------------------

fn refresh_command(
    world: &mut World,
    keys: &mut PanelKeys,
    theme: &UiTheme,
    font: &UiFont,
    art: &ArtSkin,
) {
    let Some(card_key) = active_card_key(world) else {
        keys.0.insert(PanelKind::Command, "cmd:none".into());
        set_visible(world, PanelKind::Command, false);
        return;
    };
    let card = world.resource::<CommandCards>().cards.get(&card_key).cloned();
    let version = world.resource::<PanelVersion>().0;
    let key = format!("cmd:{card_key}:{version}:{}", theme.revision);
    rebuild(world, keys, PanelKind::Command, key, true, theme, font, art, |p, theme, font, art| {
        let Some(card) = &card else { return };
        let t = &theme.typography;
        let s = &theme.spacing;
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
            for slot in card.slots.iter().take(8) {
                let badge = hotkey_badge(&slot.hotkey);
                let (bg, fg) = if slot.enabled {
                    (theme.palette.panel_raised.color(), theme.palette.text_primary.color())
                } else {
                    (theme.state_colors.slot_disabled_bg.color(), theme.palette.text_dim.color())
                };
                // UITEX: the command slot face rides the art seam (insets
                // from the slot border token at the drawn .max(1.0) width).
                let slot_art =
                    art.surface(UI_TEX_SLOT, bg, s.slot_border_w.max(1.0));
                let mut slot_e = grid.spawn((
                    Button,
                    Interaction::None,
                    CommandSlotButton {
                        action: slot.action.clone(),
                        enabled: slot.enabled,
                        tooltip: slot.tooltip.clone(),
                    },
                    Node {
                        width: Val::Px(s.slot_w),
                        height: Val::Px(s.slot_h),
                        padding: UiRect::all(Val::Px(s.slot_pad)),
                        border: UiRect::all(Val::Px(s.slot_border_w.max(1.0))),
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        border_radius: BorderRadius::all(Val::Px(theme.radius.slot)),
                        ..default()
                    },
                    BackgroundColor(bg),
                    BorderColor::all(theme.palette.panel_border.color()),
                ));
                if let Some(img) = slot_art {
                    slot_e.insert(img);
                }
                slot_e
                .with_children(|sl| {
                    sl.spawn(col_node()).with_children(|txt| {
                        let (lt, lf, lc) = font_text(font, &slot.label, t.font_label, fg);
                        txt.spawn((lt, lf, lc));
                        let (ct, cf, cc) = font_text(
                            font,
                            &slot.tooltip,
                            t.font_caption,
                            theme.palette.text_secondary.color(),
                        );
                        txt.spawn((ct, cf, cc));
                    });
                    if !badge.is_empty() {
                        let (bt, bf, bc) =
                            font_text(font, &badge, t.font_badge, theme.palette.accent.color());
                        sl.spawn((bt, bf, bc, Node {
                            position_type: PositionType::Absolute,
                            top: Val::Px(s.badge_top),
                            right: Val::Px(s.badge_right),
                            ..default()
                        }));
                    }
                });
            }
        });
    });
}

// ---------------------------------------------------------------------------
// Artifact aura rings (领取后全队光环标记): a flat gold torus at every
// Defender tower's base while the artifact is claimed. 3D pass — guarded on
// the mesh/material stores (headless test apps carry neither).
// ---------------------------------------------------------------------------

fn refresh_aura_rings(world: &mut World) {
    if world.get_resource::<crate::systems::artifact::ClaimedArtifact>().is_none() {
        return;
    }
    if !world.contains_resource::<Assets<Mesh>>()
        || !world.contains_resource::<Assets<StandardMaterial>>()
    {
        return; // headless: no render stores, no rings (the plaque still shows it)
    }
    let mut targets: Vec<Entity> = Vec::new();
    {
        let mut q = world.query_filtered::<(Entity, &Allegiance), With<UnitInstance>>();
        for (e, side) in q.iter(world) {
            if *side == Allegiance::Defender && world.get::<HasAuraRing>(e).is_none() {
                targets.push(e);
            }
        }
    }
    if targets.is_empty() {
        return;
    }
    let mesh = world.resource_mut::<Assets<Mesh>>().add(Torus::new(0.66, 0.08));
    let material = world
        .resource_mut::<Assets<StandardMaterial>>()
        .add(StandardMaterial {
            base_color: Color::srgb(1.0, 0.8, 0.28),
            emissive: LinearRgba::rgb(0.85, 0.62, 0.12),
            ..default()
        });
    info!("[loop-td] artifact aura: marking {} towers", targets.len());
    for e in targets {
        let Ok(mut em) = world.get_entity_mut(e) else { continue };
        em.insert(HasAuraRing);
        em.with_children(|c| {
            c.spawn((
                AuraRing,
                Mesh3d(mesh.clone()),
                MeshMaterial3d(material.clone()),
                // Local transform: flat gold ring at the tower's base (the
                // tower keeps its own placement transform untouched).
                Transform::from_rotation(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2))
                    .with_translation(Vec3::new(0.0, 0.1, 0.0)),
            ));
        });
    }
}

// ---------------------------------------------------------------------------
// Tests: R1 layout + the affix vocabulary (panel/data tests live in
// tests/lt2_ui.rs, which runs the full game composition).
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Every panel rect fits the canvas and stays out of the central 50%x50%
    /// battlefield core (R1 — the loop ring must never sit under a panel).
    #[test]
    fn all_panel_rects_fit_canvas_and_clear_the_core() {
        for (name, r) in [
            ("res", RES_RECT),
            ("artifact", ARTIFACT_RECT),
            ("dig", DIG_RECT),
            ("wave", WAVE_RECT),
            ("challenge", CHALLENGE_RECT),
            ("minimap", MINIMAP_RECT),
            ("bonds", BONDS_RECT),
            ("hero", HERO_RECT),
            ("equip", EQUIP_RECT),
            ("command", COMMAND_RECT),
        ] {
            assert!(
                r.x >= 0.0 && r.y >= 0.0 && r.x + r.w <= CANVAS.0 && r.y + r.h <= CANVAS.1,
                "{name} off-canvas: {r:?}"
            );
            let overlap_x = r.x < CANVAS.0 * 0.75 && r.x + r.w > CANVAS.0 * 0.25;
            let overlap_y = r.y < CANVAS.1 * 0.75 && r.y + r.h > CANVAS.1 * 0.25;
            assert!(!(overlap_x && overlap_y), "{name} intrudes into the center battlefield: {r:?}");
        }
    }

    /// The affix vocabulary renders the proposal's exact forms.
    #[test]
    fn affix_labels_use_the_proposal_vocabulary() {
        let mk = |passive: Option<PassiveStat>| ItemDef {
            id: "x".into(),
            name: String::new(),
            kind: ItemKind::Passive,
            charges: None,
            cooldown: 0.0,
            effect: None,
            cost_gold: 0,
            passive,
        };
        let blade = mk(Some(PassiveStat { kind: StatKind::DamageMult, amount: 0.12 }));
        let quiver = mk(Some(PassiveStat { kind: StatKind::AttackIntervalMult, amount: -0.10 }));
        // SEAM3b: the scope's range affix rides StatKind::RangeMult (the
        // retired game-side +1.5 flat = +30% on the 5.0 base range).
        let scope = mk(Some(PassiveStat { kind: StatKind::RangeMult, amount: 0.30 }));
        assert_eq!(affix_label("blade12", &blade), "攻+12%");
        assert_eq!(affix_label("quiver", &quiver), "间-10%");
        assert_eq!(affix_label("scope", &scope), "射程+30%");
    }

    /// Hotkey badges strip the KeyCode prefixes (Digit1 -> "1", KeyQ -> "Q").
    #[test]
    fn hotkey_badges_are_single_chars() {
        assert_eq!(hotkey_badge(&Hotkey::Key(KeyCode::Digit1)), "1");
        assert_eq!(hotkey_badge(&Hotkey::Key(KeyCode::KeyQ)), "Q");
        assert_eq!(hotkey_badge(&Hotkey::None), "");
    }
}
