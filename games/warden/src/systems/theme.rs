//! UI skin adapter (WARDEN-SKIN): warden's panels draw from the base
//! `UiTheme` tokens (SEAM2 UI-DSGN) instead of local material constants.
//! The boarded skin is Plan A "Iron & Gold" (铁金条约) — the data lives in
//! `assets/ui/plan-a.ron`, embedded via `include_str!` so headless tests
//! and any working directory parse the exact same bytes the game ships.
//! Design authority: engine/warbase/docs/ui-design-brief.md 拍板记录
//! (2026-10-27) + ui-proposals/plan-A/ screenshots.
//!
//! Legacy note: this module used to define the "晴昼要塞" material palette
//! (白灰石/羊皮纸/要塞金 consts). That palette retired with the Plan-A
//! boarding; every visual constant now resolves through `UiTheme`, so
//! re-skinning warden = replacing one resource, zero render-code change.
//!
//! Token mapping (the adapter contract — where each surface reads from):
//!   bar/plaque base      <- palette.panel_bg
//!   base border          <- palette.panel_border (width spacing.panel_border_w)
//!   top edge hairline    <- palette.accent
//!   inset surfaces (info zone / field report / tooltip / skill chips)
//!                        <- palette.panel_raised + palette.panel_border
//!   enabled slot bg/border <- palette.panel_raised / palette.accent
//!   disabled slot        <- state_colors.slot_disabled_bg + palette.panel_border
//!   body text            <- palette.text_primary (secondary: text_secondary)
//!   gold numbers / gold action bg <- state_colors.gold
//!   hotkey badges / gold headings <- palette.accent
//!   text on gold surfaces <- palette.panel_bg (dark iron on gold)
//!   low-base warning     <- state_colors.health_low
//!   castable skill look  <- state_colors.mana
//!   fusion pick hint     <- state_colors.warning
//!   end-screen scrim     <- state_colors.cooldown
//!
//! What deliberately stays here (structure, NOT skin — WARDEN-SKIN changes
//! style reads only): panel dimensions and layout gaps. Sizes/positions are
//! warden's layout contract; the skin file intentionally does not carry them.

use warbase::ui::UiTheme;

/// The Plan-A skin source (RON, serde-stable). Single source of truth:
/// edit the file, not a code table.
pub const PLAN_A_RON: &str = include_str!("../../assets/ui/plan-a.ron");

/// Parse the boarded skin. `expect` is fine: the bytes are compiled in and
/// pinned by tests/skin.rs (a malformed edit fails CI, not the runtime).
pub fn plan_a() -> UiTheme {
    UiTheme::from_ron(PLAN_A_RON).expect("assets/ui/plan-a.ron must parse (pinned by tests)")
}

// --- Layout constants (structure, 4-multiples; NOT part of the skin) ----------

pub const BAR_HEIGHT: f32 = 128.0; // 底甲板
pub const BTN_W: f32 = 108.0; // 命令卡石槽
pub const BTN_H: f32 = 46.0;
pub const GAP: f32 = 8.0; // 通用间距
pub const CARD_W: f32 = 200.0; // 三选一卡
pub const CARD_H: f32 = 88.0;

// TODO(WARDEN-SKIN): surfaces still hard-coded, tracked for a later pass:
//   (the end-screen banner moved to typography.font_display in SEAM3b)
//   - world-space FX colors (selection ring, fusion pair discs, hover ghost,
//     fx healthbars) — 3D scene effects, not panel skin
//   - tooltip anchor left=240/width=360 and all paddings — layout, not skin
//   - debug overlays (DB3 strip / F1 panel) — debug tooling, not game UI
