//! Command card (BX4a headless core): def-driven card faces + hotkey
//! resolution + action dispatch as messages. Rendering panels and the warden
//! migration (WX3) build on this. Kept UI-free so it stays headless-testable
//! under the "ui" feature.

use bevy::prelude::*;
use std::collections::HashMap;

use crate::unit::{UnitInstance, UnitRegistry};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Hotkey {
    Key(KeyCode),
    None,
}

/// What a slot does. `Build` carries the unit def id; `Cast` carries the
/// SkillCaster slot index; Upgrade/Sell apply to the selected buildable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandAction {
    Build(String),
    Upgrade,
    Sell,
    Cast(usize),
}

#[derive(Debug, Clone)]
pub struct CommandSlotDef {
    pub label: String,
    pub hotkey: Hotkey,
    pub tooltip: String,
    pub action: CommandAction,
}

#[derive(Debug, Clone, Default)]
pub struct CommandCardDef {
    pub slots: Vec<CommandSlotDef>,
}

/// Reserved card key for the build menu (shown when nothing is selected).
pub const BUILD_CARD: &str = "__build__";

#[derive(Resource, Debug, Default)]
pub struct CommandCards {
    pub cards: HashMap<String, CommandCardDef>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SelectionMode {
    None,
    Build,
    Unit(Entity),
}

impl Default for SelectionMode {
    fn default() -> Self {
        SelectionMode::None
    }
}

#[derive(Resource, Debug, Default)]
pub struct Selection {
    pub mode: SelectionMode,
}

#[derive(Message, Debug)]
pub struct CommandCardAction {
    pub action: CommandAction,
    pub selection: SelectionMode,
}

/// Generate the build-menu card from a unit registry: every def with a
/// build cost, stable sorted order, hotkeys 1..9 then Q W E R.
pub fn build_card_from_registry(registry: &UnitRegistry) -> CommandCardDef {
    let keys = [
        KeyCode::Digit1, KeyCode::Digit2, KeyCode::Digit3, KeyCode::Digit4, KeyCode::Digit5,
        KeyCode::Digit6, KeyCode::Digit7, KeyCode::Digit8, KeyCode::Digit9,
        KeyCode::KeyQ, KeyCode::KeyW, KeyCode::KeyE, KeyCode::KeyR,
    ];
    let mut ids: Vec<&crate::unit::UnitDef> =
        registry.defs.values().filter(|d| d.cost_gold > 0 || d.cost_wood > 0).collect();
    ids.sort_by(|a, b| a.id.cmp(&b.id));
    CommandCardDef {
        slots: ids
            .iter()
            .enumerate()
            .map(|(i, def)| CommandSlotDef {
                label: def.id.clone(),
                hotkey: keys.get(i).copied().map(Hotkey::Key).unwrap_or(Hotkey::None),
                tooltip: format!("{} ({}g/{}w)", def.id, def.cost_gold, def.cost_wood),
                action: CommandAction::Build(def.id.clone()),
            })
            .collect(),
    }
}

/// Which card is active for the current selection: the build menu when in
/// build mode, the unit's own def card when a unit is selected.
pub fn active_card_key(world: &World) -> Option<String> {
    match world.resource::<Selection>().mode {
        SelectionMode::None => None,
        SelectionMode::Build => Some(BUILD_CARD.to_string()),
        SelectionMode::Unit(e) => world.get::<UnitInstance>(e).map(|i| i.def.clone()),
    }
}

/// Update (exclusive): resolve hotkey presses on the active card into
/// CommandCardAction messages; the game side dispatches them (place/sell/
/// upgrade/cast live in game logic or base helpers).
pub fn command_input(world: &mut World) {
    // Headless apps may not carry an input resource — the card is inert there.
    if !world.contains_resource::<ButtonInput<KeyCode>>() {
        return;
    }
    let keys: ButtonInput<KeyCode> = world.resource::<ButtonInput<KeyCode>>().clone();
    let Some(card_key) = active_card_key(world) else { return };
    let Some(card) = world.resource::<CommandCards>().cards.get(&card_key).cloned() else {
        return;
    };
    let selection = world.resource::<Selection>().mode.clone();
    for slot in &card.slots {
        if let Hotkey::Key(k) = slot.hotkey {
            if keys.just_pressed(k) {
                world
                    .resource_mut::<bevy::ecs::message::Messages<CommandCardAction>>()
                    .write(CommandCardAction { action: slot.action.clone(), selection: selection.clone() });
            }
        }
    }
}

pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<CommandCardAction>()
            .init_resource::<CommandCards>()
            .init_resource::<Selection>()
            .add_systems(Update, command_input);
    }
}
