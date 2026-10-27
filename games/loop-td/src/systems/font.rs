//! Shared CJK font (SEAM3b): bevy's default font has no CJK glyphs — every
//! Chinese label rendered as tofu boxes (the R4 readability reject from the
//! UI-DSGN review). The fix is Noto Sans SC (SIL OFL 1.1, license travels
//! next to the file) living ONCE at the workspace-root `assets/fonts/` —
//! both games (warden/loop-td) read the same file, deduplicating the
//! 17.7MB copy-per-crate layout. `AssetServer` roots are per-crate, so the
//! loader reads the shared file's BYTES and adds them to `Assets<Font>`
//! directly — working identically windowed and headless, independent of the
//! process CWD.
//!
//! `setup_ui_font` resolves the handle once at Startup:
//! - with an `Assets<Font>` store (windowed AND the LT2-UI headless tests):
//!   read the shared file from disk and add it — even headless runs carry a
//!   real CJK-capable handle (acceptance asserts on it);
//! - bare headless (warbase test apps): fall back to `FontSource::Default`,
//!   never panic — the warden UI6 contract.

use bevy::prelude::*;
use bevy::text::FontSource;

/// File name of the shared font inside the workspace-root `assets/fonts/`.
pub const UI_FONT_FILE: &str = "NotoSansSC-VariableFont_wght.ttf";

/// Absolute path of the shared font file (workspace root, single copy).
pub fn font_file() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../assets/fonts")
        .join(UI_FONT_FILE)
}

/// The resolved UI font every text-bearing panel clones into its `TextFont`s.
#[derive(Resource)]
pub struct UiFont(pub FontSource);

/// Startup: resolve the UI font once; runs before any panel-building system.
pub fn setup_ui_font(
    mut commands: Commands,
    mut fonts: Option<ResMut<Assets<Font>>>,
) {
    // (source for this game's panels, shared handle for the base tooltip
    // seam — MMP tooltip CJK: same Noto face through warbase::ui::UiFont).
    let (source, shared) = if let Some(fonts) = fonts.as_mut() {
        match std::fs::read(font_file()) {
            Ok(bytes) => {
                let handle = fonts.add(Font::from_bytes(bytes));
                (FontSource::Handle(handle.clone()), Some(handle))
            }
            Err(e) => {
                info!("[loop-td] shared ui font unreadable ({e}) — default fallback");
                (FontSource::default(), None)
            }
        }
    } else {
        (FontSource::default(), None)
    };
    commands.insert_resource(UiFont(source));
    commands.insert_resource(warbase::ui::UiFont(shared));
    info!(
        "[loop-td] ui font ready ({})",
        if fonts.is_some() { "shared NotoSansSC" } else { "default fallback" }
    );
}
