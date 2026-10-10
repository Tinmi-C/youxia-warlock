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

/// Reserved card key for the skill bar (commander skills — Z/X/C zone).
pub const SKILL_CARD: &str = "__skills__";

/// Selection states the panel reacts to (five-state model from warden UI9:
/// None → Build → Unit → Aiming → Casting — the base drives the first three;
/// aiming/casting overlays are game-side).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanelState {
    Hidden,
    BuildMenu,
    UnitSelected,
}

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

#[derive(Message, Debug, Clone)]
pub struct CommandCardAction {
    pub action: CommandAction,
    pub selection: SelectionMode,
}

/// Generate the build-menu card from a unit registry: defs with a build
/// cost that are NOT upgrade targets (tier-2/3 chains are reached via
/// upgrades, not the build menu), stable sorted order, hotkeys 1..9 QWER.
pub fn build_card_from_registry(registry: &UnitRegistry) -> CommandCardDef {
    let keys = [
        KeyCode::Digit1, KeyCode::Digit2, KeyCode::Digit3, KeyCode::Digit4, KeyCode::Digit5,
        KeyCode::Digit6, KeyCode::Digit7, KeyCode::Digit8, KeyCode::Digit9,
        KeyCode::KeyQ, KeyCode::KeyW, KeyCode::KeyE, KeyCode::KeyR,
    ];
    let upgrade_targets: std::collections::HashSet<&str> =
        registry.defs.values().filter_map(|d| d.upgrade_to.as_deref()).collect();
    let mut ids: Vec<&crate::unit::UnitDef> = registry
        .defs
        .values()
        .filter(|d| {
            (d.cost_gold > 0 || d.cost_wood > 0) && !upgrade_targets.contains(d.id.as_str())
        })
        .collect();
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

// ---------------------------------------------------------------------------
// Panel (BX4b): the bottom-bar face of the active card, rebuilt only when
// the active card key changes (cache-key; the CJK reflow pitfall). Logic is
// headless-testable — Node/Text components exist without a renderer; the
// visual pass is a human gate (H3) deferred with the rest of the art line.
// ---------------------------------------------------------------------------

#[derive(Component, Debug)]
pub struct CommandPanel;

#[derive(Resource, Debug, Default)]
struct PanelKey(Option<String>);

pub fn spawn_command_panel(mut commands: Commands) {
    commands.spawn((
        CommandPanel,
        Node {
            position_type: PositionType::Absolute,
            bottom: Val::Px(8.0),
            left: Val::Px(8.0),
            row_gap: Val::Px(4.0),
            display: Display::Flex,
            flex_direction: FlexDirection::Row,
            ..default()
        },
        BackgroundColor(Color::srgb(0.06, 0.07, 0.09)),
    ));
}

/// Rebuild the panel children when (and only when) the active card changes.
pub fn refresh_command_panel(
    world: &mut World,
) {
    let Some(key) = active_card_key(world) else {
        maybe_clear(world);
        return;
    };
    let changed = world.resource::<PanelKey>().0.as_ref() != Some(&key);
    if !changed {
        return;
    }
    let Some(card) = world.resource::<CommandCards>().cards.get(&key).cloned() else {
        maybe_clear(world);
        return;
    };
    world.resource_mut::<PanelKey>().0 = Some(key);

    let panel = world
        .query_filtered::<Entity, With<CommandPanel>>()
        .single(world)
        .expect("command panel spawned at startup");
    let mut cmds = world.entity_mut(panel);
    cmds.clear_children();
    // Collect children to despawn (clear_children unparents; despawn them).
    let children: Vec<Entity> = cmds.get::<Children>().map(|c| c.to_vec()).unwrap_or_default();
    for c in children {
        world.despawn(c);
    }
    let mut entity = world.get_entity_mut(panel).unwrap();
    for slot in &card.slots {
        let hotkey = match slot.hotkey {
            Hotkey::Key(k) => format!("{k:?}"),
            Hotkey::None => " ".to_string(),
        };
        entity.with_children(|p| {
            p.spawn((
                Node {
                    width: Val::Px(84.0),
                    height: Val::Px(56.0),
                    padding: UiRect::all(Val::Px(4.0)),
                    ..default()
                },
                BackgroundColor(Color::srgb(0.14, 0.16, 0.2)),
            ))
            .with_children(|slot_root| {
                slot_root.spawn((
                    Text::new(format!("[{hotkey}] {}", slot.label)),
                    TextFont { font_size: FontSize::Px(14.0), ..default() },
                    TextColor(Color::WHITE),
                ));
                slot_root.spawn((
                    Text::new(slot.tooltip.clone()),
                    TextFont { font_size: FontSize::Px(10.0), ..default() },
                    TextColor(Color::srgb(0.7, 0.7, 0.7)),
                ));
            });
        });
    }
}

fn maybe_clear(world: &mut World) {
    if world.resource_mut::<PanelKey>().0.take().is_some() {
        if let Ok(panel) = world
            .query_filtered::<Entity, With<CommandPanel>>()
            .single(world)
        {
            let children: Vec<Entity> = world
                .get::<Children>(panel)
                .map(|c| c.to_vec())
                .unwrap_or_default();
            for c in children {
                world.despawn(c);
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
            .init_resource::<PanelKey>()
            .add_systems(Startup, spawn_command_panel)
            .add_systems(Update, (command_input, refresh_command_panel));
    }
}
