//! WARDEN-SKIN regression: the Plan-A ("Iron & Gold", 铁金条约) re-skin of
//! warden's panels onto the base `UiTheme` tokens. Pins three acceptance
//! clauses:
//! 1. the shipped skin data (assets/ui/plan-a.ron) carries the boarded
//!    token values digit for digit (a stray edit to the RON fails here);
//! 2. warden's own panels (bar root / info zone / command card buttons)
//!    actually RENDER the Plan-A colors + type scale after the startup
//!    insert — skin swap reaches the pixels, not just the resource;
//! 3. the same skin resource drops onto the BASE command card unchanged
//!    (the zero-code re-skin promise, now with warden's own skin data).

use std::time::Duration;

use bevy::{prelude::*, state::app::StatesPlugin, time::TimeUpdateStrategy};

use warden::{
    plugins::{
        acquisition::AcquisitionPlugin, economy::EconomyPlugin, enemies::EnemiesPlugin,
        fx::FxPlugin, game::GamePlugin, map::MapPlugin, meta::MetaPlugin,
        projectiles::ProjectilesPlugin, skills::SkillsPlugin, towers::TowersPlugin, ui::UiPlugin,
        waves::WavesPlugin,
    },
    resources::{Hand, MetaSavePath, UiPrefsPath},
    states::GameState,
    systems::{
        command::{CommandBarRoot, CommandButton, InfoZoneText},
        theme,
    },
};
use warbase::ui::{
    CommandCardDef, CommandPanel, CommandSlotDef, Hotkey, Selection as BaseSelection, SelectionMode,
    ThemeColor, UiTheme, BUILD_CARD,
};

/// Headless app: the full gameplay stack (same composition as
/// pick_upcast.rs's harness — copied so each regression file stays an
/// independent binary).
fn test_app() -> App {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, StatesPlugin))
        .init_state::<GameState>()
        .add_plugins((
            FxPlugin,
            GamePlugin,
            MapPlugin,
            TowersPlugin,
            EnemiesPlugin,
            WavesPlugin,
            EconomyPlugin,
            AcquisitionPlugin,
            MetaPlugin,
            ProjectilesPlugin,
            SkillsPlugin,
            UiPlugin,
        ))
        .insert_resource(MetaSavePath(std::env::temp_dir().join("warden_meta_skin_test.txt")))
        .insert_resource(UiPrefsPath(std::env::temp_dir().join("warden_ui_skin_test.txt")))
        .init_resource::<ButtonInput<KeyCode>>()
        .init_resource::<ButtonInput<bevy::input::mouse::MouseButton>>()
        .init_resource::<Assets<Mesh>>()
        .init_resource::<Assets<StandardMaterial>>()
        .init_resource::<Assets<bevy::gltf::Gltf>>()
        .init_resource::<Assets<AnimationGraph>>()
        .init_resource::<Assets<AnimationClip>>()
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(
            1.0 / 60.0,
        )));
    app
}

/// AC 1 — the shipped RON pins the boarded Plan-A tokens (the loop-td mock
/// values the user reviewed in ui-proposals/plan-A/, digit for digit).
#[test]
fn plan_a_ron_pins_boarded_tokens() {
    let ui = theme::plan_a();
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
    assert_eq!(ui.spacing.panel_border_w, 2.0);
    assert_eq!(ui.spacing.slot_border_w, 1.0);
    assert_eq!(ui.radius.panel, 3.0);
    assert_eq!(ui.radius.slot, 2.0);
    // Fields the skin file omits fall back to the base defaults (serde
    // default groups) — semantic state colors stay the shared WC3 grammar.
    assert_eq!(ui.state_colors.gold, UiTheme::default().state_colors.gold);
    assert_eq!(ui.state_colors, {
        let mut d = UiTheme::default().state_colors;
        d.slot_disabled_bg = ui.state_colors.slot_disabled_bg;
        d.tooltip_bg = ui.state_colors.tooltip_bg;
        d
    });
}

/// AC 2 — after the startup insert, warden's own panel surfaces render the
/// Plan-A tokens: bar base, info-zone inset, enabled/disabled command
/// buttons, and the info-zone type scale.
#[test]
fn warden_panels_render_plan_a() {
    let mut app = test_app();
    // Give the player one owned tower type and plenty of gold so the build
    // card spawns an affordable (enabled) button on the first rebuild.
    app.world_mut().resource_mut::<Hand>().owned_towers.push(0);
    app.world_mut().resource_mut::<warden::resources::Economy>().gold = 999;
    for _ in 0..3 {
        app.update();
    }
    let ui = theme::plan_a();

    // Bar root = the Plan-A iron base + gold-brown border.
    let mut q = app
        .world_mut()
        .query_filtered::<(&BackgroundColor, &BorderColor), With<CommandBarRoot>>();
    let (bg, border) = q.single(app.world()).unwrap();
    assert_eq!(bg.0, ui.palette.panel_bg.color(), "bar root bg = plan-a panel_bg");
    assert_eq!(border.top, ui.palette.panel_border.color(), "bar root border = plan-a panel_border");

    // Info zone (the inset panel parenting InfoZoneText) = raised surface.
    let mut q = app
        .world_mut()
        .query_filtered::<&Children, With<CommandBarRoot>>();
    let info_zones: Vec<Entity> = q
        .single(app.world())
        .unwrap()
        .iter()
        .filter(|child| {
            app.world()
                .get::<Children>(*child)
                .map(|gc| gc.iter().any(|g| app.world().get::<InfoZoneText>(g).is_some()))
                .unwrap_or(false)
        })
        .collect();
    assert_eq!(info_zones.len(), 1, "one info zone under the bar root");
    let inset = app.world().get::<BackgroundColor>(info_zones[0]).unwrap();
    assert_eq!(
        inset.0, ui.palette.panel_raised.color(),
        "info zone bg = plan-a panel_raised"
    );
    let text = app
        .world_mut()
        .query_filtered::<&TextFont, With<InfoZoneText>>()
        .single(app.world())
        .unwrap();
    assert!(
        matches!(text.font_size, FontSize::Px(v) if (v - ui.typography.font_label).abs() < 0.01),
        "info zone font = plan-a font_label ({:?})",
        text.font_size
    );

    // Enabled build button = raised bg + gold accent border.
    let mut q = app
        .world_mut()
        .query_filtered::<(&BackgroundColor, &BorderColor, &CommandButton), ()>();
    let mut saw_enabled = false;
    for (bg, border, btn) in q.iter(app.world()) {
        if !matches!(btn.action, warden::systems::command::CommandAction::Build(_)) {
            continue;
        }
        if btn.enabled {
            assert_eq!(bg.0, ui.palette.panel_raised.color(), "enabled build bg");
            assert_eq!(border.top, ui.palette.accent.color(), "enabled build border = gold accent");
            saw_enabled = true;
        }
    }
    assert!(saw_enabled, "an affordable build button exists");

    // The same card greys to the disabled tokens when gold runs out (the
    // economy gold is in the card's cache key, so it rebuilds by itself).
    app.world_mut().resource_mut::<warden::resources::Economy>().gold = 0;
    for _ in 0..3 {
        app.update();
    }
    let mut q = app
        .world_mut()
        .query_filtered::<(&BackgroundColor, &CommandButton), ()>();
    let mut saw_disabled = false;
    for (bg, btn) in q.iter(app.world()) {
        if matches!(btn.action, warden::systems::command::CommandAction::Build(_)) && !btn.enabled
        {
            assert_eq!(
                bg.0, ui.state_colors.slot_disabled_bg.color(),
                "unaffordable build bg = plan-a slot_disabled_bg"
            );
            saw_disabled = true;
        }
    }
    assert!(saw_disabled, "an unaffordable build button exists");
}

/// AC 3 — the zero-code promise: inserting warden's Plan-A skin resource
/// re-skins the BASE command card (warbase's own panel + slot) with no
/// warden render code involved. Same harness shape as warbase's own
/// `theme_swap_reskins_panel_zero_code`, driven with warden's skin data.
#[test]
fn plan_a_skin_drops_onto_base_command_card() {
    let mut app = warbase::testing::test_app(); // WarbasePlugin => base UiPlugin + default UiTheme
    let mut cards = warbase::ui::CommandCards::default();
    cards.cards.insert(
        BUILD_CARD.to_string(),
        CommandCardDef {
            slots: vec![CommandSlotDef {
                label: "sell".into(),
                hotkey: Hotkey::None,
                tooltip: "refund".into(),
                action: warbase::ui::CommandAction::Sell,
                enabled: true,
                badge: Some('Q'),
            }],
        },
    );
    app.insert_resource(cards)
        .insert_resource(BaseSelection { mode: SelectionMode::Build });
    app.update(); // Startup + first rebuild: renders under the default skin.
    let mut q = app
        .world_mut()
        .query_filtered::<&BackgroundColor, With<CommandPanel>>();
    assert_eq!(
        q.single(app.world()).unwrap().0,
        UiTheme::default().palette.panel_bg.color(),
        "base panel starts on the default skin"
    );
    // Swap only the resource (the game-side re-skin seam); the theme
    // revision is part of the panel cache key, so it re-styles itself.
    app.insert_resource(theme::plan_a());
    app.update();
    let ui = theme::plan_a();
    let mut q = app
        .world_mut()
        .query_filtered::<&BackgroundColor, With<CommandPanel>>();
    assert_eq!(
        q.single(app.world()).unwrap().0,
        ui.palette.panel_bg.color(),
        "base command panel re-skinned to plan-a"
    );
    // The slot follows: enabled slot bg = plan-a panel_raised.
    use warbase::ui::CommandSlotButton;
    let mut q = app
        .world_mut()
        .query_filtered::<&BackgroundColor, With<CommandSlotButton>>();
    assert_eq!(
        q.single(app.world()).unwrap().0,
        ui.palette.panel_raised.color(),
        "base command slot re-skinned to plan-a"
    );
}