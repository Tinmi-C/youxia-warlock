//! BX4a acceptance: def-driven cards, hotkey resolution, dispatch messages.

use bevy::prelude::*;
use warbase::prelude::*;
use warbase::testing::test_app;
use warbase::ui::{build_card_from_registry, CommandAction, CommandCardAction, CommandCards, Hotkey, Selection, SelectionMode, BUILD_CARD};

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
