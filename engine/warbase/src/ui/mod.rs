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
    /// WX3: grey-out state — disabled slots render grey and neither their
    /// click nor their hotkey dispatches (games set it for combat locks,
    /// unaffordable entries...).
    pub enabled: bool,
    /// WX3: corner badge letter (the grid position's hotkey letter).
    pub badge: Option<char>,
}

#[derive(Debug, Clone, Default)]
pub struct CommandCardDef {
    pub slots: Vec<CommandSlotDef>,
}

/// Reserved card key for the build menu (shown when nothing is selected).
pub const BUILD_CARD: &str = "__build__";

/// Reserved card key for the skill bar (commander skills — Z/X/C zone).
pub const SKILL_CARD: &str = "__skills__";

/// The WC3 2x4 command grid, row-major: Q W E R / A S D F (WX3 lifted from
/// warden §16.7 — the canonical grid order for card hotkeys and badges).
pub const GRID_KEYS: [KeyCode; 8] = [
    KeyCode::KeyQ,
    KeyCode::KeyW,
    KeyCode::KeyE,
    KeyCode::KeyR,
    KeyCode::KeyA,
    KeyCode::KeyS,
    KeyCode::KeyD,
    KeyCode::KeyF,
];

/// Badge letter for a grid slot (row-major, mod 8).
pub fn grid_badge(grid: u8) -> char {
    "QWERASDF".as_bytes()[grid as usize % 8] as char
}

/// Fill hotkeys + corner badges in slot order (grid 0..8); slots beyond the
/// 2x4 grid get neither. Games wanting the 1..9QWER build-menu order keep
/// `build_card_from_registry`'s assignment instead.
pub fn assign_grid_hotkeys(card: &mut CommandCardDef) {
    for (i, slot) in card.slots.iter_mut().enumerate() {
        match GRID_KEYS.get(i) {
            Some(k) => {
                slot.hotkey = Hotkey::Key(*k);
                slot.badge = Some(grid_badge(i as u8));
            }
            None => {
                slot.hotkey = Hotkey::None;
                slot.badge = None;
            }
        }
    }
}

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
                enabled: true,
                badge: None,
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
            if !slot.enabled {
                continue; // WX3: a greyed slot's hotkey stays inert too
            }
            if keys.just_pressed(k) {
                world
                    .resource_mut::<bevy::ecs::message::Messages<CommandCardAction>>()
                    .write(CommandCardAction { action: slot.action.clone(), selection: selection.clone() });
            }
        }
    }
}

/// Marker on a rendered panel slot (WX3). Carries everything the click and
/// tooltip systems need without re-reading the card def.
#[derive(Component, Debug)]
pub struct CommandSlotButton {
    pub action: CommandAction,
    pub enabled: bool,
    pub tooltip: String,
}

/// WX3 click path: a pressed slot dispatches the same `CommandCardAction` a
/// hotkey press produces — one dispatch seam, so the two can never diverge.
pub fn command_clicks(
    q: Query<(&Interaction, &CommandSlotButton), Changed<Interaction>>,
    selection: Res<Selection>,
    mut actions: MessageWriter<CommandCardAction>,
) {
    for (inter, slot) in &q {
        if *inter == Interaction::Pressed && slot.enabled {
            actions.write(CommandCardAction {
                action: slot.action.clone(),
                selection: selection.mode.clone(),
            });
        }
    }
}

/// Marker on the hover tooltip's text (WX3).
#[derive(Component, Debug)]
pub struct TooltipText;

/// WX3 tooltip: hovering a slot shows its def-provided tooltip string in a
/// panel above the bar; leaving despawns it. Content is the game's business
/// (labels/tooltips arrive localized in the card def); the base only owns
/// the show/hide mechanics.
pub fn refresh_tooltip(
    mut commands: Commands,
    slots: Query<(&Interaction, &CommandSlotButton)>,
    existing: Query<Entity, With<TooltipText>>,
    mut cache: Local<Option<String>>,
) {
    let hovered = slots
        .iter()
        .find(|(inter, _)| **inter == Interaction::Hovered)
        .map(|(_, b)| b.tooltip.clone());
    if *cache == hovered {
        return;
    }
    *cache = hovered.clone();
    for e in &existing {
        commands.entity(e).despawn();
    }
    if let Some(text) = hovered {
        commands.spawn((
            TooltipText,
            Text::new(text),
            TextFont { font_size: FontSize::Px(11.0), ..default() },
            TextColor(Color::srgb(0.85, 0.88, 0.92)),
            Node {
                position_type: PositionType::Absolute,
                bottom: Val::Px(72.0), // above the panel's slot row
                left: Val::Px(8.0),
                padding: UiRect::all(Val::Px(6.0)),
                ..default()
            },
            BackgroundColor(Color::srgb(0.05, 0.06, 0.08)),
        ));
    }
}

// ---------------------------------------------------------------------------
// Panel (BX4b + WX3): the bottom-bar face of the active card, rebuilt only
// when the active card key OR the game-bumped version changes (cache-key;
// the CJK reflow pitfall). Slots are real buttons (WX3): clicks dispatch the
// same actions hotkeys do, disabled slots render grey, badges print in the
// corner, hovering shows the def's tooltip. Logic is headless-testable —
// Node/Text components exist without a renderer; the visual pass is a human
// gate (H3) deferred with the rest of the art line.
// ---------------------------------------------------------------------------

#[derive(Component, Debug)]
pub struct CommandPanel;

#[derive(Resource, Debug, Default)]
struct PanelKey(Option<String>, u64);

/// WX3: games bump this when the card CONTENT changed without the active
/// key changing (language toggle, economy affordability greying...) — the
/// panel rebuilds on the next update.
#[derive(Resource, Debug, Default)]
pub struct PanelVersion(pub u64);

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

/// Rebuild the panel children when (and only when) the active card or the
/// game-bumped version changes.
pub fn refresh_command_panel(
    world: &mut World,
) {
    let version = world.resource::<PanelVersion>().0;
    let Some(key) = active_card_key(world) else {
        maybe_clear(world);
        return;
    };
    let (last_key, last_version) = {
        let pk = world.resource::<PanelKey>();
        (pk.0.clone(), pk.1)
    };
    if last_key == Some(key.clone()) && last_version == version {
        return;
    }
    let Some(card) = world.resource::<CommandCards>().cards.get(&key).cloned() else {
        maybe_clear(world);
        return;
    };
    world.resource_mut::<PanelKey>().0 = Some(key);
    world.resource_mut::<PanelKey>().1 = version;

    let panel = world
        .query_filtered::<Entity, With<CommandPanel>>()
        .single(world)
        .expect("command panel spawned at startup");
    // Despawn the previous slots (bottom-up subtree walk — a slot's Texts
    // are grandchildren the shallow despawn helpers would orphan).
    let prev: Vec<Entity> = world
        .get::<Children>(panel)
        .map(|c| c.to_vec())
        .unwrap_or_default();
    for slot in prev {
        despawn_subtree(world, slot);
    }
    let mut entity = world.get_entity_mut(panel).unwrap();
    for slot in &card.slots {
        let hotkey = match slot.hotkey {
            Hotkey::Key(k) => format!("{k:?}"),
            Hotkey::None => " ".to_string(),
        };
        // WX3: disabled slots render grey (combat locks, affordability).
        let (bg, fg) = if slot.enabled {
            (Color::srgb(0.14, 0.16, 0.2), Color::WHITE)
        } else {
            (Color::srgb(0.16, 0.16, 0.18), Color::srgb(0.45, 0.45, 0.45))
        };
        entity.with_children(|p| {
            p.spawn((
                Button,
                CommandSlotButton {
                    action: slot.action.clone(),
                    enabled: slot.enabled,
                    tooltip: slot.tooltip.clone(),
                },
                Interaction::None,
                Node {
                    width: Val::Px(84.0),
                    height: Val::Px(56.0),
                    padding: UiRect::all(Val::Px(4.0)),
                    ..default()
                },
                BackgroundColor(bg),
            ))
            .with_children(|slot_root| {
                slot_root.spawn((
                    Text::new(format!("[{hotkey}] {}", slot.label)),
                    TextFont { font_size: FontSize::Px(14.0), ..default() },
                    TextColor(fg),
                ));
                slot_root.spawn((
                    Text::new(slot.tooltip.clone()),
                    TextFont { font_size: FontSize::Px(10.0), ..default() },
                    TextColor(Color::srgb(0.7, 0.7, 0.7)),
                ));
                // WX3: grid badge in the corner (when the card assigns one).
                if let Some(badge) = slot.badge {
                    slot_root.spawn((
                        Text::new(badge.to_string()),
                        TextFont { font_size: FontSize::Px(9.0), ..default() },
                        TextColor(Color::srgb(0.6, 0.8, 1.0)),
                        Node {
                            position_type: PositionType::Absolute,
                            top: Val::Px(2.0),
                            right: Val::Px(4.0),
                            ..default()
                        },
                    ));
                }
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
                despawn_subtree(world, c);
            }
        }
    }
}

/// Despawn an entity and everything below it in the UI tree.
fn despawn_subtree(world: &mut World, root: Entity) {
    let children: Vec<Entity> = world
        .get::<Children>(root)
        .map(|c| c.to_vec())
        .unwrap_or_default();
    for c in children {
        despawn_subtree(world, c);
    }
    let _ = world.entity_mut(root).despawn();
}

pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<CommandCardAction>()
            .init_resource::<CommandCards>()
            .init_resource::<Selection>()
            .init_resource::<PanelKey>()
            .init_resource::<PanelVersion>()
            .add_systems(Startup, spawn_command_panel)
            .add_systems(
                Update,
                (
                    command_input,
                    command_clicks,
                    refresh_command_panel,
                    refresh_tooltip,
                ),
            );
    }
}
