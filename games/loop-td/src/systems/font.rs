//! Bundled CJK font (LT2-UI): bevy's default font has no CJK glyphs — every
//! Chinese label rendered as tofu boxes (the R4 readability reject from the
//! UI-DSGN review). The fix is the same asset warden ships (UI6): Noto Sans
//! SC (SIL OFL 1.1, license travels next to the file), duplicated into this
//! crate's assets because `engine/` is frozen this increment (a shared
//! asset root would be the engine-side hoist, recorded for the C-channel).
//!
//! `setup_ui_font` resolves the handle once at Startup:
//! - windowed app: `AssetServer::load` of the canonical asset path;
//! - headless WITH an `Assets<Font>` store (the LT2-UI tests): read the
//!   pinned file from disk and add it, so even headless runs carry a real
//!   CJK-capable handle (acceptance asserts on it);
//! - bare headless (warbase test apps): fall back to `FontSource::Default`,
//!   never panic — the warden UI6 contract.

use bevy::prelude::*;
use bevy::text::FontSource;

/// Asset path of the bundled font, relative to the `assets/` root.
pub const UI_FONT_PATH: &str = "fonts/NotoSansSC-VariableFont_wght.ttf";

/// Absolute path of the bundled font file (test/headless loading).
pub fn font_file() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets").join(UI_FONT_PATH)
}

/// The resolved UI font every text-bearing panel clones into its `TextFont`s.
#[derive(Resource)]
pub struct UiFont(pub FontSource);

/// Startup: resolve the UI font once; runs before any panel-building system.
pub fn setup_ui_font(
    mut commands: Commands,
    server: Option<Res<AssetServer>>,
    mut fonts: Option<ResMut<Assets<Font>>>,
) {
    let source = if let Some(server) = &server {
        FontSource::Handle(server.load(UI_FONT_PATH))
    } else if let Some(fonts) = fonts.as_mut() {
        match std::fs::read(font_file()) {
            Ok(bytes) => FontSource::Handle(fonts.add(Font::from_bytes(bytes))),
            Err(e) => {
                info!("[loop-td] ui font file unreadable ({e}) — default fallback");
                FontSource::default()
            }
        }
    } else {
        FontSource::default()
    };
    commands.insert_resource(UiFont(source));
    info!(
        "[loop-td] ui font ready ({})",
        if server.is_some() || fonts.is_some() { "bundled NotoSansSC" } else { "default fallback" }
    );
}
