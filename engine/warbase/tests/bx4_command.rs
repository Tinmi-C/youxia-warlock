//! BX4a acceptance: def-driven cards, hotkey resolution, dispatch messages.
//! WX3: the panel rendering layer — clickable slots, grey-out, grid
//! hotkeys/badges, hover tooltips, version-keyed rebuilds.

use bevy::prelude::*;
use warbase::prelude::*;
use warbase::testing::test_app;
use warbase::ui::{
    assign_grid_hotkeys, build_card_from_registry, grid_badge, CommandAction, CommandCardAction,
    CommandCards, CommandSlotButton, Hotkey, PanelVersion, Selection, SelectionMode, TooltipText,
    BUILD_CARD, GRID_KEYS,
};

fn registry_with(ids: &[(&str, u32)]) -> UnitRegistry {
    let mut defs = std::collections::HashMap::new();
    for (id, cost) in ids {
        defs.insert(
            id.to_string(),
            UnitDef {
                id: id.to_string(),
                attrs: BaseAttributes { hp: 80.0, move_speed: 0.0, body_radius: 0.4 },
                attack: None,
                armor_type: ArmorType::Fortified,
                cost_gold: *cost,
                cost_wood: 0,
                bounty_gold: 0, bounty_wood: 0, grant_xp: 0,
                traits: vec![], auras: vec![], hero: false, skills: vec![],
                upgrade_to: None, heal: None,
            },
        );
    }
    UnitRegistry { defs }
}

/// Card generation: only buildable defs, stable order, sequential hotkeys,
/// tooltips carry the cost.
#[test]
fn build_card_lists_buildables_in_order() {
    let reg = registry_with(&[("cannon", 80), ("arrow", 50), ("grunt", 0), ("mage", 70)]);
    let card = build_card_from_registry(&reg);
    let labels: Vec<&str> = card.slots.iter().map(|s| s.label.as_str()).collect();
    assert_eq!(labels, vec!["arrow", "cannon", "mage"], "sorted, grunt (no cost) excluded");
    assert_eq!(card.slots[0].hotkey, Hotkey::Key(KeyCode::Digit1));
    assert_eq!(card.slots[2].hotkey, Hotkey::Key(KeyCode::Digit3));
    assert!(card.slots[0].tooltip.contains("50g"));
    assert_eq!(card.slots[0].action, CommandAction::Build("arrow".into()));
}

/// Hotkey dispatch: build mode + registered card + key press -> action
/// message with the selection context.
#[test]
fn hotkey_press_dispatches_action() {
    let mut app = test_app();
    let reg = registry_with(&[("arrow", 50), ("cannon", 80)]);
    let mut cards = CommandCards::default();
    cards.cards.insert(BUILD_CARD.to_string(), build_card_from_registry(&reg));
    app.insert_resource(cards)
        .insert_resource(Selection { mode: SelectionMode::Build })
        .init_resource::<ButtonInput<KeyCode>>();
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::Digit2);

    app.update();
    let mut cursor = bevy::ecs::message::MessageCursor::<CommandCardAction>::default();
    let msgs = app.world().resource::<bevy::ecs::message::Messages<CommandCardAction>>();
    let actions: Vec<&CommandCardAction> = cursor.read(msgs).collect();
    assert_eq!(actions.len(), 1);
    assert_eq!(actions[0].action, CommandAction::Build("cannon".into()));
    assert_eq!(actions[0].selection, SelectionMode::Build);
}

/// Unit selection: the unit's own def card drives its slots (sell/upgrade).
#[test]
fn selected_unit_uses_its_def_card() {
    let mut app = test_app();
    let reg = registry_with(&[("arrow", 50)]);
    let tower = spawn_unit(&mut app.world_mut(), &reg, "arrow", Vec3::ZERO, Allegiance::Defender, 1.0).unwrap();
    let mut cards = CommandCards::default();
    cards.cards.insert(
        "arrow".into(),
        warbase::ui::CommandCardDef {
            slots: vec![
                warbase::ui::CommandSlotDef {
                    label: "sell".into(),
                    hotkey: Hotkey::Key(KeyCode::KeyX),
                    tooltip: "refund".into(),
                    action: CommandAction::Sell,
                    enabled: true,
                    badge: None,
                },
            ],
        },
    );
    app.insert_resource(cards)
        .insert_resource(Selection { mode: SelectionMode::Unit(tower) })
        .init_resource::<ButtonInput<KeyCode>>();
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::KeyX);

    app.update();
    let mut cursor = bevy::ecs::message::MessageCursor::<CommandCardAction>::default();
    let msgs = app.world().resource::<bevy::ecs::message::Messages<CommandCardAction>>();
    let actions: Vec<&CommandCardAction> = cursor.read(msgs).collect();
    assert_eq!(actions.len(), 1);
    assert_eq!(actions[0].action, CommandAction::Sell);
}

/// No selection => no card => key presses do nothing.
#[test]
fn no_selection_no_dispatch() {
    let mut app = test_app();
    app.insert_resource(Selection { mode: SelectionMode::None })
        .init_resource::<ButtonInput<KeyCode>>();
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::Digit1);
    app.update();
    let mut cursor = bevy::ecs::message::MessageCursor::<CommandCardAction>::default();
    let msgs = app.world().resource::<bevy::ecs::message::Messages<CommandCardAction>>();
    assert_eq!(cursor.read(msgs).count(), 0);
}

// --- WX3: panel rendering layer -----------------------------------------------------

/// A one-slot card app with the panel rendered (two updates: spawn + build).
/// `test_app` already carries UiPlugin via WarbasePlugin (ui feature).
fn panel_app(slot: warbase::ui::CommandSlotDef) -> App {
    let mut app = test_app();
    let mut cards = CommandCards::default();
    cards
        .cards
        .insert(BUILD_CARD.to_string(), warbase::ui::CommandCardDef { slots: vec![slot] });
    app.insert_resource(cards)
        .insert_resource(Selection { mode: SelectionMode::Build })
        .init_resource::<ButtonInput<KeyCode>>();
    app.update();
    app.update();
    app
}

fn rendered_slot(app: &mut App) -> Entity {
    app.world_mut()
        .query_filtered::<Entity, With<CommandSlotButton>>()
        .single(app.world())
        .expect("one rendered slot")
}

fn drained_actions(app: &mut App) -> Vec<CommandCardAction> {
    let mut cursor = bevy::ecs::message::MessageCursor::<CommandCardAction>::default();
    app.update();
    let msgs = app.world().resource::<bevy::ecs::message::Messages<CommandCardAction>>();
    cursor.read(msgs).cloned().collect()
}

fn one_slot(label: &str, enabled: bool) -> warbase::ui::CommandSlotDef {
    warbase::ui::CommandSlotDef {
        label: label.into(),
        hotkey: Hotkey::Key(KeyCode::KeyQ),
        tooltip: format!("{label}-tip"),
        action: CommandAction::Sell,
        enabled,
        badge: Some('Q'),
    }
}

/// WX3 AC1: a pressed panel slot dispatches exactly one action carrying the
/// current selection context — the click path IS the hotkey path.
#[test]
fn clicked_slot_dispatches_action() {
    let mut app = panel_app(one_slot("sell", true));
    let slot = rendered_slot(&mut app);
    app.world_mut().entity_mut(slot).insert(Interaction::Pressed);
    let actions = drained_actions(&mut app);
    assert_eq!(actions.len(), 1, "one action per press");
    assert_eq!(actions[0].action, CommandAction::Sell);
    assert_eq!(actions[0].selection, SelectionMode::Build);
}

/// WX3 AC2: a disabled slot renders grey and neither its click nor its
/// hotkey dispatches anything.
#[test]
fn disabled_slot_greys_and_blocks_dispatch() {
    let mut app = panel_app(one_slot("sell", false));
    let slot = rendered_slot(&mut app);
    let bg = app.world().get::<BackgroundColor>(slot).unwrap().0;
    assert_eq!(
        bg,
        Color::srgb(0.16, 0.16, 0.18),
        "disabled slot renders grey"
    );
    // Click path: blocked.
    app.world_mut().entity_mut(slot).insert(Interaction::Pressed);
    assert!(drained_actions(&mut app).is_empty(), "no click dispatch");
    // Hotkey path: blocked too.
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::KeyQ);
    assert!(drained_actions(&mut app).is_empty(), "no hotkey dispatch");
}

/// WX3 AC3: grid assignment fills QWER/ASDF hotkeys + badges in slot order;
/// slots beyond the 2x4 grid get neither.
#[test]
fn grid_hotkeys_and_badges_assign_in_order() {
    let mut card = warbase::ui::CommandCardDef {
        slots: (0..10)
            .map(|i| warbase::ui::CommandSlotDef {
                label: i.to_string(),
                hotkey: Hotkey::None,
                tooltip: String::new(),
                action: CommandAction::Sell,
                enabled: true,
                badge: None,
            })
            .collect(),
    };
    assign_grid_hotkeys(&mut card);
    for (i, expected) in GRID_KEYS.iter().enumerate() {
        assert_eq!(card.slots[i].hotkey, Hotkey::Key(*expected), "slot {i} hotkey");
        assert_eq!(card.slots[i].badge, Some(grid_badge(i as u8)), "slot {i} badge");
    }
    assert_eq!(card.slots[8].hotkey, Hotkey::None, "overflow: no hotkey");
    assert_eq!(card.slots[8].badge, None, "overflow: no badge");
    // The badge letters spell the grid (row-major QWER/ASDF).
    assert_eq!(
        (0..8).map(|i| grid_badge(i)).collect::<String>(),
        "QWERASDF"
    );
}

/// WX3 AC4: a badge char renders as a Text child of the slot.
#[test]
fn badge_renders_in_slot_corner() {
    let mut app = panel_app(one_slot("sell", true));
    let slot = rendered_slot(&mut app);
    let texts: Vec<String> = app
        .world()
        .get::<Children>(slot)
        .map(|c| {
            c.iter()
                .filter_map(|e| app.world().get::<Text>(e).map(|t| t.0.clone()))
                .collect()
        })
        .unwrap_or_default();
    assert!(texts.iter().any(|t| t == "Q"), "badge text present ({texts:?})");
}

/// WX3 AC5: hovering a slot spawns one tooltip with the slot's tooltip text;
/// leaving the hover despawns it.
#[test]
fn hover_shows_and_leaving_hides_the_tooltip() {
    let mut app = panel_app(one_slot("sell", true));
    let slot = rendered_slot(&mut app);
    let count_tooltips = |app: &mut App| {
        let mut q = app.world_mut().query_filtered::<Entity, With<TooltipText>>();
        q.iter(app.world()).count()
    };
    app.world_mut().entity_mut(slot).insert(Interaction::Hovered);
    app.update();
    assert_eq!(count_tooltips(&mut app), 1, "one tooltip while hovering");
    let text = {
        let mut q = app.world_mut().query_filtered::<&Text, With<TooltipText>>();
        q.single(app.world()).unwrap().0.clone()
    };
    assert_eq!(text, "sell-tip");
    app.world_mut().entity_mut(slot).insert(Interaction::None);
    app.update();
    assert_eq!(count_tooltips(&mut app), 0, "tooltip despawns on leave");
}

/// WX3 AC6: bumping PanelVersion rebuilds the panel from the (possibly
/// changed) card content without any selection churn.
#[test]
fn version_bump_rebuilds_the_panel() {
    let mut app = panel_app(one_slot("old", true));
    let count_label = |app: &mut App, needle: &str| {
        app.world_mut()
            .query_filtered::<&Text, Without<TooltipText>>()
            .iter(app.world())
            .filter(|t| t.0 == format!("[KeyQ] {needle}"))
            .count()
    };
    assert_eq!(count_label(&mut app, "old"), 1, "old label rendered");
    // Change the card content + bump the version.
    {
        let mut cards = app.world_mut().resource_mut::<CommandCards>();
        let card = cards.cards.get_mut(BUILD_CARD).unwrap();
        card.slots[0].label = "new".into();
    }
    app.world_mut().resource_mut::<PanelVersion>().0 += 1;
    app.update();
    assert_eq!(count_label(&mut app, "new"), 1, "rebuilt with the new label");
    assert_eq!(count_label(&mut app, "old"), 0, "old label gone");
}
