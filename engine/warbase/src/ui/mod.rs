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
/// `Custom` (SEAM3b registry seam) carries a game-owned action id: the base
/// card framework (def/hotkey/dispatch/enabled) is generic and games
/// register their own entries without engine vocabulary. Chosen over a
/// registration-map resource because the break surface is one match arm —
/// exhaustive consumers just grow a `Custom(_)` case.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum CommandAction {
    Build(String),
    Upgrade,
    Sell,
    Cast(usize),
    /// Game-registered action id (e.g. loop-td's "dig"/"challenge"/
    /// "artifact"); the game dispatches it, the base only carries it.
    Custom(String),
}

impl CommandAction {
    /// Stable replay-envelope word (SEAM3c): "build:<id>" / "upgrade" /
    /// "sell" / "cast:<slot>" / "custom:<id>". Entity-free by construction;
    /// the codec lives with the vocabulary it encodes.
    pub fn envelope_word(&self) -> String {
        match self {
            CommandAction::Build(id) => format!("build:{id}"),
            CommandAction::Upgrade => "upgrade".into(),
            CommandAction::Sell => "sell".into(),
            CommandAction::Cast(slot) => format!("cast:{slot}"),
            CommandAction::Custom(id) => format!("custom:{id}"),
        }
    }

    /// Decode an envelope word; None on anything this vocabulary never
    /// wrote (the replayer surfaces it as a loud error, never silence).
    pub fn from_envelope_word(word: &str) -> Option<Self> {
        let (head, tail) = match word.split_once(':') {
            Some((h, t)) => (h, Some(t)),
            None => (word, None),
        };
        match (head, tail) {
            ("build", Some(id)) => Some(CommandAction::Build(id.to_string())),
            ("upgrade", None) => Some(CommandAction::Upgrade),
            ("sell", None) => Some(CommandAction::Sell),
            ("cast", Some(slot)) => slot.parse::<usize>().ok().map(CommandAction::Cast),
            ("custom", Some(id)) => Some(CommandAction::Custom(id.to_string())),
            _ => None,
        }
    }
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

// ---------------------------------------------------------------------------
// UI-DSGN theme tokens: every visual constant the card/panel/tooltip render
// is a field on `UiTheme` (serde, data-only). Re-skinning a game = inserting
// a different resource — zero rendering-code change (asserted by tests).
// Field defaults are the legacy hard-coded values, digit for digit, so the
// existing rendering tests lock zero drift. Design reference:
// docs/ui-design-brief.md §5 (the token tables ARE these fields).
// ---------------------------------------------------------------------------

/// One color token: sRGBA components in 0..=1 (serde-stable, engine-free).
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ThemeColor {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    /// Alpha defaults to opaque so 3-component skin colors parse.
    #[serde(default = "opaque_alpha")]
    pub a: f32,
}

fn opaque_alpha() -> f32 {
    1.0
}

impl ThemeColor {
    pub const fn rgb(r: f32, g: f32, b: f32) -> Self {
        Self { r, g, b, a: 1.0 }
    }
    pub const fn rgba(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }
    /// The bevy color this token renders as.
    pub fn color(&self) -> Color {
        Color::srgba(self.r, self.g, self.b, self.a)
    }
}

/// Structural neutrals + the one accent (brief §5.1).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct UiPalette {
    pub panel_bg: ThemeColor,
    pub panel_raised: ThemeColor,
    pub panel_border: ThemeColor,
    pub text_primary: ThemeColor,
    pub text_secondary: ThemeColor,
    pub text_dim: ThemeColor,
    pub accent: ThemeColor,
}

impl Default for UiPalette {
    fn default() -> Self {
        Self {
            panel_bg: ThemeColor::rgb(0.06, 0.07, 0.09),
            panel_raised: ThemeColor::rgb(0.14, 0.16, 0.20),
            panel_border: ThemeColor::rgb(0.35, 0.38, 0.44),
            text_primary: ThemeColor::rgb(1.0, 1.0, 1.0),
            text_secondary: ThemeColor::rgb(0.7, 0.7, 0.7),
            text_dim: ThemeColor::rgb(0.45, 0.45, 0.45),
            accent: ThemeColor::rgb(0.6, 0.8, 1.0),
        }
    }
}

/// Semantic + interaction states (brief §5.2). WC3 color grammar:
/// gold/wood resources, green health, blue mana, red enemy/danger,
/// yellow warning — shared across every game on the base.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct UiStateColors {
    pub slot_disabled_bg: ThemeColor,
    pub tooltip_bg: ThemeColor,
    pub tooltip_fg: ThemeColor,
    pub gold: ThemeColor,
    pub wood: ThemeColor,
    pub health: ThemeColor,
    pub health_low: ThemeColor,
    pub mana: ThemeColor,
    pub warning: ThemeColor,
    pub enemy: ThemeColor,
    pub xp: ThemeColor,
    pub cooldown: ThemeColor,
}

impl Default for UiStateColors {
    fn default() -> Self {
        Self {
            slot_disabled_bg: ThemeColor::rgb(0.16, 0.16, 0.18),
            tooltip_bg: ThemeColor::rgb(0.05, 0.06, 0.08),
            tooltip_fg: ThemeColor::rgb(0.85, 0.88, 0.92),
            gold: ThemeColor::rgb(1.0, 0.84, 0.0),
            wood: ThemeColor::rgb(0.55, 0.36, 0.18),
            health: ThemeColor::rgb(0.3, 0.85, 0.3),
            health_low: ThemeColor::rgb(1.0, 0.25, 0.15),
            mana: ThemeColor::rgb(0.25, 0.5, 1.0),
            warning: ThemeColor::rgb(1.0, 0.8, 0.2),
            enemy: ThemeColor::rgb(0.85, 0.2, 0.2),
            xp: ThemeColor::rgb(0.6, 0.8, 1.0),
            cooldown: ThemeColor::rgba(0.0, 0.0, 0.0, 0.55),
        }
    }
}

/// Four-step type scale + component sizes (brief §5.3), plus the display
/// tier (SEAM3b: end-screen banners / victory text — warden's hard-coded
/// 34px moved here so skins own the whole ladder).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct UiTypography {
    /// Value digits (gold / stock / wave numbers).
    pub font_value: f32,
    /// Slot labels, panel titles use `font_title`.
    pub font_label: f32,
    /// Captions, sub-lines, counts.
    pub font_caption: f32,
    /// Hotkey badges.
    pub font_badge: f32,
    /// Tooltip main line (legacy size).
    pub font_tooltip: f32,
    /// Panel headers.
    pub font_title: f32,
    /// Display tier (SEAM3b): end-screen banners and hero numbers. Default
    /// = warden's boarded 34px literal, digit for digit (zero visual drift).
    pub font_display: f32,
}

impl Default for UiTypography {
    fn default() -> Self {
        Self {
            font_value: 20.0,
            font_label: 14.0,
            font_caption: 10.0,
            font_badge: 9.0,
            font_tooltip: 11.0,
            font_title: 16.0,
            font_display: 34.0,
        }
    }
}

/// Spacing constants (brief §5.4); defaults = legacy layout values.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct UiSpacing {
    pub panel_margin: f32,
    pub panel_gap: f32,
    pub slot_w: f32,
    pub slot_h: f32,
    pub slot_pad: f32,
    pub panel_border_w: f32,
    pub slot_border_w: f32,
    pub tooltip_bottom: f32,
    pub tooltip_left: f32,
    pub tooltip_pad: f32,
    pub badge_top: f32,
    pub badge_right: f32,
    pub bar_h: f32,
    pub icon_size: f32,
}

impl Default for UiSpacing {
    fn default() -> Self {
        Self {
            panel_margin: 8.0,
            panel_gap: 4.0,
            slot_w: 84.0,
            slot_h: 56.0,
            slot_pad: 4.0,
            panel_border_w: 0.0,
            slot_border_w: 0.0,
            tooltip_bottom: 72.0,
            tooltip_left: 8.0,
            tooltip_pad: 6.0,
            badge_top: 2.0,
            badge_right: 4.0,
            bar_h: 10.0,
            icon_size: 14.0,
        }
    }
}

/// Corner radii, px (brief §5.4); 0 = right angles (legacy look).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct UiRadius {
    pub panel: f32,
    pub slot: f32,
    pub pill: f32,
}

impl Default for UiRadius {
    fn default() -> Self {
        Self { panel: 0.0, slot: 0.0, pill: 999.0 }
    }
}

/// The whole skin. Games insert/replace this resource to re-skin; bump
/// `revision` so keyed rebuilds (command panel) pick the new tokens up.
#[derive(Debug, Clone, PartialEq, Default, serde::Serialize, serde::Deserialize, Resource)]
#[serde(default)]
pub struct UiTheme {
    pub revision: u64,
    pub palette: UiPalette,
    pub typography: UiTypography,
    pub spacing: UiSpacing,
    pub radius: UiRadius,
    pub state_colors: UiStateColors,
}

impl UiTheme {
    /// A helper for skins defined as RON data files.
    pub fn from_ron(s: &str) -> ron::error::SpannedResult<Self> {
        ron::from_str(s)
    }
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

/// SEAM3c recorder extension: every dispatched card action lands in the
/// input log as a `Card` envelope word (selection is dropped — the
/// replayer re-derives it, keeping the envelope Entity-free). No-op
/// without a `RecordingLog` (one lookup, zero drift).
pub fn record_card_actions(
    mut reader: MessageReader<CommandCardAction>,
    log: Option<ResMut<crate::replay::RecordingLog>>,
) {
    let Some(mut log) = log else { return };
    for msg in reader.read() {
        crate::replay::append_envelope(
            &mut log,
            crate::replay::CommandEnvelope::Card { action: msg.action.envelope_word() },
        );
    }
}

/// Marker on the hover tooltip's text (WX3).
#[derive(Component, Debug)]
pub struct TooltipText;

/// The font face base-rendered UI text uses (MMP tooltip CJK seam): `None`
/// (the default) = bevy's default font — zero drift for apps that never
/// wire one; `Some(handle)` swaps the tooltip onto that face (bevy's default
/// font ships no CJK glyphs — Chinese tooltips render as tofu without
/// this). Games load the shared Noto Sans SC once at Startup and insert this
/// resource (warden/loop-td `setup_ui_font`, rts-lab minimap wiring); the
/// same one-handle pattern their own panels already use.
#[derive(Resource, Debug, Default, Clone)]
pub struct UiFont(pub Option<Handle<Font>>);

// ---------------------------------------------------------------------------
// SEAM4b / UITEX (round-4 plan §Phase 1): the art-texture seam. Three
// pieces, mirroring the UiTheme/UiFont shapes:
//   `UiTextureManifest` — serde data (RON): slot id -> texture file path
//                         relative to the manifest file. Deliberately NOT
//                         nested in `UiTheme`: the skin is pure token data
//                         (colors/sizes, one revision), while textures are
//                         ASSET references (ADR-0009 art-sync territory)
//                         with their own load/revision lifecycle — greybox
//                         apps never carry asset paths in their skin.
//   `UiTextures`        — the runtime companion resource (the UiFont
//                         pattern): loaded `Handle<Image>`s + a `revision`
//                         bumped per load, so keyed rebuilds pick swaps up.
//   `ArtMode`           — the global switch. Default `false` = greybox,
//                         ZERO DRIFT: surfaces keep exactly the legacy
//                         BackgroundColor+BorderColor and carry no
//                         ImageNode at all (bevy_ui stacks IMAGE above
//                         BORDER, so an always-on solid ImageNode would
//                         paint over the theme's border ring).
// ---------------------------------------------------------------------------

/// Manifest slot id: panel bases (the command-panel root, Plan-A roots).
pub const UI_TEX_PANEL: &str = "panel";
/// Manifest slot id: slot faces (command slots, equip-rack rows).
pub const UI_TEX_SLOT: &str = "slot";
// TODO(UITEX/DRILL): icon surfaces key off their own slot ids and size to
// the theme's `spacing.icon_size` anchor (icon_size square, center-fit,
// unscaled) — landing with the DRILL increment that walks real art through
// the seam (round-4 plan §Phase 2).

/// RON-declarable texture manifest: `slot id -> PNG path` relative to the
/// manifest file's directory. Data-only like `UiTheme`; the game's `--art`
/// boot path reads it once and injects the runtime `UiTextures`.
#[derive(
    Debug, Clone, PartialEq, Default, Resource, serde::Serialize, serde::Deserialize,
)]
#[serde(default)]
pub struct UiTextureManifest {
    pub textures: HashMap<String, String>,
}

impl UiTextureManifest {
    /// A helper for manifests defined as RON data files.
    pub fn from_ron(s: &str) -> ron::error::SpannedResult<Self> {
        ron::from_str(s)
    }
}

/// The loaded UI texture set (runtime half of the seam — the UiFont
/// companion-resource pattern). `revision` must change whenever the
/// handles do; keyed panel rebuilds fold it into their cache keys so a
/// texture swap re-skins without any code change.
#[derive(Resource, Debug, Default, Clone)]
pub struct UiTextures {
    pub handles: HashMap<String, Handle<Image>>,
    pub revision: u64,
}

/// Global art-mode switch (SEAM4b): `false` (the default) = greybox —
/// panel bases render exactly the legacy token components, zero drift;
/// `true` = textured surfaces wherever `UiTextures` carries the slot's
/// handle (missing slots degrade per-surface, see `art_surface`).
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ArtMode(pub bool);

/// Cache-key input for the art seam: the texture revision packed with the
/// mode flag — a reload (revision bump) OR a mode flip re-skins every
/// keyed panel surface.
pub fn art_revision(textures: &UiTextures, art: &ArtMode) -> u64 {
    (textures.revision << 1) | art.0 as u64
}

/// Nine-slice config for a panel-base texture: insets come from the
/// surface's border TOKEN (textures are authored so their border ring is
/// exactly that many source pixels thick), corners stay unscaled
/// (`max_corner_scale: 1.0` — TextureSlicer semantics), center and sides
/// stretch.
fn nine_slice(border_w: f32) -> TextureSlicer {
    TextureSlicer {
        border: BorderRect::all(border_w),
        center_scale_mode: SliceScaleMode::Stretch,
        sides_scale_mode: SliceScaleMode::Stretch,
        max_corner_scale: 1.0,
    }
}

/// The `ImageNode` for one panel-base surface, or `None` while greybox
/// (zero drift: no ImageNode at all — BackgroundColor/BorderColor keep
/// rendering alone, border ring included). In art mode: a loaded handle
/// renders nine-sliced (`border_w` sets the slice insets); a slot without
/// a handle degrades to `ImageNode::solid_color(fallback)` — visible and
/// token-colored, never a panic (the miss is a load-time warn, not a
/// runtime fault).
pub fn art_surface(
    art: &ArtMode,
    textures: &UiTextures,
    slot: &str,
    fallback: Color,
    border_w: f32,
) -> Option<ImageNode> {
    if !art.0 {
        return None;
    }
    Some(match textures.handles.get(slot) {
        Some(handle) => ImageNode::new(handle.clone())
            .with_mode(NodeImageMode::Sliced(nine_slice(border_w))),
        None => ImageNode::solid_color(fallback).with_mode(NodeImageMode::Stretch),
    })
}

/// What one `load_ui_textures` run produced (the data behind its warns —
/// games and tests decide the fallback on facts, not log scraping).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct UiTextureLoadReport {
    /// Slot ids that landed in `UiTextures`.
    pub loaded: Vec<String>,
    /// Slot ids whose file was missing/undecodable (greybox for them).
    pub failed: Vec<String>,
    /// False when the manifest itself was unreadable or unparseable.
    pub manifest_ok: bool,
}

/// Load a manifest's PNGs into `Assets<Image>` and (re)inject the
/// `UiTextures` resource with a bumped revision. Every failure mode
/// degrades to a warn + report — this NEVER panics (typo'd paths and
/// bare-headless apps must not take the game down); a failed slot is
/// dropped from the handles so the per-surface fallback takes over.
pub fn load_ui_textures(
    world: &mut World,
    manifest_path: &std::path::Path,
) -> UiTextureLoadReport {
    use bevy::asset::RenderAssetUsages;
    use bevy::image::{CompressedImageFormats, ImageSampler, ImageType};

    let mut report = UiTextureLoadReport::default();
    let bytes = match std::fs::read(manifest_path) {
        Ok(b) => b,
        Err(e) => {
            warn!(
                "[uitex] manifest {} unreadable ({e}) — greybox fallback",
                manifest_path.display()
            );
            return report; // manifest_ok stays false
        }
    };
    let manifest = match UiTextureManifest::from_ron(&String::from_utf8_lossy(&bytes)) {
        Ok(m) => m,
        Err(e) => {
            warn!(
                "[uitex] manifest {} unparseable ({e}) — greybox fallback",
                manifest_path.display()
            );
            return report;
        }
    };
    report.manifest_ok = true;
    if !world.contains_resource::<Assets<Image>>() {
        // Bare-headless app (no render plugins): an empty store still
        // hosts the CPU-side images, keeping the seam testable.
        world.init_resource::<Assets<Image>>();
    }
    let base = manifest_path.parent().unwrap_or(std::path::Path::new(".")).to_path_buf();
    let mut handles = HashMap::new();
    for (slot, rel) in &manifest.textures {
        let path = base.join(rel);
        let decoded = std::fs::read(&path)
            .map_err(|e| e.to_string())
            .and_then(|b| {
                Image::from_buffer(
                    &b,
                    ImageType::Extension("png"),
                    CompressedImageFormats::empty(),
                    true,
                    ImageSampler::default(),
                    RenderAssetUsages::default(),
                )
                .map_err(|e| e.to_string())
            });
        match decoded {
            Ok(image) => {
                let handle = world.resource_mut::<Assets<Image>>().add(image);
                handles.insert(slot.clone(), handle);
                report.loaded.push(slot.clone());
            }
            Err(e) => {
                warn!(
                    "[uitex] texture slot '{slot}' ({}) failed: {e} — greybox for that slot",
                    path.display()
                );
                report.failed.push(slot.clone());
            }
        }
    }
    let revision = world.get_resource::<UiTextures>().map(|t| t.revision).unwrap_or(0) + 1;
    world.insert_resource(UiTextures { handles, revision });
    report
}

/// WX3 tooltip: hovering a slot shows its def-provided tooltip string in a
/// panel above the bar; leaving despawns it. Content is the game's business
/// (labels/tooltips arrive localized in the card def); the base only owns
/// the show/hide mechanics. Styling reads the theme tokens (UI-DSGN) inside
/// the queued spawn — signature stays `Commands`-only (system contract).
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
        commands.queue(move |world: &mut World| {
            let theme = world.resource::<UiTheme>().clone();
            // MMP: the tooltip's face follows the UiFont seam — None keeps
            // `TextFont::default()`'s font source exactly (zero drift).
            let font = match world.resource::<UiFont>().0.clone() {
                Some(handle) => bevy::text::FontSource::Handle(handle),
                None => bevy::text::FontSource::default(),
            };
            let s = &theme.spacing;
            world.spawn((
                TooltipText,
                Text::new(text),
                TextFont {
                    font,
                    font_size: FontSize::Px(theme.typography.font_tooltip),
                    ..default()
                },
                TextColor(theme.state_colors.tooltip_fg.color()),
                Node {
                    position_type: PositionType::Absolute,
                    bottom: Val::Px(s.tooltip_bottom), // above the panel's slot row
                    left: Val::Px(s.tooltip_left),
                    padding: UiRect::all(Val::Px(s.tooltip_pad)),
                    border: UiRect::all(Val::Px(theme.spacing.panel_border_w)),
                    border_radius: BorderRadius::all(Val::Px(theme.radius.panel)),
                    ..default()
                },
                BackgroundColor(theme.state_colors.tooltip_bg.color()),
                BorderColor::all(theme.palette.panel_border.color()),
            ));
        });
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
struct PanelKey(Option<String>, u64, u64);

/// WX3: games bump this when the card CONTENT changed without the active
/// key changing (language toggle, economy affordability greying...) — the
/// panel rebuilds on the next update.
#[derive(Resource, Debug, Default)]
pub struct PanelVersion(pub u64);

pub fn spawn_command_panel(mut commands: Commands) {
    // Theme read inside the queued spawn: the system signature (the
    // UI-DSGN contract) stays `Commands`-only.
    commands.queue(|world: &mut World| {
        let theme = world.resource::<UiTheme>().clone();
        let s = &theme.spacing;
        world.spawn((
            CommandPanel,
            Node {
                position_type: PositionType::Absolute,
                bottom: Val::Px(s.panel_margin),
                left: Val::Px(s.panel_margin),
                row_gap: Val::Px(s.panel_gap),
                display: Display::Flex,
                flex_direction: FlexDirection::Row,
                border: UiRect::all(Val::Px(s.panel_border_w)),
                border_radius: BorderRadius::all(Val::Px(theme.radius.panel)),
                ..default()
            },
            BackgroundColor(theme.palette.panel_bg.color()),
            BorderColor::all(theme.palette.panel_border.color()),
        ));
    });
}

/// Rebuild the panel children when (and only when) the active card, the
/// game-bumped version, or the theme revision changes (UI-DSGN: swapping
/// the `UiTheme` resource re-skins without any code change).
pub fn refresh_command_panel(
    world: &mut World,
) {
    let version = world.resource::<PanelVersion>().0;
    let theme_rev = world.resource::<UiTheme>().revision;
    let Some(key) = active_card_key(world) else {
        maybe_clear(world);
        return;
    };
    let (last_key, last_version, last_theme) = {
        let pk = world.resource::<PanelKey>();
        (pk.0.clone(), pk.1, pk.2)
    };
    if last_key == Some(key.clone()) && last_version == version && last_theme == theme_rev {
        return;
    }
    let Some(card) = world.resource::<CommandCards>().cards.get(&key).cloned() else {
        maybe_clear(world);
        return;
    };
    world.resource_mut::<PanelKey>().0 = Some(key);
    world.resource_mut::<PanelKey>().1 = version;
    world.resource_mut::<PanelKey>().2 = theme_rev;

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
    let theme = world.resource::<UiTheme>().clone();
    // Re-skin the panel root itself (background/border/position/radius all
    // come from the theme — swapping the resource moves the whole panel).
    {
        let s = &theme.spacing;
        let mut root = world.get_entity_mut(panel).unwrap();
        if let Some(mut bg) = root.get_mut::<BackgroundColor>() {
            *bg = BackgroundColor(theme.palette.panel_bg.color());
        }
        if let Some(mut bc) = root.get_mut::<BorderColor>() {
            *bc = BorderColor::all(theme.palette.panel_border.color());
        }
        if let Some(mut node) = root.get_mut::<Node>() {
            node.bottom = Val::Px(s.panel_margin);
            node.left = Val::Px(s.panel_margin);
            node.row_gap = Val::Px(s.panel_gap);
            node.border = UiRect::all(Val::Px(s.panel_border_w));
            node.border_radius = BorderRadius::all(Val::Px(theme.radius.panel));
        }
    }
    let t = &theme.typography;
    let s = &theme.spacing;
    let mut entity = world.get_entity_mut(panel).unwrap();
    for slot in &card.slots {
        let hotkey = match slot.hotkey {
            Hotkey::Key(k) => format!("{k:?}"),
            Hotkey::None => " ".to_string(),
        };
        // WX3 + UI-DSGN: disabled slots render in the theme's disabled
        // state (combat locks, affordability) — default tokens are the
        // legacy grey, digit for digit.
        let (bg, fg) = if slot.enabled {
            (theme.palette.panel_raised.color(), theme.palette.text_primary.color())
        } else {
            (theme.state_colors.slot_disabled_bg.color(), theme.palette.text_dim.color())
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
                    width: Val::Px(s.slot_w),
                    height: Val::Px(s.slot_h),
                    padding: UiRect::all(Val::Px(s.slot_pad)),
                    border: UiRect::all(Val::Px(s.slot_border_w)),
                    border_radius: BorderRadius::all(Val::Px(theme.radius.slot)),
                    ..default()
                },
                BackgroundColor(bg),
                BorderColor::all(theme.palette.panel_border.color()),
            ))
            .with_children(|slot_root| {
                slot_root.spawn((
                    Text::new(format!("[{hotkey}] {}", slot.label)),
                    TextFont { font_size: FontSize::Px(t.font_label), ..default() },
                    TextColor(fg),
                ));
                slot_root.spawn((
                    Text::new(slot.tooltip.clone()),
                    TextFont { font_size: FontSize::Px(t.font_caption), ..default() },
                    TextColor(theme.palette.text_secondary.color()),
                ));
                // WX3: grid badge in the corner (when the card assigns one).
                if let Some(badge) = slot.badge {
                    slot_root.spawn((
                        Text::new(badge.to_string()),
                        TextFont { font_size: FontSize::Px(t.font_badge), ..default() },
                        TextColor(theme.palette.accent.color()),
                        Node {
                            position_type: PositionType::Absolute,
                            top: Val::Px(s.badge_top),
                            right: Val::Px(s.badge_right),
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
            .init_resource::<UiTheme>()
            .init_resource::<UiFont>()
            // SEAM4b: the art-texture seam registers here once (UiPlugin is
            // wired once from lib.rs) — every game gets the resources and
            // rebuild keys without touching its own plugin lists.
            .init_resource::<UiTextureManifest>()
            .init_resource::<UiTextures>()
            .init_resource::<ArtMode>()
            .add_systems(Startup, spawn_command_panel)
            .add_systems(
                Update,
                (
                    command_input,
                    command_clicks,
                    // SEAM3c: record dispatched card actions after the two
                    // dispatchers (same-frame message visibility).
                    record_card_actions.after(command_input).after(command_clicks),
                    refresh_command_panel,
                    refresh_tooltip,
                ),
            );
    }
}

// ---------------------------------------------------------------------------
// UI-DSGN acceptance tests: default theme = legacy colors (zero drift),
// theme swap re-skins with zero code change, skins travel as RON data.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod theme_tests {
    use super::*;
    use crate::testing::test_app;

    fn one_slot(enabled: bool) -> CommandSlotDef {
        CommandSlotDef {
            label: "sell".into(),
            hotkey: Hotkey::Key(KeyCode::KeyQ),
            tooltip: "refund".into(),
            action: CommandAction::Sell,
            enabled,
            badge: Some('Q'),
        }
    }

    fn panel_app(slot: CommandSlotDef) -> App {
        let mut app = test_app();
        let mut cards = CommandCards::default();
        cards.cards.insert(BUILD_CARD.to_string(), CommandCardDef { slots: vec![slot] });
        app.insert_resource(cards)
            .insert_resource(Selection { mode: SelectionMode::Build })
            .init_resource::<ButtonInput<KeyCode>>();
        app.update();
        app.update();
        app
    }

    fn panel_bg(app: &mut App) -> Color {
        let mut q = app.world_mut().query_filtered::<&BackgroundColor, With<CommandPanel>>();
        **q.single(app.world()).unwrap()
    }

    fn slot_bg(app: &mut App) -> Color {
        let mut q = app.world_mut().query_filtered::<&BackgroundColor, With<CommandSlotButton>>();
        **q.single(app.world()).unwrap()
    }

    /// Default theme reproduces the legacy hard-coded panel/slot colors
    /// digit for digit — the token migration is visually a no-op.
    #[test]
    fn default_theme_reproduces_legacy_colors() {
        let mut app = panel_app(one_slot(false));
        assert_eq!(panel_bg(&mut app), Color::srgb(0.06, 0.07, 0.09), "panel bg");
        assert_eq!(slot_bg(&mut app), Color::srgb(0.16, 0.16, 0.18), "disabled slot grey");
        let mut app = panel_app(one_slot(true));
        assert_eq!(slot_bg(&mut app), Color::srgb(0.14, 0.16, 0.2), "enabled slot");
    }

    /// UI-DSGN AC: inserting a different `UiTheme` resource re-skins the
    /// rendered panel on the next update — zero rendering-code change (the
    /// game only touches the resource).
    #[test]
    fn theme_swap_reskins_panel_zero_code() {
        let mut app = panel_app(one_slot(false));
        assert_eq!(panel_bg(&mut app), Color::srgb(0.06, 0.07, 0.09), "legacy skin first");
        // Swap the skin: only a resource insert, revision bumped.
        let mut skin = UiTheme::default();
        skin.revision = 1;
        skin.palette.panel_bg = ThemeColor::rgb(0.5, 0.1, 0.1);
        skin.state_colors.slot_disabled_bg = ThemeColor::rgb(0.4, 0.0, 0.4);
        app.insert_resource(skin);
        app.update();
        assert_eq!(panel_bg(&mut app), Color::srgb(0.5, 0.1, 0.1), "panel re-skinned");
        assert_eq!(slot_bg(&mut app), Color::srgb(0.4, 0.0, 0.4), "slot re-skinned");
        // Same revision again => no rebuild (keyed).
        app.update();
        assert_eq!(panel_bg(&mut app), Color::srgb(0.5, 0.1, 0.1), "stable after swap");
    }

    /// Skins travel as data: a partial RON skin (defaults for the rest)
    /// round-trips losslessly — "换肤 = 改 RON" holds.
    #[test]
    fn theme_ron_round_trip() {
        let ron_src = r#"
            (
                revision: 7,
                palette: (
                    panel_bg: ( r: 0.11, g: 0.08, b: 0.05 ),
                    accent: ( r: 1.0, g: 0.78, b: 0.25 ),
                ),
                typography: ( font_value: 22.0 ),
                radius: ( panel: 6.0 ),
            )
        "#;
        let theme = UiTheme::from_ron(ron_src).expect("partial skin parses");
        assert_eq!(theme.revision, 7);
        assert_eq!(theme.palette.accent, ThemeColor::rgb(1.0, 0.78, 0.25));
        assert_eq!(theme.typography.font_value, 22.0);
        assert_eq!(theme.radius.panel, 6.0);
        // Untouched fields fall back to defaults (serde(default) per group).
        assert_eq!(theme.state_colors, UiStateColors::default());
        let out = ron::to_string(&theme).expect("serialize");
        let back: UiTheme = ron::from_str(&out).expect("round trip");
        assert_eq!(theme, back);
    }
}
