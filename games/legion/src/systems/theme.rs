//! UI skin source (LG3): legion's boarded design system is Plan A "Iron &
//! Gold" (铁金条约) — copied verbatim from loop-td (`assets/ui/plan-a.ron`,
//! the boarded 拍板 skin), embedded via `include_str!` so headless tests and
//! any working directory parse the exact same bytes the game ships.
//! Design authority: engine/warbase/docs/ui-design-brief.md 拍板记录
//! (2026-10-27) + ui-proposals/plan-A/ screenshots.
//!
//! The game inserts this as the `UiTheme` resource at plugin build; every
//! panel (these six + the warbase command surfaces) reads tokens from it, so
//! re-skinning legion = replacing one resource / one RON file, zero
//! rendering-code change (the LG3 acceptance asserts exactly that).

use warbase::ui::UiTheme;

/// The Plan-A skin source (RON, serde-stable). Single source of truth:
/// edit the file, not a code table.
pub const PLAN_A_RON: &str = include_str!("../../assets/ui/plan-a.ron");

/// Parse the boarded skin. `expect` is fine: the bytes are compiled in and
/// pinned by tests/ui_panels.rs (a malformed edit fails CI, not the runtime).
pub fn plan_a() -> UiTheme {
    UiTheme::from_ron(PLAN_A_RON).expect("assets/ui/plan-a.ron must parse (pinned by tests)")
}
