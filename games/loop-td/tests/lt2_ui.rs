//! LT2-UI acceptance: the Plan-A (铁金条约) panel suite — every panel
//! renders from the `UiTheme` tokens (a skin mutate re-skins them, zero
//! code change), the bundled CJK font reaches every text (no tofu), the
//! six feature panels reflect REAL game state, and the boarded skin data is
//! pinned digit for digit. Runs the REAL game composition (GamePlugin on
//! the base test app), same stance as tests/v2_depth.rs.

use bevy::prelude::*;
use bevy::text::{Font, FontSource};
use loop_td::plugins::game::GamePlugin;
use loop_td::systems::gameplay::try_place;
use loop_td::systems::panels::{PanelKind, PanelRoot};
use warbase::prelude::*;
use warbase::testing::test_app;
use warbase::ui::{ThemeColor, UiTheme};

// ---------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------

/// The real game headless (same recipe as v2_depth): base test app +
/// GamePlugin (which carries the Plan-A theme + PanelsPlugin); Startup loads
/// the shipped map (grid, cards, item table, bonds).
fn game_app() -> App {
    let mut app = test_app();
    app.add_plugins(GamePlugin);
    app.insert_resource(MatchRng::seeded(42));
    app.update();
    app.update();
    app
}

/// First free grid cell (row-major).
fn first_free_cell(grid: &BuildGrid) -> Cell {
    (0..grid.cols)
        .flat_map(|x| (0..grid.rows).map(move |y| Cell(x, y)))
        .find(|c| grid.check(*c).is_ok())
        .expect("a free cell")
}

/// Place the hero tower (funded) and let one update run so the attach kits
/// (Selectable / Inventory / EquipSlots) land on the fresh spawn.
fn place_hero(app: &mut App) -> Entity {
    app.world_mut().resource_mut::<Resources>().gold += 200;
    let cell = first_free_cell(app.world().resource::<BuildGrid>());
    let hero = try_place(&mut app.world_mut(), "hunter", cell).expect("place hunter");
    app.update();
    hero
}

fn select_hero(app: &mut App, hero: Entity) {
    app.world_mut().insert_resource(warbase::ui::Selection {
        mode: warbase::ui::SelectionMode::Unit(hero),
    });
    app.update();
    app.update();
}

fn panel_entity(app: &mut App, kind: PanelKind) -> Entity {
    let world = app.world_mut();
    let mut q = world.query_filtered::<Entity, With<PanelRoot>>();
    q.iter(world)
        .find(|&e| world.get::<PanelRoot>(e).map(|r| r.0 == kind).unwrap_or(false))
        .expect("panel root exists")
}

fn panel_visible(app: &mut App, kind: PanelKind) -> bool {
    let e = panel_entity(app, kind);
    *app.world().get::<Visibility>(e).unwrap() == Visibility::Visible
}

/// All text strings under a panel root (descendant walk).
fn panel_texts(app: &mut App, kind: PanelKind) -> Vec<String> {
    fn walk(world: &World, e: Entity, out: &mut Vec<String>) {
        if let Some(t) = world.get::<Text>(e) {
            out.push(t.0.clone());
        }
        if let Some(children) = world.get::<Children>(e) {
            for c in children.iter() {
                walk(world, c, out);
            }
        }
    }
    let root = panel_entity(app, kind);
    let mut out = Vec::new();
    walk(app.world(), root, &mut out);
    out
}

fn texts_join(app: &mut App, kind: PanelKind) -> String {
    panel_texts(app, kind).join("|")
}

// ---------------------------------------------------------------------------
// Skin data: the shipped RON pins the boarded Plan-A tokens
// ---------------------------------------------------------------------------

/// The shipped assets/ui/plan-a.ron carries the boarded values digit for
/// digit (a stray edit to the RON fails here, not at runtime).
#[test]
fn plan_a_skin_pins_boarded_tokens() {
    let ui = loop_td::systems::theme::plan_a();
    assert_eq!(ui.revision, 1);
    assert_eq!(ui.palette.panel_bg, ThemeColor::rgba(0.10, 0.09, 0.08, 0.94));
    assert_eq!(ui.palette.panel_raised, ThemeColor::rgb(0.18, 0.16, 0.13));
    assert_eq!(ui.palette.panel_border, ThemeColor::rgb(0.52, 0.40, 0.18));
    assert_eq!(ui.palette.text_primary, ThemeColor::rgb(0.98, 0.95, 0.86));
    assert_eq!(ui.palette.text_secondary, ThemeColor::rgb(0.76, 0.70, 0.58));
    assert_eq!(ui.palette.text_dim, ThemeColor::rgb(0.46, 0.42, 0.36));
    assert_eq!(ui.palette.accent, ThemeColor::rgb(1.0, 0.80, 0.28));
    assert_eq!(ui.state_colors.slot_disabled_bg, ThemeColor::rgb(0.13, 0.12, 0.11));
    assert_eq!(ui.state_colors.tooltip_bg, ThemeColor::rgba(0.07, 0.06, 0.05, 0.96));
    assert_eq!(ui.typography.font_value, 22.0);
    assert_eq!(ui.typography.font_label, 15.0);
    assert_eq!(ui.typography.font_caption, 11.0);
    assert_eq!(ui.typography.font_badge, 10.0);
    assert_eq!(ui.typography.font_tooltip, 13.0);
    assert_eq!(ui.typography.font_title, 16.0);
    // The loop-td boarding included layout spacing (the mock laid out with it).
    assert_eq!(ui.spacing.panel_margin, 6.0);
    assert_eq!(ui.spacing.panel_gap, 5.0);
    assert_eq!(ui.spacing.slot_w, 78.0);
    assert_eq!(ui.spacing.slot_h, 38.0);
    assert_eq!(ui.spacing.panel_border_w, 2.0);
    assert_eq!(ui.spacing.slot_border_w, 1.0);
    assert_eq!(ui.spacing.bar_h, 12.0);
    assert_eq!(ui.spacing.icon_size, 16.0);
    assert_eq!(ui.radius.panel, 3.0);
    assert_eq!(ui.radius.slot, 2.0);
    assert_eq!(ui.radius.pill, 999.0);
}

/// The proposal mock reads the exact bytes the game ships — proposal shots
/// and the shipped HUD can never drift apart.
#[test]
fn mock_plan_a_reads_the_shipped_skin() {
    let spec = loop_td::plugins::ui_mock::plan_spec('A');
    assert_eq!(spec.theme, loop_td::systems::theme::plan_a());
}

// ---------------------------------------------------------------------------
// CJK font: the file travels, covers the HUD glyphs, reaches every text
// ---------------------------------------------------------------------------

/// The bundled font file (and its OFL license) must stay in the repo so
/// every machine renders the same CJK text. Guards against accidental
/// deletes (the warden UI6 contract, mirrored).
#[test]
fn bundled_cjk_font_files_are_present() {
    let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/fonts");
    assert!(
        base.join("NotoSansSC-VariableFont_wght.ttf").is_file(),
        "assets/fonts/NotoSansSC-VariableFont_wght.ttf must exist (bundled CJK font)"
    );
    assert!(
        base.join("OFL.txt").is_file(),
        "assets/fonts/OFL.txt must exist (SIL OFL 1.1 license travels with the font)"
    );
}

/// Minimal sfnt `cmap` reader (subtable formats 4 and 12): does the bundled
/// font actually map the code points the HUD prints? A .notdef/absent
/// mapping is exactly what renders as tofu (bevy's default font has none of
/// these CJK glyphs — that was the R4 reject).
fn cmap_covers(bytes: &[u8], cp: u32) -> bool {
    let u16 = |o: usize| u16::from_be_bytes([bytes[o], bytes[o + 1]]);
    let read_u32 = |o: usize| u32::from_be_bytes([bytes[o], bytes[o + 1], bytes[o + 2], bytes[o + 3]]);
    let num_tables = u16(4) as usize;
    for i in 0..num_tables {
        let rec = 12 + i * 16;
        if &bytes[rec..rec + 4] != b"cmap" {
            continue;
        }
        let cmap_off = read_u32(rec + 8) as usize;
        let n_sub = u16(cmap_off + 2) as usize;
        for t in 0..n_sub {
            let srec = cmap_off + 4 + t * 8;
            let sub_off = cmap_off + read_u32(srec + 4) as usize;
            match u16(sub_off) {
                4 => {
                    let seg_x2 = u16(sub_off + 6) as usize;
                    let seg_count = seg_x2 / 2;
                    let end_base = sub_off + 14;
                    let start_base = end_base + seg_x2 + 2;
                    let delta_base = start_base + seg_x2;
                    let range_base = delta_base + seg_x2;
                    for s in 0..seg_count {
                        let end = u16(end_base + s * 2) as u32;
                        let start = u16(start_base + s * 2) as u32;
                        if !(start..=end).contains(&cp) {
                            continue;
                        }
                        let delta = u16(delta_base + s * 2);
                        let range_off = u16(range_base + s * 2);
                        let glyph = if range_off == 0 {
                            (cp as u16).wrapping_add(delta)
                        } else {
                            let addr = range_base + s * 2 + range_off as usize + (cp - start) as usize * 2;
                            u16(addr)
                        };
                        return glyph != 0; // 0 = .notdef = tofu
                    }
                }
                12 => {
                    let n_groups = read_u32(sub_off + 12) as usize;
                    for g in 0..n_groups {
                        let go = sub_off + 16 + g * 12;
                        let start = read_u32(go);
                        let end = read_u32(go + 4);
                        if (start..=end).contains(&cp) {
                            return true;
                        }
                    }
                }
                _ => continue,
            }
        }
    }
    false
}

/// Every CJK label the Plan-A HUD prints maps to a real glyph in the
/// bundled font — the tofu fix is glyph-verified, not assumed.
#[test]
fn bundled_font_covers_the_hud_cjk_glyphs() {
    let path = loop_td::systems::font::font_file();
    let bytes = std::fs::read(&path).expect("bundled font readable");
    let mut missing: Vec<char> = Vec::new();
    for ch in "金木人口存量波清首领神器任务已认领可取队光环攻挑战下一血赏挖宝期望空手装箭塔炮冰哨羁绊急冻卫间生命魔法经验射程上猎手战鼓腐".chars() {
        if !cmap_covers(&bytes, ch as u32) {
            missing.push(ch);
        }
    }
    assert!(missing.is_empty(), "font lacks glyphs for: {missing:?}");
}

/// Headless-with-Assets<Font>: the font system resolves a REAL loaded font
/// (not the default handle) and every panel text carries it — the wiring
/// the tofu fix rides on.
#[test]
fn ui_font_handle_reaches_every_panel_text() {
    let mut app = test_app();
    app.init_resource::<Assets<Font>>();
    app.add_plugins(GamePlugin);
    app.insert_resource(MatchRng::seeded(42));
    app.update();
    app.update();
    app.update();

    let handle = match &app.world().resource::<loop_td::systems::font::UiFont>().0 {
        FontSource::Handle(h) => h.clone(),
        other => panic!("expected a real font handle, got {other:?}"),
    };
    assert_ne!(handle, Handle::<Font>::default(), "the bundled font must load");

    // Every text UNDER a panel root carries the loaded handle (the hidden
    // warbase command panel's own texts are engine-side and excluded).
    fn walk_fonts(world: &World, e: Entity, handle: &Handle<Font>, checked: &mut usize) {
        if let Some(tf) = world.get::<bevy::text::TextFont>(e) {
            match &tf.font {
                FontSource::Handle(h) => assert_eq!(h, handle, "a panel text misses the CJK font"),
                other => panic!("a panel text carries no font: {other:?}"),
            }
            *checked += 1;
        }
        if let Some(children) = world.get::<Children>(e) {
            for c in children.iter() {
                walk_fonts(world, c, handle, checked);
            }
        }
    }
    let kinds = [
        PanelKind::Resource,
        PanelKind::Wave,
        PanelKind::Minimap,
        PanelKind::Artifact,
        PanelKind::Challenge,
        PanelKind::Dig,
        PanelKind::Bonds,
        PanelKind::Hero,
        PanelKind::Equip,
        PanelKind::Command,
    ];
    let mut checked = 0;
    for kind in kinds {
        let root = panel_entity(&mut app, kind);
        walk_fonts(app.world(), root, &handle, &mut checked);
    }
    assert!(checked > 20, "the panel suite must carry texts (got {checked})");
}

/// Bare headless (no font store): fall back to the default handle without
/// panicking — the warden UI6 contract.
#[test]
fn ui_font_falls_back_to_default_headless() {
    let mut app = game_app();
    app.update();
    assert!(matches!(
        app.world().resource::<loop_td::systems::font::UiFont>().0,
        FontSource::Handle(ref h) if *h == Handle::<Font>::default()
    ));
}

// ---------------------------------------------------------------------------
// Panels render in Plan A and re-skin on a theme mutate (zero code change)
// ---------------------------------------------------------------------------

/// The panels ship the Plan-A surface colors; swapping the `UiTheme`
/// resource (revision bumped) re-skins them on the next update.
#[test]
fn panels_render_plan_a_and_reskin_on_theme_mutate() {
    let mut app = game_app();
    let plan_a = loop_td::systems::theme::plan_a();
    // Shipped skin: a panel root reads the Plan-A panel_bg.
    let res_root = panel_entity(&mut app, PanelKind::Resource);
    let bg = **app.world().get::<BackgroundColor>(res_root).unwrap();
    assert_eq!(bg, plan_a.palette.panel_bg.color(), "panel bg must be Plan-A");

    // Swap the skin: only a resource insert, revision bumped.
    let mut skin = plan_a.clone();
    skin.revision += 1;
    skin.palette.panel_bg = ThemeColor::rgb(0.5, 0.1, 0.1);
    app.insert_resource(skin);
    app.update();
    app.update();
    let res_root = panel_entity(&mut app, PanelKind::Resource);
    let bg = **app.world().get::<BackgroundColor>(res_root).unwrap();
    assert_eq!(bg, Color::srgb(0.5, 0.1, 0.1), "panel must re-skin on theme mutate");
    // Stable after the swap (keyed rebuild, no thrash).
    app.update();
    let bg = **app.world().get::<BackgroundColor>(res_root).unwrap();
    assert_eq!(bg, Color::srgb(0.5, 0.1, 0.1));
}

// ---------------------------------------------------------------------------
// The six feature panels reflect REAL game state
// ---------------------------------------------------------------------------

/// ①/⑥ Equip rack + hero panel: equipped items print name + affix in the
/// proposal vocabulary; an empty slot shows the [E] hint; the hero panel
/// carries the affix lines and the bars.
#[test]
fn equip_rack_and_hero_panel_reflect_the_rack() {
    let mut app = game_app();
    let hero = place_hero(&mut app);
    loop_td::systems::equip::equip_item(&mut app.world_mut(), hero, 0, "blade12").expect("equip");
    loop_td::systems::equip::equip_item(&mut app.world_mut(), hero, 1, "scope").expect("equip");
    select_hero(&mut app, hero);

    // Equip rack: 2 filled slots with affixes + one [E] empty hint.
    let equip = texts_join(&mut app, PanelKind::Equip);
    assert!(equip.contains("装备"), "title: {equip}");
    assert!(equip.contains("精锻利刃"), "item display name: {equip}");
    assert!(equip.contains("攻+12%"), "damage affix: {equip}");
    assert!(equip.contains("观测镜"), "range item: {equip}");
    assert!(equip.contains("射程+30%"), "range affix: {equip}");
    assert!(equip.contains("空 · [E] 上装"), "empty-slot hint: {equip}");

    // Hero panel: bars + the equip affix summary lines (⑥).
    let hero_txt = texts_join(&mut app, PanelKind::Hero);
    assert!(hero_txt.contains("猎手"), "hero name: {hero_txt}");
    assert!(hero_txt.contains("LV"), "level badge: {hero_txt}");
    assert!(hero_txt.contains("生命"), "hp bar: {hero_txt}");
    assert!(hero_txt.contains("精锻利刃"), "affix line: {hero_txt}");
    assert!(hero_txt.contains("射程+30%"), "range affix line: {hero_txt}");

    // Deselect: both panels hide.
    app.world_mut()
        .insert_resource(warbase::ui::Selection { mode: warbase::ui::SelectionMode::Build });
    app.update();
    app.update();
    assert!(!panel_visible(&mut app, PanelKind::Equip), "equip hides without a hero");
    assert!(!panel_visible(&mut app, PanelKind::Hero), "hero panel hides without a hero");
}

/// ② Artifact plaque: progress -> claimable highlight -> claimed aura line.
#[test]
fn artifact_plaque_progresses_highlight_and_claims() {
    let mut app = game_app();
    // Incomplete: plain task lines, no highlight copy.
    let t = texts_join(&mut app, PanelKind::Artifact);
    assert!(t.contains("神器任务"), "plaque title: {t}");
    assert!(!t.contains("可领取"), "not claimable yet: {t}");

    // Quest complete (real resource state): the gold claimable state.
    app.world_mut()
        .insert_resource(loop_td::systems::artifact::ArtifactQuest {
        waves_cleared: loop_td::systems::artifact::ARTIFACT_WAVES_REQUIRED,
        boss_kills: loop_td::systems::artifact::ARTIFACT_BOSSES_REQUIRED,
        claimed: false,
    });
    app.update();
    app.update();
    let t = texts_join(&mut app, PanelKind::Artifact);
    assert!(t.contains("可领取"), "claimable highlight: {t}");

    // Claimed (real claim path): the team aura line.
    loop_td::systems::artifact::claim_artifact(&mut app.world_mut()).expect("claim");
    app.update();
    app.update();
    let t = texts_join(&mut app, PanelKind::Artifact);
    assert!(t.contains("已认领"), "claimed plaque: {t}");
    assert!(t.contains("全队光环"), "aura line: {t}");
}

/// ③ Challenge entry: offered exactly at a milestone intermission, armed
/// after the C-arm, gone once the milestone is spent or a wave runs.
#[test]
fn challenge_entry_gates_on_milestone_intermission() {
    let mut app = game_app();
    // Wave 9 intermission: no offer.
    {
        let mut d = app.world_mut().resource_mut::<WaveDirector>();
        d.next_wave = 9;
        d.phase = DirectorPhase::Intermission;
        d.phase_timer = 5.0;
    }
    app.update();
    app.update();
    assert!(!panel_visible(&mut app, PanelKind::Challenge), "no offer at wave 9");

    // Wave 10 milestone: the gold offer.
    app.world_mut().resource_mut::<WaveDirector>().next_wave = 10;
    app.update();
    app.update();
    assert!(panel_visible(&mut app, PanelKind::Challenge), "offer at the milestone");
    let t = texts_join(&mut app, PanelKind::Challenge);
    assert!(t.contains("挑战下一波"), "offer copy: {t}");
    assert!(t.contains("[C]"), "hotkey hint: {t}");
    assert!(t.contains("血量x2"), "hp mult copy: {t}");
    assert!(t.contains("赏金x3"), "reward mult copy: {t}");

    // Armed: the standby copy.
    loop_td::systems::challenge::arm_challenge(&mut app.world_mut()).expect("arm");
    app.update();
    app.update();
    let t = texts_join(&mut app, PanelKind::Challenge);
    assert!(t.contains("已备战"), "armed copy: {t}");

    // Wave running: hidden.
    app.world_mut().resource_mut::<WaveDirector>().phase = DirectorPhase::Spawning;
    app.update();
    app.update();
    assert!(!panel_visible(&mut app, PanelKind::Challenge), "hidden while the wave runs");
}

/// ④ Dig entry: intermission-only, affordability dim, last outcome printed.
#[test]
fn dig_entry_gates_on_intermission_and_affordance() {
    let mut app = game_app();
    // Wave running: hidden.
    app.world_mut().resource_mut::<WaveDirector>().phase = DirectorPhase::Spawning;
    app.update();
    app.update();
    assert!(!panel_visible(&mut app, PanelKind::Dig), "no dig while a wave runs");

    // Intermission + gold: live, with the EV hint.
    app.world_mut().resource_mut::<WaveDirector>().phase = DirectorPhase::Intermission;
    app.world_mut().resource_mut::<Resources>().gold = 500;
    app.update();
    app.update();
    assert!(panel_visible(&mut app, PanelKind::Dig));
    let t = texts_join(&mut app, PanelKind::Dig);
    assert!(t.contains("挖宝"), "title: {t}");
    assert!(t.contains("50金"), "cost: {t}");
    assert!(t.contains("期望 +31.5金"), "EV hint: {t}");

    // Last outcome rides the real DigStats.
    app.world_mut().insert_resource(loop_td::systems::dig::DigStats {
        rolls: 1,
        gold_won: 80,
        equipment_found: 0,
        last: Some(loop_td::systems::dig::DigOutcome::Gold(80)),
    });
    app.update();
    app.update();
    let t = texts_join(&mut app, PanelKind::Dig);
    assert!(t.contains("上次:+80金"), "last outcome: {t}");

    // Broke: the disabled look (slot_disabled_bg surface inside the panel).
    let disabled = app
        .world()
        .resource::<UiTheme>()
        .state_colors
        .slot_disabled_bg
        .color();
    app.world_mut().resource_mut::<Resources>().gold = 10;
    app.update();
    app.update();
    let root = panel_entity(&mut app, PanelKind::Dig);
    fn collect_bgs(world: &World, e: Entity, out: &mut Vec<Color>) {
        if let Some(bg) = world.get::<BackgroundColor>(e) {
            out.push(**bg);
        }
        if let Some(children) = world.get::<Children>(e) {
            for c in children.iter() {
                collect_bgs(world, c, out);
            }
        }
    }
    let mut bgs = Vec::new();
    collect_bgs(app.world(), root, &mut bgs);
    assert!(bgs.contains(&disabled), "broke dig renders the disabled surface");
}

/// ⑤ Bond chips: satisfied combos light up (accent border), unsatisfied stay
/// dim with live counts.
#[test]
fn bond_chips_mark_satisfied_combos() {
    let mut app = game_app();
    let t = texts_join(&mut app, PanelKind::Bonds);
    assert!(t.contains("箭雨急冻"), "arrow_ice chip: {t}");
    assert!(t.contains("机动哨卫"), "cannon_sentry chip: {t}");
    assert!(t.contains("0/2"), "live counts start at zero: {t}");

    // 2 arrows + 1 ice satisfies arrow_ice (cannon_sentry stays partial).
    app.world_mut().resource_mut::<Resources>().gold += 300;
    let grid = app.world().resource::<BuildGrid>().clone();
    let mut cells = (0..grid.cols)
        .flat_map(|x| (0..grid.rows).map(move |y| Cell(x, y)))
        .filter(|c| grid.check(*c).is_ok());
    for _ in 0..2 {
        let c = cells.next().unwrap();
        try_place(&mut app.world_mut(), "arrow", c).expect("arrow");
    }
    let ice_cell = cells.next().unwrap();
    try_place(&mut app.world_mut(), "ice", ice_cell).expect("ice");
    app.update();
    app.update();
    let t = texts_join(&mut app, PanelKind::Bonds);
    assert!(t.contains("全体攻+10%"), "effect line: {t}");
    assert!(t.contains("全体间-8%"), "second effect: {t}");
    assert!(t.contains("箭塔2/2"), "satisfied member counts: {t}");
    assert!(t.contains("冰塔1/1"), "satisfied ice count: {t}");

    // Exactly the satisfied chip wears the accent border.
    let accent = app.world().resource::<UiTheme>().palette.accent.color();
    let root = panel_entity(&mut app, PanelKind::Bonds);
    fn collect_borders(world: &World, e: Entity, out: &mut Vec<Color>) {
        if let Some(bc) = world.get::<BorderColor>(e) {
            out.push(bc.bottom); // ::all() carries the same color on every side
        }
        if let Some(children) = world.get::<Children>(e) {
            for c in children.iter() {
                collect_borders(world, c, out);
            }
        }
    }
    let mut borders = Vec::new();
    collect_borders(app.world(), root, &mut borders);
    assert_eq!(
        borders.iter().filter(|c| **c == accent).count(),
        1,
        "exactly one satisfied chip is highlighted: {borders:?}"
    );
}

/// The legacy single-line HUD retires with the panels (R2: no bare text).
#[test]
fn legacy_hud_line_retires() {
    let mut app = game_app();
    app.update();
    let mut q = app.world_mut().query_filtered::<Entity, With<loop_td::systems::hud::HudText>>();
    assert_eq!(q.iter(app.world()).count(), 0, "the debug HUD line must be gone");
}
