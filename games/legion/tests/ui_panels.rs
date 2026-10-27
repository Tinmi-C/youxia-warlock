//! legion LG3 acceptance (ADR-0019 Phase 2, proposal §3 LG3): the Plan-A
//! panel suite — every panel renders from the `UiTheme` tokens (a skin
//! mutate re-skins them, zero code change), the bundled CJK font reaches
//! every text (no tofu), and the six surfaces reflect REAL round-layer
//! state (LivesBoard / def table prices+pop+types / frame-converted
//! countdown / escalating send prices / the three-state banner / the
//! deploy-slot preview highlight). Runs the REAL game composition
//! (GamePlugin carries the theme + PanelsPlugin), same stance as
//! tests/behavior.rs.

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use bevy::text::{Font, FontSource};
use legion::components::{DeploySlot, Merc};
use legion::resources::{LivesBoard, PendingSends, Phase, Purse, RoundFlow};
use legion::systems::panels::{
    merc_name, DeployGlow, PanelKind, PanelRoot, SETTLE_FLASH_FRAMES,
};
use legion::systems::round::{HireCommand, RoundSettled, SendCommand};
use warbase::prelude::*;
use warbase::testing::{send, test_app};
use warbase::ui::{CommandAction, CommandSlotButton, ThemeColor, UiTheme};

// -- helpers ---------------------------------------------------------------

/// The real game headless: base test app + GamePlugin (which carries the
/// Plan-A theme + PanelsPlugin); Startup assembles the map and the panel
/// roots; one update lets the first refresh build the trees.
fn game_app() -> App {
    let mut app = test_app();
    app.add_plugins(legion::plugins::game::GamePlugin);
    app.update();
    app
}

/// First free deploy slot of `player` (authored slot order = deterministic).
fn free_slot(app: &mut App, player: u8) -> Entity {
    let occupied: Vec<Entity> = {
        let mut q = app.world_mut().query::<&Merc>();
        q.iter(app.world()).map(|m| m.slot).collect()
    };
    let mut q = app.world_mut().query::<(Entity, &DeploySlot)>();
    let (entity, _) = q
        .iter(app.world())
        .find(|(e, s)| s.player == player && !occupied.contains(e))
        .expect("player has a free slot");
    entity
}

/// Set every wallet to `gold`.
fn set_gold(app: &mut App, gold: u32) {
    app.world_mut().resource_mut::<Purse>().0 = [gold; 4];
}

fn panel_entity(app: &mut App, kind: PanelKind) -> Entity {
    let world = app.world_mut();
    let mut q = world.query_filtered::<Entity, With<PanelRoot>>();
    q.iter(world)
        .find(|&e| world.get::<PanelRoot>(e).map(|r| r.0 == kind).unwrap_or(false))
        .expect("panel root exists")
}

fn panel_visible(app: &mut App, kind: PanelKind) -> bool {
    let root = panel_entity(app, kind);
    *app.world().get::<Visibility>(root).unwrap() == Visibility::Visible
}

/// Every text string under a panel root, depth-first.
fn panel_texts(app: &mut App, kind: PanelKind) -> Vec<String> {
    fn walk(world: &World, e: Entity, out: &mut Vec<String>) {
        if let Some(text) = world.get::<Text>(e) {
            out.push((**text).clone());
        }
        if let Some(children) = world.get::<Children>(e) {
            for &c in children {
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
    panel_texts(app, kind).join(" | ")
}

/// Shop slot buttons by hire def id (the six Custom("hire:<def>") entries).
fn hire_buttons(app: &mut App) -> Vec<(Entity, String)> {
    let world = app.world_mut();
    let mut q = world.query_filtered::<(Entity, &CommandSlotButton), With<Button>>();
    q.iter(world)
        .filter_map(|(e, b)| match &b.action {
            CommandAction::Custom(id) => {
                id.strip_prefix("hire:").map(|def| (e, def.to_string()))
            }
            _ => None,
        })
        .collect()
}

/// Count live `M` messages written over `frames` updates.
fn run_counting<M: Message>(app: &mut App, frames: usize) -> usize {
    let mut cursor = MessageCursor::<M>::default();
    let mut total = 0;
    for _ in 0..frames {
        app.update();
        let msgs = app.world().resource::<Messages<M>>();
        total += cursor.read(msgs).count();
    }
    total
}

// -- ① theme wiring: the shipped skin + zero-code re-skin ----------------------

/// LG3-①: the boarded Plan-A skin ships as data and parses with its tokens
/// pinned (loop-td's RON, digit for digit — the 换肤复用 copy).
#[test]
fn plan_a_skin_pins_the_boarded_tokens() {
    let ui = legion::systems::theme::plan_a();
    assert_eq!(ui.revision, 1);
    assert_eq!(ui.palette.accent, ThemeColor::rgb(1.0, 0.80, 0.28));
    assert_eq!(ui.palette.panel_border, ThemeColor::rgb(0.52, 0.40, 0.18));
    assert_eq!(ui.typography.font_value, 22.0);
    assert_eq!(ui.spacing.panel_border_w, 2.0);
    assert_eq!(ui.radius.panel, 3.0);
}

/// LG3-①: panels render from the theme tokens — swapping ONLY the resource
/// (revision bumped) re-skins them on the next update, zero code change.
#[test]
fn panels_reskin_on_theme_mutate() {
    let mut app = game_app();
    assert!(panel_visible(&mut app, PanelKind::Resource), "panels render");

    let plan_a = app.world().resource::<UiTheme>().clone();
    let root = panel_entity(&mut app, PanelKind::Resource);
    let before = *app.world().get::<BackgroundColor>(root).unwrap();
    assert_eq!(before.0, plan_a.palette.panel_bg.color(), "Plan-A tokens paint it");

    // Swap the skin: only a resource insert, revision bumped.
    let mut skin = plan_a.clone();
    skin.revision += 1;
    skin.palette.panel_bg = ThemeColor::rgb(0.5, 0.1, 0.1);
    app.insert_resource(skin);
    app.update();
    let after = *app.world().get::<BackgroundColor>(root).unwrap();
    assert_eq!(after.0, Color::srgb(0.5, 0.1, 0.1), "panel must re-skin on theme mutate");
}

// -- ② lives board -------------------------------------------------------------

/// LG3-②: the board mirrors all four wallets' lives; a spent player reads
/// 出局 instead of a number.
#[test]
fn lives_board_reflects_four_players_and_elimination() {
    let mut app = game_app();
    app.world_mut().resource_mut::<LivesBoard>().0 = [7, 8, 9, 10];
    app.update();
    let joined = texts_join(&mut app, PanelKind::Lives);
    for (player, lives) in [(0, "7"), (1, "8"), (2, "9"), (3, "10")] {
        assert!(joined.contains(&format!("P{player}")), "row P{player}: {joined}");
        assert!(joined.contains(lives), "value {lives}: {joined}");
    }

    app.world_mut().resource_mut::<LivesBoard>().0 = [7, 0, 9, 10];
    app.update();
    let joined = texts_join(&mut app, PanelKind::Lives);
    assert!(joined.contains("出局"), "eliminated player reads 出局: {joined}");
    assert!(joined.contains("命数板"), "board has its CJK title: {joined}");
}

// -- ③ hire shop card -------------------------------------------------------------

/// LG3-③: the card lists all six mercenaries with the def table's price,
/// pop and attack/armor line, as clickable Custom("hire:<def>") slots.
#[test]
fn shop_card_lists_six_mercs_with_price_pop_and_types() {
    let mut app = game_app();
    set_gold(&mut app, 300);

    let buttons = hire_buttons(&mut app);
    let mut defs: Vec<String> = buttons.iter().map(|(_, d)| d.clone()).collect();
    defs.sort();
    assert_eq!(
        defs,
        ["merc_aoe", "merc_cheap", "merc_ranged", "merc_support", "merc_swarm", "merc_tank"],
        "six hire slots, one per def"
    );

    let joined = texts_join(&mut app, PanelKind::Shop);
    // Prices + pop from the def table (proposal §2 anchors).
    assert!(joined.contains("120金 · 人口3"), "tank anchor: {joined}");
    assert!(joined.contains("30金 · 人口1"), "cheap anchor: {joined}");
    assert!(joined.contains("100金 · 人口3"), "aoe anchor: {joined}");
    // Attack/armor vocabulary.
    assert!(joined.contains("穿刺攻 · 轻甲"), "ranged type line: {joined}");
    assert!(joined.contains("普通攻 · 重甲"), "tank type line: {joined}");
    assert!(joined.contains("攻城攻 · 中甲"), "aoe type line: {joined}");
    // CJK names cover the table.
    for def in ["merc_cheap", "merc_swarm", "merc_tank", "merc_ranged", "merc_aoe", "merc_support"]
    {
        assert!(joined.contains(merc_name(def)), "{} name shown: {joined}", def);
    }
}

// -- ① (data) build countdown ------------------------------------------------------

/// LG3-①: the countdown converts the frame clock to whole seconds on the
/// resource plaque (frame 0 -> 30s, near the end -> 1s).
#[test]
fn build_countdown_reads_the_frame_clock() {
    let mut app = game_app();
    app.world_mut().resource_mut::<RoundFlow>().frame = 0;
    app.update(); // frame 1 now
    let joined = texts_join(&mut app, PanelKind::Resource);
    assert!(joined.contains("30s"), "fresh phase shows 30s: {joined}");
    assert!(joined.contains("建造期倒计时"), "the countdown row is labeled: {joined}");
    assert!(joined.contains("人口"), "population on the plaque: {joined}");
    assert!(joined.contains("回合"), "round number on the plaque: {joined}");

    app.world_mut().resource_mut::<RoundFlow>().frame = 1795;
    app.update(); // frame 1796 -> 4 frames left -> 1s
    let joined = texts_join(&mut app, PanelKind::Resource);
    assert!(joined.contains("1s"), "brink of battle shows 1s: {joined}");
}

// -- ⑤ send entry -------------------------------------------------------------------

/// LG3-⑤: the entry previews the escalating prices (this / next / the one
/// after) and moves a step after a paid send — the send_cost ledger view.
#[test]
fn send_entry_previews_escalating_prices() {
    let mut app = game_app();
    set_gold(&mut app, 100);

    let joined = texts_join(&mut app, PanelKind::Send);
    assert!(joined.contains("目标 P1"), "the lane opponent is the target: {joined}");
    assert!(joined.contains("25金"), "first send 25: {joined}");
    assert!(joined.contains("27金"), "next send 27: {joined}");
    assert!(joined.contains("30金"), "the one after 30: {joined}");

    // One paid send steps the whole preview.
    send(&mut app, SendCommand { player: 0, target: 1, def: "merc_swarm".into() });
    app.update();
    assert_eq!(app.world().resource::<Purse>().0[0], 75, "the send charged 25");
    let joined = texts_join(&mut app, PanelKind::Send);
    assert!(joined.contains("33金"), "preview stepped to 33: {joined}");
    assert!(joined.contains("已排队 1"), "queued count shows: {joined}");
}

// -- ⑥ round banner -------------------------------------------------------------------

/// LG3-⑥: the banner walks its three Chinese states (build / battle /
/// settle) plus the end-of-match verdict.
#[test]
fn round_banner_walks_build_battle_settle_and_end() {
    let mut app = game_app();
    let joined = texts_join(&mut app, PanelKind::Banner);
    assert!(joined.contains("第 1 回合 · 建造期"), "build state: {joined}");

    app.world_mut().resource_mut::<RoundFlow>().phase = Phase::Battle;
    app.update();
    let joined = texts_join(&mut app, PanelKind::Banner);
    assert!(joined.contains("第 1 回合 · 战斗期"), "battle state: {joined}");

    // Settlement is instant in the round layer — the banner holds the
    // settle text for SETTLE_FLASH_FRAMES via the flash resource.
    // (`testing::send` advances one update itself.)
    send(&mut app, RoundSettled { round: 1, income: 50 });
    let joined = texts_join(&mut app, PanelKind::Banner);
    assert!(joined.contains("第 1 回合结算 · +50 金"), "settle state: {joined}");
    let left = app.world().resource::<legion::systems::panels::SettleFlash>().frames_left;
    assert_eq!(left, SETTLE_FLASH_FRAMES, "flash is fresh (no decay on its first frame)");

    // Match end overrides the phase states once the flash dies.
    app.world_mut().remove_resource::<legion::systems::panels::SettleFlash>();
    *app.world_mut().resource_mut::<MatchFlow>() =
        MatchFlow::Won("round cap 20: draw on lives (t0=20, t1=20)".into());
    app.update();
    let joined = texts_join(&mut app, PanelKind::Banner);
    assert!(joined.starts_with("平局"), "draw verdict readable: {joined}");
}

// -- ③ (font) no tofu --------------------------------------------------------------------

/// LG3-③: the shared CJK font file travels at the workspace root and, with
/// a font store, resolves to a REAL handle that every panel text carries —
/// the wiring the no-tofu acceptance rides on.
#[test]
fn cjk_font_handle_reaches_every_panel_text() {
    assert!(
        legion::systems::font::font_file().is_file(),
        "shared NotoSansSC must exist (single workspace copy)"
    );

    let mut app = test_app();
    app.init_resource::<Assets<Font>>();
    app.add_plugins(legion::plugins::game::GamePlugin);
    app.update();
    app.update();

    let handle = match &app.world().resource::<legion::systems::font::UiFont>().0 {
        FontSource::Handle(h) => h.clone(),
        other => panic!("expected a real font handle, got {other:?}"),
    };
    assert_ne!(handle, Handle::<Font>::default(), "the bundled font must load");
    // The base tooltip seam rides the same face.
    assert!(app.world().resource::<warbase::ui::UiFont>().0.is_some());

    // Every text UNDER a panel root carries the loaded handle.
    fn walk_fonts(world: &World, e: Entity, handle: &Handle<Font>, checked: &mut usize) {
        if let Some(tf) = world.get::<TextFont>(e) {
            match &tf.font {
                FontSource::Handle(h) => assert_eq!(h, handle, "a panel text misses the CJK font"),
                other => panic!("a panel text carries no font: {other:?}"),
            }
            *checked += 1;
        }
        if let Some(children) = world.get::<Children>(e) {
            for &c in children {
                walk_fonts(world, c, handle, checked);
            }
        }
    }
    let mut checked = 0;
    for kind in [
        PanelKind::Resource,
        PanelKind::Lives,
        PanelKind::Banner,
        PanelKind::Shop,
        PanelKind::Send,
    ] {
        let root = panel_entity(&mut app, kind);
        walk_fonts(app.world(), root, &handle, &mut checked);
    }
    assert!(checked >= 25, "expected a living panel suite, checked {checked} texts");

    // The suite carries actual CJK content (the glyphs the font must cover).
    let sample = texts_join(&mut app, PanelKind::Shop);
    assert!(sample.contains("雇佣兵商店"), "CJK title renders: {sample}");
}

// -- ④ deploy-slot preview highlight --------------------------------------------------------

/// LG3-④: hovering a shop card lights the local player's FREE deploy cells
/// (the placement preview); occupied cells, battle phase and unhovered all
/// clear it.
#[test]
fn deploy_glow_marks_own_free_slots_on_shop_hover() {
    let mut app = game_app();
    // Deep wallet: the hire below must not flip any afford bit (120->90
    // would grey the tank card, rebuild the shop and drop the hover state).
    set_gold(&mut app, 300);

    let glow_count = |app: &mut App| {
        let mut q = app.world_mut().query_filtered::<Entity, With<DeployGlow>>();
        q.iter(app.world()).count()
    };
    assert_eq!(glow_count(&mut app), 0, "nothing glows unhovered");

    // Hover the cheap card.
    let (btn, def) = hire_buttons(&mut app)
        .into_iter()
        .find(|(_, d)| d == "merc_cheap")
        .expect("cheap slot exists");
    *app.world_mut().get_mut::<Interaction>(btn).unwrap() = Interaction::Hovered;
    app.update();
    assert_eq!(glow_count(&mut app), 12, "all 12 own cells are free and lit");
    let markers: Vec<&DeployGlow> = {
        let mut q = app.world_mut().query_filtered::<&DeployGlow, ()>();
        q.iter(app.world()).collect()
    };
    assert!(markers.iter().all(|g| g.def == def), "the glow names the hovered def");

    // An occupied cell drops out of the preview (`send` advances a frame).
    let slot = free_slot(&mut app, 0);
    send(&mut app, HireCommand { player: 0, def: "merc_cheap".into(), slot });
    assert_eq!(glow_count(&mut app), 11, "the hired-into cell stops glowing");

    // Unhover clears; battle phase keeps the shop dark even hovered.
    *app.world_mut().get_mut::<Interaction>(btn).unwrap() = Interaction::None;
    app.update();
    assert_eq!(glow_count(&mut app), 0, "unhover clears every marker");
    *app.world_mut().get_mut::<Interaction>(btn).unwrap() = Interaction::Hovered;
    app.world_mut().resource_mut::<RoundFlow>().phase = Phase::Battle;
    app.update();
    assert_eq!(glow_count(&mut app), 0, "battle phase suspends the preview");
}

// -- ③⑤ (dispatch) clicks ride the base command seam ------------------------------------------

/// LG3-③⑤: pressing a shop/send slot goes through the BASE command-card
/// seam (command_clicks -> CommandCardAction -> Custom id) and lands in the
/// SAME HireCommand/SendCommand the keyboard bridges write.
#[test]
fn shop_clicks_dispatch_hire_and_send_commands() {
    let mut app = game_app();
    set_gold(&mut app, 300);
    // The keyboard path needs a selected own cell — same for the click path.
    let slot = free_slot(&mut app, 0);
    app.world_mut().resource_mut::<SelectionState>().entities = vec![slot];

    let (hire_btn, _) = hire_buttons(&mut app)
        .into_iter()
        .find(|(_, d)| d == "merc_tank")
        .expect("tank slot exists");
    *app.world_mut().get_mut::<Interaction>(hire_btn).unwrap() = Interaction::Pressed;
    run_counting::<legion::systems::round::HireRejected>(&mut app, 3);
    assert_eq!(app.world().resource::<Purse>().0[0], 300 - 120, "click-hired the 120g tank");
    {
        let mut q = app.world_mut().query::<&Merc>();
        assert_eq!(q.iter(app.world()).count(), 1, "the tank is on the field");
    }

    // The send button: same seam, SendCommand underneath.
    let send_btn = {
        let world = app.world_mut();
        let mut q = world.query_filtered::<(Entity, &CommandSlotButton), With<Button>>();
        q.iter(world)
            .find(|(_, b)| matches!(&b.action, CommandAction::Custom(id) if id == "send:merc_swarm"))
            .map(|(e, _)| e)
            .expect("send slot exists")
    };
    *app.world_mut().get_mut::<Interaction>(send_btn).unwrap() = Interaction::Pressed;
    run_counting::<legion::systems::round::SendRejected>(&mut app, 3);
    let queued = app.world().resource::<PendingSends>().0.len();
    assert_eq!(queued, 1, "click-send queued one swarm");
    assert_eq!(app.world().resource::<Purse>().0[0], 300 - 120 - 25, "the 25g send charged");
}
