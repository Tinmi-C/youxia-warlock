//! UI-DSGN mock mode (`--ui-mock A|B|C`): a frame-scripted scene that stages
//! the full loop-td v2 HUD suite in one of three design proposals (see
//! engine/warbase/docs/ui-design-brief.md §7) and screenshots each panel for
//! the H-UI human gate. Runs the REAL game (map, towers, combat pipeline) —
//! only the HUD is mock: panels are built from the same `UiTheme` tokens the
//! warbase command card now reads, so what gets shot is what would ship.
//!
//! Frame script (60fps): build-mode panorama -> wave starts + hero selected
//! -> per-panel solo close-ups -> tooltip demo -> self-exit. Screenshots land
//! in <repo>/ui-proposals/plan-{A,B,C}/ regardless of the working directory.
//!
//! Everything here is design-proposal tooling, not shipping gameplay; normal
//! runs never add this plugin.

use bevy::prelude::*;
use bevy::render::view::screenshot::{save_to_disk, Screenshot};
use warbase::prelude::*;
use warbase::ui::{
    assign_grid_hotkeys, CommandAction, CommandCardDef, CommandCards, CommandSlotDef, Hotkey,
    Selection, SelectionMode, ThemeColor, UiTheme, BUILD_CARD,
};

use crate::systems::gameplay::{try_place, BuildCursor};

// ---------------------------------------------------------------------------
// Plan specs: one struct = one complete design proposal (theme + layout).
// Layout coordinates are absolute px on the 1280x720 design canvas.
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

#[derive(Clone, Debug, Resource)]
pub struct PlanSpec {
    pub id: char,
    pub theme: UiTheme,
    pub res: Rect,
    pub wave: Rect,
    pub hero: Rect,
    pub hero_compact: bool,
    pub cmd: Rect,
    pub skill_size: f32,
    pub slot_size: (f32, f32),
    pub cmd_cols: usize,
    pub item: Rect,
    pub item_size: f32,
    pub item_cols: usize,
    pub bonds: Rect,
    pub minimap: Rect,
    pub tooltip: Rect,
}

/// Design canvas: the mock lays out in absolute px on the 1280x720 window.
#[cfg_attr(not(test), allow(dead_code))]
const CANVAS: (f32, f32) = (1280.0, 720.0);

/// Plan A "Iron & Gold" — the classic WC3 console: one chunky bottom
/// console (portrait | commands | items), resource plaque TL, minimap TR.
/// Dark iron surfaces, gold accent, 2px borders, near-square corners.
fn plan_a() -> PlanSpec {
    let mut theme = UiTheme::default();
    theme.revision = 1;
    theme.palette = warbase::ui::UiPalette {
        panel_bg: ThemeColor::rgba(0.10, 0.09, 0.08, 0.94),
        panel_raised: ThemeColor::rgba(0.18, 0.16, 0.13, 1.0),
        panel_border: ThemeColor::rgb(0.52, 0.40, 0.18),
        text_primary: ThemeColor::rgb(0.98, 0.95, 0.86),
        text_secondary: ThemeColor::rgb(0.76, 0.70, 0.58),
        text_dim: ThemeColor::rgb(0.46, 0.42, 0.36),
        accent: ThemeColor::rgb(1.0, 0.80, 0.28),
    };
    theme.state_colors.slot_disabled_bg = ThemeColor::rgba(0.13, 0.12, 0.11, 1.0);
    theme.state_colors.tooltip_bg = ThemeColor::rgba(0.07, 0.06, 0.05, 0.96);
    theme.typography = warbase::ui::UiTypography {
        font_value: 22.0,
        font_label: 15.0,
        font_caption: 11.0,
        font_badge: 10.0,
        font_tooltip: 13.0,
        font_title: 16.0,
    };
    theme.spacing = warbase::ui::UiSpacing {
        panel_margin: 6.0,
        panel_gap: 5.0,
        slot_w: 78.0,
        slot_h: 38.0,
        slot_pad: 4.0,
        panel_border_w: 2.0,
        slot_border_w: 1.0,
        tooltip_bottom: 0.0,
        tooltip_left: 0.0,
        tooltip_pad: 7.0,
        badge_top: 1.0,
        badge_right: 3.0,
        bar_h: 12.0,
        icon_size: 16.0,
    };
    theme.radius = warbase::ui::UiRadius { panel: 3.0, slot: 2.0, pill: 999.0 };
    PlanSpec {
        id: 'A',
        theme,
        res: Rect { x: 8.0, y: 8.0, w: 196.0, h: 118.0 },
        wave: Rect { x: 540.0, y: 8.0, w: 200.0, h: 32.0 },
        hero: Rect { x: 8.0, y: 572.0, w: 210.0, h: 140.0 },
        hero_compact: false,
        cmd: Rect { x: 226.0, y: 572.0, w: 382.0, h: 140.0 },
        skill_size: 48.0,
        slot_size: (78.0, 38.0),
        cmd_cols: 4,
        item: Rect { x: 616.0, y: 572.0, w: 180.0, h: 140.0 },
        item_size: 50.0,
        item_cols: 3,
        bonds: Rect { x: 968.0, y: 408.0, w: 304.0, h: 120.0 },
        minimap: Rect { x: 1096.0, y: 8.0, w: 176.0, h: 176.0 },
        tooltip: Rect { x: 226.0, y: 482.0, w: 272.0, h: 80.0 },
    }
}

/// Plan B "Halo Deck" — floating translucent islands: hero card TL, resource
/// pill top-center, detached command island bottom-center, minimap TR.
/// Cool slate glass, one cyan accent, 12px radius, 1px hairline borders.
fn plan_b() -> PlanSpec {
    let mut theme = UiTheme::default();
    theme.revision = 1;
    theme.palette = warbase::ui::UiPalette {
        panel_bg: ThemeColor::rgba(0.09, 0.12, 0.15, 0.80),
        panel_raised: ThemeColor::rgba(0.16, 0.20, 0.25, 0.90),
        panel_border: ThemeColor::rgba(0.34, 0.45, 0.55, 0.95),
        text_primary: ThemeColor::rgb(0.94, 0.97, 1.0),
        text_secondary: ThemeColor::rgb(0.60, 0.69, 0.78),
        text_dim: ThemeColor::rgb(0.40, 0.46, 0.52),
        accent: ThemeColor::rgb(0.32, 0.86, 0.96),
    };
    theme.state_colors.slot_disabled_bg = ThemeColor::rgba(0.12, 0.14, 0.16, 0.75);
    theme.state_colors.tooltip_bg = ThemeColor::rgba(0.05, 0.07, 0.09, 0.95);
    theme.typography = warbase::ui::UiTypography {
        font_value: 19.0,
        font_label: 12.0,
        font_caption: 10.0,
        font_badge: 9.0,
        font_tooltip: 12.0,
        font_title: 14.0,
    };
    theme.spacing = warbase::ui::UiSpacing {
        panel_margin: 10.0,
        panel_gap: 4.0,
        slot_w: 78.0,
        slot_h: 48.0,
        slot_pad: 4.0,
        panel_border_w: 1.0,
        slot_border_w: 1.0,
        tooltip_bottom: 0.0,
        tooltip_left: 0.0,
        tooltip_pad: 8.0,
        badge_top: 2.0,
        badge_right: 4.0,
        bar_h: 8.0,
        icon_size: 13.0,
    };
    theme.radius = warbase::ui::UiRadius { panel: 12.0, slot: 10.0, pill: 999.0 };
    PlanSpec {
        id: 'B',
        theme,
        res: Rect { x: 452.0, y: 10.0, w: 376.0, h: 42.0 },
        wave: Rect { x: 552.0, y: 58.0, w: 176.0, h: 26.0 },
        hero: Rect { x: 10.0, y: 10.0, w: 232.0, h: 160.0 },
        hero_compact: false,
        cmd: Rect { x: 468.0, y: 584.0, w: 344.0, h: 126.0 },
        skill_size: 46.0,
        slot_size: (78.0, 48.0),
        cmd_cols: 4,
        item: Rect { x: 832.0, y: 594.0, w: 168.0, h: 108.0 },
        item_size: 46.0,
        item_cols: 3,
        bonds: Rect { x: 10.0, y: 316.0, w: 284.0, h: 132.0 },
        minimap: Rect { x: 1102.0, y: 10.0, w: 168.0, h: 168.0 },
        tooltip: Rect { x: 530.0, y: 476.0, w: 256.0, h: 78.0 },
    }
}

/// Plan C "Whisper UI" — immersion first: UI retreats to hairline clusters at
/// the edges. Micro resource cluster TR, slim hero strip top-center, one
/// compact command row bottom-center, tiny ring-minimap. Warm umber, no
/// borders, ~55% alpha, amber glow accent.
fn plan_c() -> PlanSpec {
    let mut theme = UiTheme::default();
    theme.revision = 1;
    theme.palette = warbase::ui::UiPalette {
        panel_bg: ThemeColor::rgba(0.09, 0.07, 0.05, 0.55),
        panel_raised: ThemeColor::rgba(0.14, 0.11, 0.08, 0.65),
        panel_border: ThemeColor::rgba(0.35, 0.28, 0.20, 0.9),
        text_primary: ThemeColor::rgb(0.96, 0.91, 0.82),
        text_secondary: ThemeColor::rgb(0.68, 0.61, 0.50),
        text_dim: ThemeColor::rgb(0.42, 0.38, 0.32),
        accent: ThemeColor::rgb(0.98, 0.68, 0.30),
    };
    theme.state_colors.slot_disabled_bg = ThemeColor::rgba(0.10, 0.09, 0.07, 0.6);
    theme.state_colors.tooltip_bg = ThemeColor::rgba(0.06, 0.05, 0.04, 0.92);
    theme.typography = warbase::ui::UiTypography {
        font_value: 16.0,
        font_label: 11.0,
        font_caption: 9.0,
        font_badge: 8.0,
        font_tooltip: 11.0,
        font_title: 13.0,
    };
    theme.spacing = warbase::ui::UiSpacing {
        panel_margin: 4.0,
        panel_gap: 3.0,
        slot_w: 62.0,
        slot_h: 36.0,
        slot_pad: 3.0,
        panel_border_w: 0.0,
        slot_border_w: 0.0,
        tooltip_bottom: 0.0,
        tooltip_left: 0.0,
        tooltip_pad: 6.0,
        badge_top: 1.0,
        badge_right: 3.0,
        bar_h: 6.0,
        icon_size: 11.0,
    };
    theme.radius = warbase::ui::UiRadius { panel: 4.0, slot: 4.0, pill: 999.0 };
    PlanSpec {
        id: 'C',
        theme,
        res: Rect { x: 1082.0, y: 8.0, w: 190.0, h: 52.0 },
        wave: Rect { x: 544.0, y: 8.0, w: 192.0, h: 18.0 },
        hero: Rect { x: 460.0, y: 32.0, w: 360.0, h: 28.0 },
        hero_compact: true,
        cmd: Rect { x: 372.0, y: 626.0, w: 536.0, h: 88.0 },
        skill_size: 34.0,
        slot_size: (62.0, 36.0),
        cmd_cols: 8,
        item: Rect { x: 1082.0, y: 676.0, w: 190.0, h: 36.0 },
        item_size: 26.0,
        item_cols: 6,
        bonds: Rect { x: 1016.0, y: 288.0, w: 256.0, h: 84.0 },
        minimap: Rect { x: 1176.0, y: 68.0, w: 96.0, h: 96.0 },
        tooltip: Rect { x: 372.0, y: 552.0, w: 248.0, h: 66.0 },
    }
}

pub fn plan_spec(id: char) -> PlanSpec {
    match id {
        'B' => plan_b(),
        'C' => plan_c(),
        _ => plan_a(),
    }
}

// ---------------------------------------------------------------------------
// Plugin + frame script
// ---------------------------------------------------------------------------

pub struct UiMockPlugin {
    pub plan: char,
}

/// Which mock panel group a root node is (solo close-ups toggle these).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PanelGroup {
    ResourceBar,
    WaveBanner,
    HeroPanel,
    CommandCard,
    ItemBar,
    Bonds,
    Minimap,
    TooltipDemo,
}

#[derive(Component, Debug)]
pub struct MockPanel(pub PanelGroup);

/// The command card's two sub-rows (shot 04 vs 05 toggle these).
#[derive(Component, Debug)]
pub struct MockCardGrid;
#[derive(Component, Debug)]
pub struct MockSkillRow;

/// Live-refresh markers.
#[derive(Component, Debug)] pub struct MockGoldText;
#[derive(Component, Debug)] pub struct MockWaveText;
#[derive(Component, Debug)] pub struct MockStockFill;
#[derive(Component, Debug)] pub struct MockHeroHpFill;
#[derive(Component, Debug)] pub struct MockEnemyDot;

#[derive(Resource, Debug)]
pub struct MockFrame(pub u64);

#[derive(Resource, Debug)]
pub struct MockHero(pub Entity);

#[derive(Resource, Debug, Default, Clone)]
struct HudCache {
    gold: u32,
    wave_label: String,
    hero_hp: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ShotMode {
    /// Full panorama, all panels.
    All,
    /// One panel group solo (others hidden).
    Solo(PanelGroup),
    /// Command card grid without the skill row.
    GridOnly,
    /// Skill row without the grid.
    SkillsOnly,
}

fn shots() -> Vec<(u64, &'static str, ShotMode)> {
    use PanelGroup::*;
    vec![
        (36, "01-panorama-build", ShotMode::All),
        (108, "02-panorama-combat", ShotMode::All),
        (132, "03-resource-bar", ShotMode::Solo(ResourceBar)),
        (148, "04-command-card", ShotMode::GridOnly),
        (164, "05-skill-bar", ShotMode::SkillsOnly),
        (180, "06-item-bar", ShotMode::Solo(ItemBar)),
        (196, "07-hero-panel", ShotMode::Solo(HeroPanel)),
        (212, "08-bond-hints", ShotMode::Solo(Bonds)),
        (228, "09-minimap", ShotMode::Solo(Minimap)),
        (244, "10-tooltip", ShotMode::Solo(TooltipDemo)),
    ]
}

impl Plugin for UiMockPlugin {
    fn build(&self, app: &mut App) {
        let spec = plan_spec(self.plan);
        // hud::refresh_hud requires SelectedTower but the game graph no
        // longer inserts it (pre-existing gap, harmless headless, panics on
        // the real path). The mock provides it so the windowed app boots.
        app.insert_resource(crate::systems::gameplay::SelectedTower(String::new()))
            .insert_resource(spec.theme.clone())
            .insert_resource(spec)
            .insert_resource(MockFrame(0))
            .add_systems(
                Startup,
                (mock_scene, build_mock_hud)
                    .after(crate::systems::gameplay::load_map)
                    .chain(),
            )
            .add_systems(Update, (refresh_mock_hud, mock_frame_driver).chain());
    }
}

fn out_dir(plan: char) -> String {
    // <crate>/../../ui-proposals/plan-X — the worktree root no matter the
    // directory cargo run was started from.
    format!("{}/../../ui-proposals/plan-{plan}", env!("CARGO_MANIFEST_DIR"))
}

// ---------------------------------------------------------------------------
// Scene staging (real game state, authored numbers)
// ---------------------------------------------------------------------------

/// Stage the game: econ/pop, towers around the ring, the hero, wave director
/// parked in intermission ("WAVE 8 in 12s"), plus the unit command card.
fn mock_scene(world: &mut World) {
    // Econ: plenty of gold for placement; display value set after.
    world.resource_mut::<Resources>().gold = 20_000;
    world.resource_mut::<Population>().cap = 40;
    world.resource_mut::<StockCount>().0 = 38;

    // Director parked mid-intermission before wave 8 (0-based 7).
    {
        let mut d = world.resource_mut::<WaveDirector>();
        d.next_wave = 7;
        d.phase = DirectorPhase::Intermission;
        d.phase_timer = 12.0;
    }

    // Towers around the ring (the loop runs (-10,-6)->(10,-6)->(10,6)->(-10,6)).
    // Grid cells are world + (cols/2, rows/2): loop.ron is 28x18 => +(14, 9).
    let towers: &[(&str, i32, i32)] = &[
        ("arrow", 8, 6), ("arrow", 9, 4), ("cannon", 8, -7), ("cannon", 10, -2),
        ("mage", 6, 7), ("mage", 4, -8), ("sentry", 12, 5), ("sentry", 0, 8),
        ("ice", 11, 0), ("drum", 2, 8), ("corrode", -9, -8), ("arrow", -11, 3),
        ("hunter", 3, 4),
    ];
    for (def, wx, wy) in towers {
        if let Err(e) = try_place(world, def, Cell(wx + 14, wy + 9)) {
            info!("[ui-mock] place {def} at world ({wx},{wy}) failed: {e}");
        }
    }
    // Display econ + cursor for the build-mode shot (cell 20,5 = world 6.5,-3.5).
    world.resource_mut::<Resources>().gold = 347;
    world.resource_mut::<Population>().cap = 20;
    world.resource_mut::<BuildCursor>().0 = Cell(20, 5);

    // Hero: level 7, damaged (so bars read), mana for the mock panel.
    let hero = world
        .query_filtered::<Entity, With<XpHolder>>()
        .iter(world)
        .next()
        .expect("hunter placed");
    if let Some(mut xp) = world.get_mut::<XpHolder>(hero) {
        xp.level = 7;
        xp.talents_taken = 1;
        xp.xp = 2140;
        xp.next_level_xp = 5300;
    }
    if let Some(mut hp) = world.get_mut::<Hp>(hero) {
        hp.0 = 96.0;
    }
    let _ = world.entity_mut(hero).insert((Mana(38.0), MaxMana(60.0)));
    world.insert_resource(MockHero(hero));

    // Unit command card (the face shown once the hero is selected).
    let mut card = CommandCardDef {
        slots: vec![
            mock_slot("Snipe", "autocast / 60 dmg", CommandAction::Cast(0), true),
            mock_slot("Frost", "Frost Arrow / mana 40 / cd 6s", CommandAction::Cast(1), false),
            mock_slot("Upgrade", "90g / tier up", CommandAction::Upgrade, true),
            mock_slot("Sell", "refund 70%", CommandAction::Sell, true),
        ],
    };
    assign_grid_hotkeys(&mut card);
    world
        .resource_mut::<CommandCards>()
        .cards
        .insert("hunter".to_string(), card);

    // Selection starts in build mode (the build-menu card face).
    world.insert_resource(Selection { mode: SelectionMode::Build });
}

fn mock_slot(label: &str, sub: &str, action: CommandAction, enabled: bool) -> CommandSlotDef {
    CommandSlotDef {
        label: label.into(),
        hotkey: Hotkey::None,
        tooltip: sub.into(),
        action,
        enabled,
        badge: None,
    }
}

// ---------------------------------------------------------------------------
// Mock HUD construction (pure bevy_ui, themed by UiTheme, laid out by spec)
// ---------------------------------------------------------------------------

fn build_mock_hud(world: &mut World) {
    let spec = world.resource::<PlanSpec>().clone();
    let theme = world.resource::<UiTheme>().clone();

    resource_panel(world, &spec, &theme);
    wave_banner(world, &spec, &theme);
    hero_panel(world, &spec, &theme);
    command_block(world, &spec, &theme);
    item_bar(world, &spec, &theme);
    bond_hints(world, &spec, &theme);
    minimap_panel(world, &spec, &theme);
    tooltip_demo(world, &spec, &theme);
}

/// Absolute-positioned panel root bundle.
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

fn text_node(s: &str, size: f32, color: Color) -> (Text, TextFont, TextColor) {
    (Text::new(s), TextFont { font_size: FontSize::Px(size), ..default() }, TextColor(color))
}

/// A progress bar built through a ChildBuilder; the fill carries marker M so
/// live refresh can resize it (`fill frac` 0..1).
fn bar<M: Component>(
    p: &mut ChildSpawner,
    theme: &UiTheme,
    w: f32,
    frac: f32,
    fill: Color,
    marker: M,
) {
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
            marker,
            Node {
                width: Val::Percent((frac * 100.0).clamp(0.0, 100.0)),
                height: Val::Percent(100.0),
                border_radius: BorderRadius::all(Val::Px(h / 2.0)),
                ..default()
            },
            BackgroundColor(fill),
        ));
        // 90% red-line tick inside the trough (the leak-pressure threshold).
        trough.spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Percent(90.0),
                top: Val::Px(0.0),
                width: Val::Px(2.0),
                height: Val::Percent(100.0),
                ..default()
            },
            BackgroundColor(Color::srgb(1.0, 0.25, 0.15)),
        ));
    });
}

/// Geometric icon glyph: circle / square / pill (R2 — no bare numbers).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Glyph {
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

/// One resource readout: glyph + value + tiny label in a row.
fn resource_row(
    p: &mut ChildSpawner,
    theme: &UiTheme,
    glyph: Glyph,
    color: Color,
    value: &str,
    label: &str,
    value_size: f32,
    gold_marker: bool,
) {
    p.spawn(Node {
        display: Display::Flex,
        flex_direction: FlexDirection::Row,
        align_items: AlignItems::Center,
        column_gap: Val::Px(theme.spacing.panel_gap + 2.0),
        ..default()
    })
    .with_children(|row| {
        row.spawn(icon_node(color, theme.spacing.icon_size, glyph));
        let (t, f, c) = text_node(value, value_size, theme.palette.text_primary.color());
        if gold_marker {
            row.spawn((MockGoldText, t, f, c));
        } else {
            row.spawn((t, f, c));
        }
        row.spawn(text_node(
            label,
            theme.typography.font_caption,
            theme.palette.text_secondary.color(),
        ));
    });
}

fn resource_panel(world: &mut World, spec: &PlanSpec, theme: &UiTheme) {
    let mut e = world.spawn((MockPanel(PanelGroup::ResourceBar), panel_node(theme, spec.res)));
    let t = &theme.typography;
    let sc = &theme.state_colors;
    let compact = spec.res.h < 70.0;
    e.with_children(|p| {
        if compact {
            // Plan C: one inline row + a micro stock bar.
            p.spawn(Node {
                display: Display::Flex,
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: Val::Px(10.0),
                ..default()
            })
            .with_children(|row| {
                resource_row(&mut *row, theme, Glyph::Circle, sc.gold.color(), "347", "", t.font_value, true);
                resource_row(&mut *row, theme, Glyph::Square, sc.wood.color(), "58", "", t.font_value, false);
                resource_row(&mut *row, theme, Glyph::Pill, theme.palette.accent.color(), "13/20", "", t.font_value, false);
            });
        } else {
            resource_row(p, theme, Glyph::Circle, sc.gold.color(), "347", "GOLD", t.font_value, true);
            resource_row(p, theme, Glyph::Square, sc.wood.color(), "58", "WOOD", t.font_value, false);
            resource_row(p, theme, Glyph::Pill, theme.palette.accent.color(), "13/20", "POP", t.font_value, false);
        }
        // Stock meter: the leak-pressure read (R3/R4 — fills warning-yellow
        // past 70%, red past the 90% tick).
        let inner = spec.res.w - 2.0 * (theme.spacing.panel_gap + theme.spacing.panel_border_w);
        p.spawn(Node {
            display: Display::Flex,
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: Val::Px(theme.spacing.panel_gap + 2.0),
            ..default()
        })
        .with_children(|row| {
            if !compact {
                row.spawn(text_node(
                    "STOCK",
                    t.font_caption,
                    theme.palette.text_secondary.color(),
                ));
            }
            bar(&mut *row, theme, inner * 0.72, 38.0 / 50.0, sc.warning.color(), MockStockFill);
            row.spawn(text_node("38/50", t.font_caption, theme.palette.text_secondary.color()));
        });
    });
}

fn wave_banner(world: &mut World, spec: &PlanSpec, theme: &UiTheme) {
    let mut e = world.spawn((
        MockPanel(PanelGroup::WaveBanner),
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(spec.wave.x),
            top: Val::Px(spec.wave.y),
            width: Val::Px(spec.wave.w),
            height: Val::Px(spec.wave.h),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            border: UiRect::all(Val::Px(theme.spacing.panel_border_w)),
            border_radius: BorderRadius::all(Val::Px(theme.radius.pill.min(32.0))),
            ..default()
        },
        BackgroundColor(theme.palette.panel_bg.color()),
        BorderColor::all(theme.palette.panel_border.color()),
    ));
    e.with_children(|p| {
        let (t, f, c) = text_node(
            "WAVE 8 IN 12s",
            theme.typography.font_title,
            theme.state_colors.warning.color(),
        );
        p.spawn((MockWaveText, t, f, c));
    });
}

fn hero_panel(world: &mut World, spec: &PlanSpec, theme: &UiTheme) {
    let mut e = world.spawn((MockPanel(PanelGroup::HeroPanel), panel_node(theme, spec.hero)));
    let t = &theme.typography;
    let sc = &theme.state_colors;
    if spec.hero_compact {
        // Plan C slim strip: LV badge + name + three micro bars inline.
        e.with_children(|p| {
            p.spawn(Node {
                display: Display::Flex,
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: Val::Px(theme.spacing.panel_gap + 2.0),
                ..default()
            })
            .with_children(|row| {
                row.spawn(text_node("LV7", t.font_caption, theme.palette.accent.color()));
                row.spawn(text_node("HUNTER", t.font_label, theme.palette.text_primary.color()));
                bar(&mut *row, theme, 92.0, 96.0 / 150.0, sc.health.color(), MockHeroHpFill);
                bar(&mut *row, theme, 64.0, 38.0 / 60.0, sc.mana.color(), MockHeroHpFill);
                bar(&mut *row, theme, 64.0, 2140.0 / 5300.0, sc.xp.color(), MockHeroHpFill);
            });
        });
        return;
    }
    e.with_children(|p| {
        // Portrait + name/level head.
        p.spawn(Node {
            display: Display::Flex,
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: Val::Px(theme.spacing.panel_gap + 2.0),
            ..default()
        })
        .with_children(|head| {
            head.spawn((
                Node {
                    width: Val::Px(spec.hero.h * 0.38),
                    height: Val::Px(spec.hero.h * 0.38),
                    border: UiRect::all(Val::Px(theme.spacing.slot_border_w.max(1.0))),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::FlexEnd,
                    padding: UiRect::all(Val::Px(2.0)),
                    border_radius: BorderRadius::all(Val::Px(theme.radius.slot)),
                    ..default()
                },
                BackgroundColor(theme.palette.panel_raised.color()),
                BorderColor::all(theme.palette.accent.color()),
            ))
            .with_children(|port| {
                port.spawn(icon_node(
                    theme.palette.accent.color().with_alpha(0.55),
                    spec.hero.h * 0.16,
                    Glyph::Circle,
                ));
                port.spawn(text_node("LV7", t.font_caption, theme.palette.accent.color()));
            });
            head.spawn(Node {
                display: Display::Flex,
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(2.0),
                ..default()
            })
            .with_children(|name| {
                name.spawn(text_node("HUNTER", t.font_title, theme.palette.text_primary.color()));
                name.spawn(text_node(
                    "hero tower - ranged",
                    t.font_caption,
                    theme.palette.text_secondary.color(),
                ));
            });
        });
        // HP / MP / XP bars with inline captions.
        let inner = spec.hero.w - 2.0 * (theme.spacing.panel_gap + theme.spacing.panel_border_w);
        p.spawn(Node {
            display: Display::Flex,
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: Val::Px(theme.spacing.panel_gap),
            ..default()
        })
        .with_children(|row| {
            row.spawn(text_node("HP", t.font_caption, theme.palette.text_secondary.color()));
            bar(&mut *row, theme, inner * 0.58, 96.0 / 150.0, sc.health.color(), MockHeroHpFill);
            row.spawn(text_node("96/150", t.font_caption, theme.palette.text_secondary.color()));
        });
        p.spawn(Node {
            display: Display::Flex,
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: Val::Px(theme.spacing.panel_gap),
            ..default()
        })
        .with_children(|row| {
            row.spawn(text_node("MP", t.font_caption, theme.palette.text_secondary.color()));
            bar(&mut *row, theme, inner * 0.58, 38.0 / 60.0, sc.mana.color(), MockHeroHpFill);
            row.spawn(text_node("38/60", t.font_caption, theme.palette.text_secondary.color()));
        });
        p.spawn(Node {
            display: Display::Flex,
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: Val::Px(theme.spacing.panel_gap),
            ..default()
        })
        .with_children(|row| {
            row.spawn(text_node("XP", t.font_caption, theme.palette.text_secondary.color()));
            bar(&mut *row, theme, inner * 0.58, 2140.0 / 5300.0, sc.xp.color(), MockHeroHpFill);
            row.spawn(text_node("2140/5300", t.font_caption, theme.palette.text_secondary.color()));
        });
        // Attribute placeholders (real STR/AGI/INT land with the HERO line).
        p.spawn(text_node(
            "STR 12 / AGI 9 / INT 7",
            t.font_caption,
            theme.palette.text_secondary.color(),
        ));
    });
}

/// Mock skill row data: cooldown veil + autocast dot + lock state.
struct MockSkill {
    name: &'static str,
    cd_pct: f32,
    cd_secs: &'static str,
    autocast: bool,
    locked: bool,
}

fn skill_row(p: &mut ChildSpawner, theme: &UiTheme, spec: &PlanSpec) {
    let skills = [
        MockSkill { name: "Snipe", cd_pct: 0.0, cd_secs: "", autocast: true, locked: false },
        MockSkill { name: "Frost", cd_pct: 0.62, cd_secs: "4.2", autocast: false, locked: false },
        MockSkill { name: "Mend", cd_pct: 0.0, cd_secs: "", autocast: false, locked: false },
        MockSkill { name: "Storm", cd_pct: 0.0, cd_secs: "", autocast: false, locked: true },
    ];
    let keys = ["Q", "W", "E", "R"];
    let t = &theme.typography;
    let s = spec.skill_size;
    p.spawn((
        MockSkillRow,
        Node {
            display: Display::Flex,
            flex_direction: FlexDirection::Row,
            column_gap: Val::Px(theme.spacing.panel_gap + 2.0),
            ..default()
        },
    ))
    .with_children(|row| {
        for (i, sk) in skills.iter().enumerate() {
            let border = if sk.autocast {
                theme.palette.accent.color()
            } else if sk.locked {
                theme.palette.text_dim.color()
            } else {
                theme.palette.panel_border.color()
            };
            row.spawn((
                Node {
                    width: Val::Px(s),
                    height: Val::Px(s),
                    border: UiRect::all(Val::Px(theme.spacing.slot_border_w.max(1.0))),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    border_radius: BorderRadius::all(Val::Px(theme.radius.slot)),
                    ..default()
                },
                BackgroundColor(if sk.locked {
                    theme.state_colors.slot_disabled_bg.color()
                } else {
                    theme.palette.panel_raised.color()
                }),
                BorderColor::all(border),
            ))
            .with_children(|slot| {
                let fg = if sk.locked || sk.cd_pct > 0.0 {
                    theme.palette.text_secondary.color()
                } else {
                    theme.palette.text_primary.color()
                };
                let (st, sf, sc) = text_node(sk.name, t.font_caption.max(8.0), fg);
                slot.spawn((st, sf, sc));
                // Hotkey badge (top-right corner).
                let (bt, bf, bc) = text_node(keys[i], t.font_badge, theme.palette.accent.color());
                slot.spawn((bt, bf, bc, Node {
                    position_type: PositionType::Absolute,
                    top: Val::Px(theme.spacing.badge_top),
                    right: Val::Px(theme.spacing.badge_right),
                    ..default()
                }));
                // Autocast dot (bottom-left).
                if sk.autocast {
                    slot.spawn((
                        Node {
                            position_type: PositionType::Absolute,
                            bottom: Val::Px(2.0),
                            left: Val::Px(2.0),
                            width: Val::Px(4.0),
                            height: Val::Px(4.0),
                            border_radius: BorderRadius::all(Val::Percent(50.0)),
                            ..default()
                        },
                        BackgroundColor(theme.palette.accent.color()),
                    ));
                }
                // Cooldown veil from the top + centered seconds overlay.
                if sk.cd_pct > 0.0 {
                    slot.spawn((
                        Node {
                            position_type: PositionType::Absolute,
                            top: Val::Px(0.0),
                            left: Val::Px(0.0),
                            width: Val::Percent(100.0),
                            height: Val::Percent((sk.cd_pct * 100.0).clamp(0.0, 100.0)),
                            ..default()
                        },
                        BackgroundColor(theme.state_colors.cooldown.color()),
                    ));
                    let (vt, vf, vc) = text_node(
                        sk.cd_secs,
                        t.font_caption.max(8.0),
                        theme.palette.text_primary.color(),
                    );
                    slot.spawn((vt, vf, vc, Node {
                        position_type: PositionType::Absolute,
                        top: Val::Px(0.0),
                        left: Val::Px(0.0),
                        width: Val::Percent(100.0),
                        height: Val::Percent(100.0),
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        ..default()
                    }));
                }
            });
        }
    });
}

/// The command block: skill row + the active card's slot grid. The two rows
/// carry markers so the frame script can solo either one.
fn command_block(world: &mut World, spec: &PlanSpec, theme: &UiTheme) {
    let s = &theme.spacing;
    let card = world_card(world, BUILD_CARD);
    let mut e = world.spawn((
        MockPanel(PanelGroup::CommandCard),
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(spec.cmd.x),
            top: Val::Px(spec.cmd.y),
            width: Val::Px(spec.cmd.w),
            height: Val::Px(spec.cmd.h),
            padding: UiRect::all(Val::Px((s.panel_gap + s.panel_border_w).max(8.0))),
            border: UiRect::all(Val::Px(s.panel_border_w)),
            display: Display::Flex,
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(s.panel_gap),
            justify_content: JustifyContent::SpaceBetween,
            border_radius: BorderRadius::all(Val::Px(theme.radius.panel)),
            ..default()
        },
        BackgroundColor(theme.palette.panel_bg.color()),
        BorderColor::all(theme.palette.panel_border.color()),
    ));
    let id = e.id();
    e.with_children(|p| {
        skill_row(p, theme, spec);
        card_grid(p, theme, spec, &card);
    });
    world.entity_mut(id).insert(MockCardFace(BUILD_CARD.to_string()));
}

/// Marker remembering which card face the grid shows.
#[derive(Component, Debug)]
pub struct MockCardFace(pub String);

fn world_card(world: &World, key: &str) -> CommandCardDef {
    world
        .resource::<CommandCards>()
        .cards
        .get(key)
        .cloned()
        .unwrap_or_default()
}

fn card_grid(p: &mut ChildSpawner, theme: &UiTheme, spec: &PlanSpec, card: &CommandCardDef) {
    p.spawn((
        MockCardGrid,
        Node {
            display: Display::Flex,
            flex_direction: FlexDirection::Row,
            flex_wrap: FlexWrap::Wrap,
            column_gap: Val::Px(theme.spacing.panel_gap),
            row_gap: Val::Px(theme.spacing.panel_gap),
            ..default()
        },
    ))
    .with_children(|grid| {
        for slot in card.slots.iter().take(8) {
            let (bg, fg) = if slot.enabled {
                (theme.palette.panel_raised.color(), theme.palette.text_primary.color())
            } else {
                (theme.state_colors.slot_disabled_bg.color(), theme.palette.text_dim.color())
            };
            grid.spawn((
                Node {
                    width: Val::Px(spec.slot_size.0),
                    height: Val::Px(spec.slot_size.1),
                    padding: UiRect::all(Val::Px(theme.spacing.slot_pad)),
                    border: UiRect::all(Val::Px(theme.spacing.slot_border_w.max(1.0))),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    border_radius: BorderRadius::all(Val::Px(theme.radius.slot)),
                    ..default()
                },
                BackgroundColor(bg),
                BorderColor::all(theme.palette.panel_border.color()),
            ))
            .with_children(|s| {
                let (lt, lf, lc) = text_node(&slot.label, theme.typography.font_label, fg);
                s.spawn((lt, lf, lc));
                if let Some(b) = slot.badge {
                    let (bt, bf, bc) =
                        text_node(&b.to_string(), theme.typography.font_badge, theme.palette.accent.color());
                    s.spawn((bt, bf, bc, Node {
                        position_type: PositionType::Absolute,
                        top: Val::Px(theme.spacing.badge_top),
                        right: Val::Px(theme.spacing.badge_right),
                        ..default()
                    }));
                }
            });
        }
    });
}

/// Rebuild the mock command grid for a card face (build menu <-> unit card).
fn rebuild_card_face(world: &mut World, card_key: &str) {
    let spec = world.resource::<PlanSpec>().clone();
    let theme = world.resource::<UiTheme>().clone();
    let card = world_card(world, card_key);
    let panel = world
        .query_filtered::<Entity, With<MockCardFace>>()
        .single(world)
        .expect("mock command card exists");
    // Drop the old grid subtree (the skill row stays).
    let children: Vec<Entity> = world
        .get::<Children>(panel)
        .map(|c| c.to_vec())
        .unwrap_or_default();
    for c in children {
        if world.get::<MockCardGrid>(c).is_some() {
            despawn_subtree(world, c);
        }
    }
    world.get_entity_mut(panel).unwrap().with_children(|p| {
        card_grid(p, &theme, &spec, &card);
    });
    world.entity_mut(panel).insert(MockCardFace(card_key.to_string()));
}

fn despawn_subtree(world: &mut World, root: Entity) {
    let children: Vec<Entity> = world.get::<Children>(root).map(|c| c.to_vec()).unwrap_or_default();
    for c in children {
        despawn_subtree(world, c);
    }
    let _ = world.entity_mut(root).despawn();
}

struct MockItem {
    glyph: Glyph,
    color: Color,
    count: u8,
}

fn item_bar(world: &mut World, spec: &PlanSpec, theme: &UiTheme) {
    let items: [Option<MockItem>; 6] = [
        Some(MockItem { glyph: Glyph::Circle, color: Color::srgb(0.4, 0.9, 0.95), count: 2 }),
        Some(MockItem { glyph: Glyph::Square, color: Color::srgb(0.75, 0.45, 0.95), count: 1 }),
        None, None, None, None,
    ];
    let mut e = world.spawn((MockPanel(PanelGroup::ItemBar), panel_node(theme, spec.item)));
    e.with_children(|p| {
        p.spawn(Node {
            display: Display::Flex,
            flex_direction: FlexDirection::Row,
            flex_wrap: FlexWrap::Wrap,
            align_content: AlignContent::Center,
            justify_content: JustifyContent::Center,
            column_gap: Val::Px(theme.spacing.panel_gap),
            row_gap: Val::Px(theme.spacing.panel_gap),
            ..default()
        })
        .with_children(|grid| {
            for item in &items {
                let size = spec.item_size;
                let filled = item.is_some();
                grid.spawn((
                    Node {
                        width: Val::Px(size),
                        height: Val::Px(size),
                        border: UiRect::all(Val::Px(1.0)),
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        border_radius: BorderRadius::all(Val::Px(theme.radius.slot)),
                        ..default()
                    },
                    BackgroundColor(if filled {
                        theme.palette.panel_raised.color()
                    } else {
                        theme.palette.panel_bg.color().with_alpha(0.4)
                    }),
                    BorderColor::all(if filled {
                        theme.palette.panel_border.color()
                    } else {
                        theme.palette.text_dim.color().with_alpha(0.5)
                    }),
                ))
                .with_children(|s| {
                    if let Some(it) = item {
                        s.spawn(icon_node(it.color, size * 0.4, it.glyph));
                        if it.count > 1 {
                            let (ct, cf, cc) = text_node(
                                &it.count.to_string(),
                                theme.typography.font_caption,
                                theme.palette.text_primary.color(),
                            );
                            s.spawn((ct, cf, cc, Node {
                                position_type: PositionType::Absolute,
                                bottom: Val::Px(1.0),
                                right: Val::Px(3.0),
                                ..default()
                            }));
                        }
                    }
                });
            }
        });
    });
}

fn bond_hints(world: &mut World, spec: &PlanSpec, theme: &UiTheme) {
    let chips: [(&str, &str, bool); 2] = [
        ("FROST SHELL", "cannon +20% vs slowed", true),
        ("THUNDER DRUM", "2 / 3 towers placed", false),
    ];
    let mut e = world.spawn((MockPanel(PanelGroup::Bonds), panel_node(theme, spec.bonds)));
    e.with_children(|p| {
        for (name, desc, active) in chips {
            let alpha = if active { 1.0 } else { 0.62 };
            p.spawn((
                Node {
                    display: Display::Flex,
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    column_gap: Val::Px(theme.spacing.panel_gap + 2.0),
                    padding: UiRect::all(Val::Px(theme.spacing.panel_gap)),
                    border: UiRect::all(Val::Px(if active { theme.spacing.slot_border_w.max(1.0) } else { 0.0 })),
                    border_radius: BorderRadius::all(Val::Px(theme.radius.pill.min(14.0))),
                    ..default()
                },
                BackgroundColor(if active {
                    theme.palette.panel_raised.color().with_alpha(alpha)
                } else {
                    theme.palette.panel_bg.color().with_alpha(alpha * 0.8)
                }),
                BorderColor::all(if active {
                    theme.palette.accent.color()
                } else {
                    theme.palette.panel_border.color()
                }),
            ))
            .with_children(|chip| {
                chip.spawn(icon_node(
                    if active { theme.palette.accent.color() } else { theme.palette.text_dim.color() },
                    theme.spacing.icon_size * 0.7,
                    Glyph::Square,
                ));
                chip.spawn(Node {
                    display: Display::Flex,
                    flex_direction: FlexDirection::Column,
                    ..default()
                })
                .with_children(|txt| {
                    let name_color = if active {
                        theme.palette.text_primary.color()
                    } else {
                        theme.palette.text_dim.color()
                    };
                    let (nt, nf, nc) = text_node(name, theme.typography.font_label, name_color);
                    txt.spawn((nt, nf, nc));
                    let (dt, df, dc) = text_node(
                        desc,
                        theme.typography.font_caption,
                        theme.palette.text_secondary.color(),
                    );
                    txt.spawn((dt, df, dc));
                });
            });
        }
    });
}

fn minimap_panel(world: &mut World, spec: &PlanSpec, theme: &UiTheme) {
    let mut e = world.spawn((MockPanel(PanelGroup::Minimap), panel_node(theme, spec.minimap)));
    e.with_children(|p| {
        // Ring track (rounded-rect stroke) inset in the panel.
        p.spawn((
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(10.0),
                left: Val::Px(10.0),
                width: Val::Px(spec.minimap.w - 20.0),
                height: Val::Px(spec.minimap.h - 20.0),
                border: UiRect::all(Val::Px(2.0)),
                border_radius: BorderRadius::all(Val::Px(14.0)),
                ..default()
            },
            BackgroundColor(Color::NONE),
            BorderColor::all(theme.palette.panel_border.color()),
        ));
        // Camera viewport frame (accent stroke over the play area).
        p.spawn((
            Node {
                position_type: PositionType::Absolute,
                top: Val::Percent(8.0),
                left: Val::Percent(6.0),
                width: Val::Percent(88.0),
                height: Val::Percent(82.0),
                border: UiRect::all(Val::Px(1.0)),
                border_radius: BorderRadius::all(Val::Px(2.0)),
                ..default()
            },
            BackgroundColor(Color::NONE),
            BorderColor::all(theme.palette.accent.color().with_alpha(0.8)),
        ));
        // Static tower dots (green).
        let towers: [(f32, f32); 8] = [
            (0.30, 0.32), (0.36, 0.28), (0.62, 0.30), (0.70, 0.42),
            (0.66, 0.68), (0.40, 0.70), (0.34, 0.55), (0.52, 0.36),
        ];
        for (fx, fy) in towers {
            p.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Percent(fx * 100.0),
                    top: Val::Percent(fy * 100.0),
                    width: Val::Px(4.0),
                    height: Val::Px(4.0),
                    ..default()
                },
                BackgroundColor(theme.state_colors.health.color()),
            ));
        }
        // Enemy dot pool (red) — refreshed live from real attackers.
        for _ in 0..10 {
            p.spawn((
                MockEnemyDot,
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
        let (wt, wf, wc) = text_node(
            "W8",
            theme.typography.font_caption,
            theme.palette.text_secondary.color(),
        );
        p.spawn((wt, wf, wc, Node {
            position_type: PositionType::Absolute,
            bottom: Val::Px(3.0),
            right: Val::Px(6.0),
            ..default()
        }));
    });
}

fn tooltip_demo(world: &mut World, spec: &PlanSpec, theme: &UiTheme) {
    let mut e = world.spawn((
        MockPanel(PanelGroup::TooltipDemo),
        panel_node(theme, spec.tooltip),
        Visibility::Hidden,
    ));
    e.with_children(|p| {
        let (t1, f1, c1) = text_node(
            "Frost Arrow — [W]",
            theme.typography.font_tooltip,
            theme.palette.accent.color(),
        );
        p.spawn((t1, f1, c1));
        let (t2, f2, c2) = text_node(
            "45 magic damage, slow 25% for 2s",
            theme.typography.font_caption,
            theme.palette.text_primary.color(),
        );
        p.spawn((t2, f2, c2));
        let (t3, f3, c3) = text_node(
            "MANA 40 / CD 6s",
            theme.typography.font_caption,
            theme.palette.text_secondary.color(),
        );
        p.spawn((t3, f3, c3));
    });
}

// ---------------------------------------------------------------------------
// Live refresh (numbers follow the real game; cache-key guarded)
// ---------------------------------------------------------------------------

fn refresh_mock_hud(world: &mut World) {
    let Some(frame) = world.get_resource::<MockFrame>() else { return };
    if frame.0 % 10 != 2 {
        return;
    }
    let mut cache: HudCache = (*world
        .get_resource_or_insert_with(HudCache::default))
        .clone();

    // Gold readout follows bounty income.
    let gold = world.resource::<Resources>().gold;
    if gold != cache.gold {
        cache.gold = gold;
        let mut q = world.query_filtered::<&mut Text, With<MockGoldText>>();
        if let Ok(mut txt) = q.single_mut(world) {
            *txt = Text::new(gold.to_string());
        }
    }

    // Wave banner: intermission countdown vs incoming.
    let director = world.resource::<WaveDirector>();
    let label = match director.phase {
        DirectorPhase::Intermission => format!(
            "WAVE {} IN {:.0}s",
            director.next_wave + 1,
            director.phase_timer.max(0.0)
        ),
        DirectorPhase::Spawning => {
            format!("WAVE {} INCOMING", director.running.unwrap_or(0) + 1)
        }
        _ => format!("WAVE {}", director.next_wave + 1),
    };
    if label != cache.wave_label {
        cache.wave_label = label.clone();
        let mut q = world.query_filtered::<&mut Text, With<MockWaveText>>();
        if let Ok(mut txt) = q.single_mut(world) {
            *txt = Text::new(label);
        }
    }

    // Hero HP fills follow the real hero.
    if let Some(hero) = world.get_resource::<MockHero>() {
        let hp = world.get::<Hp>(hero.0).map(|h| h.0);
        let max = world.get::<MaxHp>(hero.0).map(|m| m.0);
        if let (Some(hp), Some(max)) = (hp, max) {
            if (hp - cache.hero_hp).abs() > 0.5 {
                cache.hero_hp = hp;
                let frac = (hp / max.max(1.0) * 100.0).clamp(0.0, 100.0);
                let mut q = world.query_filtered::<&mut Node, With<MockHeroHpFill>>();
                for mut node in q.iter_mut(world) {
                    node.width = Val::Percent(frac);
                }
            }
        }
    }

    refresh_enemy_dots(world);
    *world.resource_mut::<HudCache>() = cache;
}

/// Map live attacker positions onto the minimap dot pool. The grid spans
/// x in [-14,14], z in [-9,9] (28x18 cells).
fn refresh_enemy_dots(world: &mut World) {
    let mut dots: Vec<Entity> = world
        .query_filtered::<Entity, With<MockEnemyDot>>()
        .iter(world)
        .collect();
    let enemies: Vec<(f32, f32)> = {
        let mut q = world.query_filtered::<(&Transform, &Allegiance), With<UnitInstance>>();
        q.iter(world)
            .filter(|(_, side)| **side == Allegiance::Attacker)
            .map(|(tf, _)| (tf.translation.x, tf.translation.z))
            .collect()
    };
    for (i, dot) in dots.iter_mut().enumerate() {
        let vis = match enemies.get(i) {
            Some((x, z)) => {
                let fx = ((x + 14.0) / 28.0 * 100.0).clamp(2.0, 97.0);
                let fy = ((9.0 - z) / 18.0 * 100.0).clamp(2.0, 97.0);
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

// ---------------------------------------------------------------------------
// Frame driver: stage transitions, screenshots, self-exit
// ---------------------------------------------------------------------------

fn mock_frame_driver(world: &mut World) {
    let frame = &mut world.resource_mut::<MockFrame>().0;
    *frame += 1;
    let f = *frame;

    // Frame 1: retire the legacy HUD pieces (the design under rejection).
    if f == 1 {
        let legacy: Vec<Entity> = world
            .query_filtered::<Entity, With<crate::systems::hud::HudText>>()
            .iter(world)
            .collect();
        for e in legacy {
            let _ = world.entity_mut(e).despawn();
        }
        if let Ok(panel) = world
            .query_filtered::<Entity, With<warbase::ui::CommandPanel>>()
            .single(world)
        {
            let _ = world.entity_mut(panel).insert(Visibility::Hidden);
        }
    }

    // Combat transition: start the wave, select the hero, swap card face.
    if f == 42 {
        start_mock_combat(world);
    }

    // Stage switches 12 frames before each shot.
    let shots = shots();
    for (frame_at, _, mode) in &shots {
        if f + 12 == *frame_at {
            apply_shot_mode(world, mode);
        }
    }

    // Screenshot moments.
    let dir = out_dir(world.resource::<PlanSpec>().id);
    std::fs::create_dir_all(&dir).ok();
    for (frame_at, stem, _) in &shots {
        if f == *frame_at {
            // Engine-side diagnostics (the adjudication channel when the
            // visual read and the simulation disagree).
            let mut q = world.query::<(&Transform, &Hp, &Allegiance, &UnitInstance)>();
            let enemies: Vec<String> = q
                .iter(world)
                .filter(|(tf, _, side, _)| {
                    tf.translation.y < 5.0 && **side == Allegiance::Attacker
                })
                .map(|(tf, hp, _, _)| {
                    format!("({:.1},{:.1},{:.0}hp)", tf.translation.x, tf.translation.z, hp.0)
                })
                .collect();
            let towers = {
                let mut q = world.query::<(&Allegiance, &UnitInstance)>();
                q.iter(world)
                    .filter(|(side, _)| **side == Allegiance::Defender)
                    .count()
            };
            info!(
                "[ui-mock] diag f{f}: towers={towers} enemies={} {:?}",
                enemies.len(),
                enemies
            );
            let path = format!("{dir}/{stem}.png");
            info!("[ui-mock] shot -> {path}");
            world
                .spawn(Screenshot::primary_window())
                .observe(save_to_disk(path));
        }
    }

    if f >= 258 {
        info!("[ui-mock] done — exiting");
        world.write_message(AppExit::Success);
    }
}

fn apply_shot_mode(world: &mut World, mode: &ShotMode) {
    let solo_group: Option<PanelGroup> = match mode {
        ShotMode::All => None,
        ShotMode::Solo(g) => Some(*g),
        ShotMode::GridOnly | ShotMode::SkillsOnly => Some(PanelGroup::CommandCard),
    };
    let mut q = world.query_filtered::<(&MockPanel, &mut Visibility), ()>();
    for (group, mut vis) in q.iter_mut(world) {
        let show = match solo_group {
            None => !matches!(group.0, PanelGroup::TooltipDemo),
            Some(g) => group.0 == g,
        };
        *vis = if show { Visibility::Visible } else { Visibility::Hidden };
    }
    // Command-card sub-rows (grid vs skills) for the two command shots.
    let show_grid = !matches!(mode, ShotMode::SkillsOnly);
    let mut grid_q = world.query_filtered::<&mut Visibility, With<MockCardGrid>>();
    for mut vis in grid_q.iter_mut(world) {
        *vis = if show_grid { Visibility::Visible } else { Visibility::Hidden };
    }
    let mut skill_q = world.query_filtered::<&mut Visibility, With<MockSkillRow>>();
    for mut vis in skill_q.iter_mut(world) {
        *vis = if matches!(mode, ShotMode::SkillsOnly | ShotMode::All) {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
    }
}

fn start_mock_combat(world: &mut World) {
    // Wave 8 running for real (empty ledger: the staged squad IS the wave).
    world.resource_mut::<WaveDirector>().force_begin(7);
    // Staged squad around the ring at staggered distances.
    let path_e = world.resource::<Paths>().0[0];
    let registry = world.resource::<UnitRegistry>().clone();
    let squad: &[(&str, f32, f32)] = &[
        // (def, path progress, hp fraction)
        ("grunt", 2.0, 0.8), ("grunt", 4.0, 0.55), ("grunt", 6.0, 1.0),
        ("fast", 9.0, 0.7), ("grunt", 12.0, 0.9), ("tank", 18.0, 0.65),
        ("flyer", 24.0, 1.0), ("grunt", 30.0, 0.45), ("fast", 34.0, 0.85),
    ];
    for (def, progress, hp_frac) in squad {
        let pos = world
            .get::<PathDef>(path_e)
            .map(|p| p.sample(*progress))
            .unwrap_or_default();
        if let Some(u) = spawn_unit(
            world,
            &registry,
            def,
            Vec3::new(pos.x, 0.0, pos.y),
            Allegiance::Attacker,
            4.0, // hp scale: durable enough to survive until the shots
        ) {
            let _ = world.entity_mut(u).insert((
                PathFollower { path: path_e, progress: *progress, laps: 0, finished: false },
                WaveTag(7),
            ));
            if *hp_frac < 1.0 {
                if let Some(max) = world.get::<MaxHp>(u).map(|m| m.0) {
                    if let Some(mut hp) = world.get_mut::<Hp>(u) {
                        hp.0 = max * hp_frac;
                    }
                }
            }
        }
    }
    // Hero selected -> unit card face.
    let hero = world.resource::<MockHero>().0;
    world.insert_resource(Selection { mode: SelectionMode::Unit(hero) });
    rebuild_card_face(world, "hunter");
    info!("[ui-mock] combat staged: wave 8 running, hero selected");
}

// ---------------------------------------------------------------------------
// Tests (headless): the three proposals are real, distinct, and on-canvas.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn within(r: &Rect) -> bool {
        r.x >= 0.0 && r.y >= 0.0 && r.x + r.w <= CANVAS.0 && r.y + r.h <= CANVAS.1
    }

    /// Every plan's every panel rect fits the 1280x720 canvas (R1: panels
    /// stay in edge anchor zones by construction).
    #[test]
    fn all_plan_rects_fit_the_canvas() {
        for id in ['A', 'B', 'C'] {
            let s = plan_spec(id);
            for (name, r) in [
                ("res", &s.res), ("wave", &s.wave), ("hero", &s.hero), ("cmd", &s.cmd),
                ("item", &s.item), ("bonds", &s.bonds), ("minimap", &s.minimap),
                ("tooltip", &s.tooltip),
            ] {
                assert!(within(r), "plan {id} panel {name} off-canvas: {r:?}");
            }
            // The battlefield core (central 50% x 50%, which fully covers
            // the loop ring) stays clear of every permanent panel (R1).
            for (name, r) in [
                ("res", &s.res), ("wave", &s.wave), ("hero", &s.hero), ("cmd", &s.cmd),
                ("item", &s.item), ("bonds", &s.bonds), ("minimap", &s.minimap),
            ] {
                let cx0 = CANVAS.0 * 0.25;
                let cx1 = CANVAS.0 * 0.75;
                let cy0 = CANVAS.1 * 0.25;
                let cy1 = CANVAS.1 * 0.75;
                let overlap_x = r.x < cx1 && r.x + r.w > cx0;
                let overlap_y = r.y < cy1 && r.y + r.h > cy0;
                assert!(
                    !(overlap_x && overlap_y),
                    "plan {id} panel {name} intrudes into the center battlefield: {r:?}"
                );
            }
        }
    }

    /// The three proposals are genuinely different skins AND layouts (the
    /// H-UI choice is a real choice, not three recolors).
    #[test]
    fn plans_are_distinct_designs() {
        let a = plan_spec('A');
        let b = plan_spec('B');
        let c = plan_spec('C');
        assert_ne!(a.theme.palette.accent, b.theme.palette.accent);
        assert_ne!(b.theme.palette.accent, c.theme.palette.accent);
        assert_ne!(a.theme.palette.accent, c.theme.palette.accent);
        // Structural spread: console strip vs floating island vs micro row.
        assert!(a.cmd.h > b.cmd.h && b.cmd.h > c.cmd.h, "console -> island -> micro heights");
        assert_ne!(a.cmd_cols, c.cmd_cols, "grid vs single-row command layout");
        // Plan C is genuinely low-UI-area (R1: immersion proposal).
        let area = |s: &PlanSpec| {
            [s.res, s.wave, s.hero, s.cmd, s.item, s.bonds, s.minimap]
                .iter().map(|r| r.w * r.h).sum::<f32>()
        };
        assert!(area(&c) < area(&b) * 0.75, "plan C occupies markedly less canvas");
        assert!(
            area(&a) > area(&b) && area(&b) > area(&c),
            "console -> floating -> micro canvas ordering"
        );
    }

    /// Plan themes ride the UiTheme serde contract (skins are data).
    #[test]
    fn plan_themes_are_serializable() {
        for id in ['A', 'B', 'C'] {
            let s = plan_spec(id);
            let out = ron::to_string(&s.theme).expect("serialize");
            let back: UiTheme = ron::from_str(&out).expect("deserialize");
            assert_eq!(s.theme, back);
        }
    }
}
