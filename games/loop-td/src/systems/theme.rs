//! UI skin source (LT2-UI): loop-td's boarded design system is Plan A
//! "Iron & Gold" (铁金条约) — the data lives in `assets/ui/plan-a.ron`,
//! embedded via `include_str!` so headless tests and any working directory
//! parse the exact same bytes the game ships (the warden WARDEN-SKIN
//! pattern, mirrored). Design authority: engine/warbase/docs/ui-design-brief.md
//! 拍板记录 (2026-10-27) + ui-proposals/plan-A/ screenshots.
//!
//! The game inserts this as the `UiTheme` resource at plugin build; every
//! panel (the warbase command card included) reads tokens from it, so
//! re-skinning loop-td = replacing one resource / one RON file, zero
//! rendering-code change.

use warbase::ui::UiTheme;

/// The Plan-A skin source (RON, serde-stable). Single source of truth:
/// edit the file, not a code table.
pub const PLAN_A_RON: &str = include_str!("../../assets/ui/plan-a.ron");

/// Parse the boarded skin. `expect` is fine: the bytes are compiled in and
/// pinned by tests/lt2_ui.rs (a malformed edit fails CI, not the runtime).
pub fn plan_a() -> UiTheme {
    UiTheme::from_ron(PLAN_A_RON).expect("assets/ui/plan-a.ron must parse (pinned by tests)")
}
