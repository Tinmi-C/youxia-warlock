//! UITEX (round-4 plan §Phase 1) acceptance: the SEAM4b art-texture seam
//! end to end. Greybox is the default and stays component-identical (zero
//! drift); art mode adds nine-sliced `ImageNode` surfaces driven by the
//! `UiTextures` companion resource; every keyed consumer folds the texture
//! revision into its cache key; the `--art` loader degrades to warns +
//! greybox on every miss and never panics.
//!
//! Two compositions run here: the BASE test app (warbase `UiPlugin` — the
//! engine command-panel consumption points) and the real loop-td game
//! composition (`GamePlugin` carries the Plan-A theme + `PanelsPlugin` —
//! the `panel_node`/`rebuild`/equip+command slot faces). Fixtures are two
//! procedurally generated nine-grid PNGs under `tests/fixtures/` — the
//! headless discipline: nothing outside the `--art`/evidence paths reads
//! an asset file, and nothing lands in the game's `assets/`.
//!
//! Offscreen evidence (greybox vs textured) is the `#[ignore]`d GPU test
//! at the bottom:
//!   cargo test -p loop-td --test uitex_domain evidence -- --ignored --nocapture

use bevy::prelude::*;
use bevy::sprite::{BorderRect, SliceScaleMode, TextureSlicer};
use loop_td::plugins::game::GamePlugin;
use loop_td::systems::panels::{PanelKind, PanelRoot};
use warbase::prelude::*;
use warbase::testing::test_app;
use warbase::ui::{
    load_ui_textures, ArtMode, CommandAction, CommandCardDef, CommandCards, CommandSlotDef,
    Hotkey, UiTextureManifest, UiTextures, UiTheme, BUILD_CARD, UI_TEX_PANEL, UI_TEX_SLOT,
};

// ---------------------------------------------------------------------------
// Fixture: procedural nine-grid PNGs (tests/fixtures/ only — never assets/)
// ---------------------------------------------------------------------------

fn fixture_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

/// Deterministic nine-grid RGBA buffer: a `ring`-px gold frame, a one-pixel
/// darker inner line, and a diagonally striped iron center (the stripes
/// make stretch/slice behavior visible in the evidence shots).
fn nine_grid_rgba(size: u32, ring: u32) -> Vec<u8> {
    let gold = [214u8, 176, 92];
    let gold_dim = [140, 112, 58];
    let iron_a = [26, 23, 19];
    let iron_b = [38, 34, 28];
    let mut px = vec![0u8; (size * size * 4) as usize];
    for y in 0..size {
        for x in 0..size {
            let i = ((y * size + x) * 4) as usize;
            let on_ring =
                x < ring || y < ring || x >= size - ring || y >= size - ring;
            let on_inner = !on_ring
                && (x < ring + 1 || y < ring + 1 || x >= size - ring - 1 || y >= size - ring - 1);
            let [r, g, b] = if on_ring {
                gold
            } else if on_inner {
                gold_dim
            } else if (x + y) % 8 < 4 {
                iron_a
            } else {
                iron_b
            };
            px[i] = r;
            px[i + 1] = g;
            px[i + 2] = b;
            px[i + 3] = 255;
        }
    }
    px
}

/// (Re)write the two fixtures + their manifest. Byte-deterministic, so the
/// committed files never churn.
fn write_fixtures() {
    let dir = fixture_dir();
    std::fs::create_dir_all(&dir).unwrap();
    // Panel: 48x48, 2px ring — plan-a's panel_border_w token. The domain
    // tests slice the SAME file at other insets (the slicer config comes
    // from the theme token, not the painting).
    png::write_rgba_png(
        &dir.join("uitex-panel.png"),
        48,
        48,
        &nine_grid_rgba(48, 2),
    );
    // Slot: 28x28, 1px ring — plan-a's slot_border_w token.
    png::write_rgba_png(
        &dir.join("uitex-slot.png"),
        28,
        28,
        &nine_grid_rgba(28, 1),
    );
    std::fs::write(
        dir.join("uitextures.ron"),
        "(textures: {\n    \"panel\": \"uitex-panel.png\",\n    \"slot\": \"uitex-slot.png\",\n})\n",
    )
    .unwrap();
}

/// Minimal deterministic PNG encoder (RGBA8, filter 0, stored deflate) —
/// zero dependencies; test-fixture only.
mod png {
    fn crc32(data: &[u8]) -> u32 {
        let mut table = [0u32; 256];
        for (n, slot) in table.iter_mut().enumerate() {
            let mut c = n as u32;
            for _ in 0..8 {
                c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
            }
            *slot = c;
        }
        let mut c = 0xFFFF_FFFFu32;
        for &b in data {
            c = table[((c ^ b as u32) & 0xFF) as usize] ^ (c >> 8);
        }
        c ^ 0xFFFF_FFFF
    }

    fn adler32(data: &[u8]) -> u32 {
        let (mut a, mut b) = (1u32, 0u32);
        for chunk in data.chunks(5552) {
            for &byte in chunk {
                a += byte as u32;
                b += a;
            }
            a %= 65521;
            b %= 65521;
        }
        (b << 16) | a
    }

    /// zlib stream of stored (uncompressed) deflate blocks.
    fn zlib_stored(data: &[u8]) -> Vec<u8> {
        let mut out = vec![0x78, 0x01];
        if data.is_empty() {
            out.extend_from_slice(&[0x01, 0x00, 0x00, 0xFF, 0xFF]);
        } else {
            for (i, chunk) in data.chunks(65_535).enumerate() {
                let last = i == (data.len().div_ceil(65_535) - 1);
                out.push(if last { 0x01 } else { 0x00 });
                out.extend_from_slice(&(chunk.len() as u16).to_le_bytes());
                out.extend_from_slice(&(!(chunk.len() as u16)).to_le_bytes());
                out.extend_from_slice(chunk);
            }
        }
        out.extend_from_slice(&adler32(data).to_be_bytes());
        out
    }

    fn chunk(tag: &[u8; 4], data: &[u8]) -> Vec<u8> {
        let mut out = (data.len() as u32).to_be_bytes().to_vec();
        out.extend_from_slice(tag);
        out.extend_from_slice(data);
        let mut crc_input = tag.to_vec();
        crc_input.extend_from_slice(data);
        out.extend_from_slice(&crc32(&crc_input).to_be_bytes());
        out
    }

    pub fn write_rgba_png(path: &std::path::Path, w: u32, h: u32, rgba: &[u8]) {
        assert_eq!(rgba.len(), (w * h * 4) as usize, "pixel buffer must match w*h*4");
        let mut ihdr = Vec::with_capacity(13);
        ihdr.extend_from_slice(&w.to_be_bytes());
        ihdr.extend_from_slice(&h.to_be_bytes());
        ihdr.extend_from_slice(&[8, 6, 0, 0, 0]); // depth 8, RGBA, deflate, no filter, no interlace
        // Raw scanlines: filter byte 0 per row.
        let stride = (w * 4) as usize;
        let mut raw = Vec::with_capacity((stride + 1) * h as usize);
        for y in 0..h as usize {
            raw.push(0u8);
            raw.extend_from_slice(&rgba[y * stride..(y + 1) * stride]);
        }
        let mut png = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
        png.extend_from_slice(&chunk(b"IHDR", &ihdr));
        png.extend_from_slice(&chunk(b"IDAT", &zlib_stored(&raw)));
        png.extend_from_slice(&chunk(b"IEND", &[]));
        std::fs::write(path, png).unwrap();
    }
}

// ---------------------------------------------------------------------------
// Harnesses
// ---------------------------------------------------------------------------

/// The BASE panel app: warbase test app + a one-slot build card (mirrors
/// ui/mod.rs's own theme_tests composition — the engine-side consumption
/// points without any game).
fn engine_panel_app(theme: UiTheme) -> App {
    let mut app = test_app();
    let mut cards = CommandCards::default();
    cards.cards.insert(
        BUILD_CARD.to_string(),
        CommandCardDef {
            slots: vec![
                CommandSlotDef {
                    label: "sell".into(),
                    hotkey: Hotkey::Key(KeyCode::KeyQ),
                    tooltip: "refund".into(),
                    action: CommandAction::Sell,
                    enabled: true,
                    badge: Some('Q'),
                },
                CommandSlotDef {
                    label: "lock".into(),
                    hotkey: Hotkey::None,
                    tooltip: "locked".into(),
                    action: CommandAction::Sell,
                    enabled: false,
                    badge: None,
                },
            ],
        },
    );
    app.insert_resource(cards)
        .insert_resource(theme)
        .insert_resource(warbase::ui::Selection {
            mode: warbase::ui::SelectionMode::Build,
        })
        .init_resource::<ButtonInput<KeyCode>>();
    app
}

/// The real loop-td headless game (lt2_ui's recipe): GamePlugin loads the
/// shipped map + Plan-A skin and drives the Plan-A panels.
fn game_app() -> App {
    let mut app = test_app();
    app.add_plugins(GamePlugin);
    app.insert_resource(MatchRng::seeded(42));
    app.update();
    app.update();
    app
}

/// Add two images to an app's asset store; returns (panel, slot) handles.
fn inject_test_images(app: &mut App) -> (Handle<Image>, Handle<Image>) {
    app.init_resource::<Assets<Image>>();
    let world = app.world_mut();
    let panel = world
        .resource_mut::<Assets<Image>>()
        .add(Image::default());
    let slot = world
        .resource_mut::<Assets<Image>>()
        .add(Image::default());
    (panel, slot)
}

fn insert_art(app: &mut App, panel: Option<Handle<Image>>, slot: Option<Handle<Image>>, revision: u64) {
    let mut handles = std::collections::HashMap::new();
    if let Some(h) = panel {
        handles.insert(UI_TEX_PANEL.to_string(), h);
    }
    if let Some(h) = slot {
        handles.insert(UI_TEX_SLOT.to_string(), h);
    }
    app.insert_resource(UiTextures { handles, revision });
    app.insert_resource(ArtMode(true));
}

fn engine_panel_root(app: &mut App) -> Entity {
    let mut q = app
        .world_mut()
        .query_filtered::<Entity, With<warbase::ui::CommandPanel>>();
    q.single(app.world()).unwrap()
}

fn panel_root_of(app: &mut App, kind: PanelKind) -> Entity {
    let world = app.world_mut();
    let mut q = world.query_filtered::<Entity, With<PanelRoot>>();
    q.iter(world)
        .find(|&e| world.get::<PanelRoot>(e).map(|r| r.0 == kind).unwrap_or(false))
        .expect("panel root exists")
}

fn sliced(border_w: f32) -> NodeImageMode {
    NodeImageMode::Sliced(TextureSlicer {
        border: BorderRect::all(border_w),
        center_scale_mode: SliceScaleMode::Stretch,
        sides_scale_mode: SliceScaleMode::Stretch,
        max_corner_scale: 1.0,
    })
}

// ---------------------------------------------------------------------------
// Assertion ①: default = greybox zero drift (no ImageNode anywhere)
// ---------------------------------------------------------------------------

/// Without art resources the panel surfaces carry NO ImageNode at all —
/// component-for-component the legacy rendering (BackgroundColor +
/// BorderColor alone; an always-on solid ImageNode would paint over the
/// theme's border ring — bevy_ui stacks IMAGE above BORDER).
#[test]
fn greybox_default_carries_no_image_nodes() {
    let mut app = engine_panel_app(UiTheme::default());
    app.update();
    app.update();
    let world = app.world_mut();
    let mut imgs = world.query_filtered::<(), With<ImageNode>>();
    assert_eq!(
        imgs.iter(world).count(),
        0,
        "engine panel app: greybox must not spawn ImageNodes"
    );

    let mut app = game_app();
    app.update();
    let world = app.world_mut();
    let mut imgs = world.query_filtered::<(), With<ImageNode>>();
    assert_eq!(
        imgs.iter(world).count(),
        0,
        "loop-td game app: greybox must not spawn ImageNodes"
    );
}

// ---------------------------------------------------------------------------
// Assertion ② + ⑤: art mode = nine-sliced ImageNode, insets from tokens
// ---------------------------------------------------------------------------

/// Injected textures + ArtMode(true) turn the engine panel root and slot
/// faces into `ImageNode`s with `NodeImageMode::Sliced`, slicer insets from
/// the theme's border tokens (corners unscaled by TextureSlicer semantics
/// — `max_corner_scale: 1.0` asserted literally).
#[test]
fn art_mode_slices_engine_panel_root_and_slots() {
    let mut theme = UiTheme::default();
    theme.revision = 1;
    theme.spacing.panel_border_w = 12.0;
    theme.spacing.slot_border_w = 4.0;
    let mut app = engine_panel_app(theme);
    let (panel_h, slot_h) = inject_test_images(&mut app);
    insert_art(&mut app, Some(panel_h.clone()), Some(slot_h.clone()), 1);
    app.update();
    app.update();

    // Root: Sliced, insets = panel_border_w, the injected handle.
    let root = engine_panel_root(&mut app);
    let img = app.world().get::<ImageNode>(root).expect("root ImageNode in art mode");
    assert_eq!(img.image_mode, sliced(12.0), "root slicer insets = panel_border_w token");
    assert_eq!(img.image, panel_h, "root renders the injected panel texture");

    // Slot faces: Sliced, insets = slot_border_w, the slot handle.
    let world = app.world_mut();
    let mut slots = world.query_filtered::<&ImageNode, With<warbase::ui::CommandSlotButton>>();
    let count = slots.iter(world).count();
    assert_eq!(count, 2, "both card slots render");
    let imgs: Vec<&ImageNode> = slots.iter(world).collect();
    assert_eq!(imgs.len(), 2, "both card slots render");
    for img in imgs {
        assert_eq!(img.image_mode, sliced(4.0), "slot slicer insets = slot_border_w token");
        assert_eq!(img.image, slot_h, "slot renders the injected slot texture");
    }
}

/// The same seam through the real loop-td panels: every Plan-A panel root
/// slices at plan-a's `panel_border_w` (2.0), the equip/command slot faces
/// at `slot_border_w.max(1.0)` (the width those faces draw borders at).
#[test]
fn art_mode_slices_loop_td_panels_and_slot_faces() {
    let plan_a = loop_td::systems::theme::plan_a();
    let mut app = game_app();
    let (panel_h, slot_h) = inject_test_images(&mut app);
    insert_art(&mut app, Some(panel_h.clone()), Some(slot_h.clone()), 1);

    // Build mode first -> the BC command console faces the build card.
    app.world_mut().insert_resource(warbase::ui::Selection {
        mode: warbase::ui::SelectionMode::Build,
    });
    app.update();
    app.update();

    // Root faces: every panel root sliced at the plan-a panel token.
    let border_w = plan_a.spacing.panel_border_w;
    let world = app.world_mut();
    let mut roots = world.query_filtered::<&ImageNode, With<PanelRoot>>();
    let imgs: Vec<&ImageNode> = roots.iter(world).collect();
    // Eight now: the Hero/Equip refreshes early-return without a selected
    // hero (their roots re-skin on selection — asserted at the bottom).
    assert_eq!(imgs.len(), 8, "the eight always-driven Plan-A roots textured");
    for img in imgs {
        assert_eq!(img.image_mode, sliced(border_w), "plan-a panel_border_w insets");
        assert_eq!(img.image, panel_h, "plan-a root uses the panel texture");
    }

    // Command console slots (build card face) at the slot token.
    let slot_border = plan_a.spacing.slot_border_w.max(1.0);
    let world = app.world_mut();
    let mut faces = world.query_filtered::<&ImageNode, With<warbase::ui::CommandSlotButton>>();
    let imgs: Vec<&ImageNode> = faces.iter(world).collect();
    assert!(!imgs.is_empty(), "command console slots textured");
    for img in imgs {
        assert_eq!(img.image_mode, sliced(slot_border), "command slot insets = slot token");
        assert_eq!(img.image, slot_h, "command slot uses the slot texture");
    }

    // Now a selected hero -> the BR equip rack shows its (empty) slot rows.
    app.world_mut().resource_mut::<Resources>().gold += 200;
    let cell = {
        let grid = app.world().resource::<BuildGrid>().clone();
        (0..grid.cols)
            .flat_map(|x| (0..grid.rows).map(move |y| Cell(x, y)))
            .find(|c| grid.check(*c).is_ok())
            .expect("a free cell")
    };
    let hero =
        loop_td::systems::gameplay::try_place(&mut app.world_mut(), "hunter", cell).expect("place hunter");
    app.world_mut().insert_resource(warbase::ui::Selection {
        mode: warbase::ui::SelectionMode::Unit(hero),
    });
    app.update();
    app.update();

    let rack_root = panel_root_of(&mut app, PanelKind::Equip);
    // With a hero live, the last two roots (Hero + Equip) re-skinned too.
    {
        let world = app.world_mut();
        let mut roots = world.query_filtered::<&ImageNode, With<PanelRoot>>();
        assert_eq!(
            roots.iter(world).count(),
            10,
            "all ten Plan-A roots textured once a hero is selected"
        );
    }
    let world = app.world_mut();
    let mut rack_faces: Vec<&ImageNode> = Vec::new();
    fn walk<'a>(world: &'a World, e: Entity, out: &mut Vec<&'a ImageNode>) {
        if let Some(img) = world.get::<ImageNode>(e) {
            out.push(img);
        }
        if let Some(children) = world.get::<Children>(e) {
            for c in children.iter() {
                walk(world, c, out);
            }
        }
    }
    // (children only — the root's own panel face is excluded)
    if let Some(children) = world.get::<Children>(rack_root) {
        for c in children.iter() {
            walk(world, c, &mut rack_faces);
        }
    }
    assert_eq!(rack_faces.len(), 3, "three equip slot rows (EQUIP_SLOTS)");
    for img in &rack_faces {
        assert_eq!(img.image_mode, sliced(slot_border), "equip row insets = slot token");
        assert_eq!(img.image, slot_h, "equip row uses the slot texture");
    }
}

/// Art mode with a MISSING slot texture degrades per-surface to the token
/// color as a stretched solid (`ImageNode::solid_color`) — never a panic,
/// never an invisible surface.
#[test]
fn art_mode_missing_slot_falls_back_to_solid_token_color() {
    let theme = UiTheme::default();
    let raised = theme.palette.panel_raised.color();
    let disabled = theme.state_colors.slot_disabled_bg.color();
    let mut app = engine_panel_app(theme);
    let (panel_h, _slot_h) = inject_test_images(&mut app);
    // Only the panel slot is loaded; UI_TEX_SLOT is absent.
    insert_art(&mut app, Some(panel_h), None, 1);
    app.update();
    app.update();

    let world = app.world_mut();
    let mut slots = world.query::<(&ImageNode, &warbase::ui::CommandSlotButton)>();
    let imgs: Vec<(&ImageNode, &warbase::ui::CommandSlotButton)> =
        slots.iter(world).collect();
    assert_eq!(imgs.len(), 2, "slot faces exist in art mode");
    for (img, button) in imgs {
        assert_eq!(img.image_mode, NodeImageMode::Stretch, "solid fallback stretches");
        let want = if button.enabled { raised } else { disabled };
        assert_eq!(img.color, want, "fallback keeps the slot's token color");
    }
}

// ---------------------------------------------------------------------------
// Assertion ③: texture revision change rebuilds (cache key)
// ---------------------------------------------------------------------------

/// Bumping `UiTextures.revision` with a new handle re-skins the engine
/// panel root on the next update — with the SAME card, version and theme
/// revision (only the art seam moved; the rebuild is keyed on it).
#[test]
fn texture_revision_change_rebuilds_engine_panel() {
    let mut app = engine_panel_app(UiTheme::default());
    let (panel_a, _s1) = inject_test_images(&mut app);
    let panel_b = app
        .world_mut()
        .resource_mut::<Assets<Image>>()
        .add(Image::default());
    insert_art(&mut app, Some(panel_a.clone()), None, 1);
    app.update();
    app.update();
    let root = engine_panel_root(&mut app);
    assert_eq!(
        app.world().get::<ImageNode>(root).unwrap().image,
        panel_a,
        "first texture in place"
    );

    // Swap texture, bump revision only.
    let world = app.world_mut();
    let mut handles = std::collections::HashMap::new();
    handles.insert(UI_TEX_PANEL.to_string(), panel_b.clone());
    world.insert_resource(UiTextures { handles, revision: 2 });
    app.update();
    let root = engine_panel_root(&mut app);
    assert_eq!(
        app.world().get::<ImageNode>(root).unwrap().image,
        panel_b,
        "revision bump re-skinned the panel (keyed rebuild)"
    );

    // Same revision again => keyed, no churn observable as stability.
    app.update();
    let root = engine_panel_root(&mut app);
    assert_eq!(app.world().get::<ImageNode>(root).unwrap().image, panel_b);
}

/// The loop-td side folds the same revision into every Plan-A panel key
/// (rebuild's single chokepoint).
#[test]
fn texture_revision_change_rebuilds_loop_td_panels() {
    let mut app = game_app();
    let (panel_a, _slot_a) = inject_test_images(&mut app);
    let panel_b = app
        .world_mut()
        .resource_mut::<Assets<Image>>()
        .add(Image::default());
    insert_art(&mut app, Some(panel_a.clone()), None, 1);
    app.update();
    let root = panel_root_of(&mut app, PanelKind::Resource);
    assert_eq!(app.world().get::<ImageNode>(root).unwrap().image, panel_a);

    let world = app.world_mut();
    let mut handles = std::collections::HashMap::new();
    handles.insert(UI_TEX_PANEL.to_string(), panel_b.clone());
    world.insert_resource(UiTextures { handles, revision: 2 });
    app.update();
    let root = panel_root_of(&mut app, PanelKind::Resource);
    assert_eq!(
        app.world().get::<ImageNode>(root).unwrap().image,
        panel_b,
        "plan-a roots re-skin on the texture revision alone"
    );
}

// ---------------------------------------------------------------------------
// Assertion ④: --art loader — warns, falls back, never panics
// ---------------------------------------------------------------------------

/// Missing manifest / missing file: the loader reports facts (the data
/// behind the warn logs), injects only what loaded, and never panics —
/// the game stays greybox because `--art` wiring refuses ArtMode(true)
/// unless the report is clean.
#[test]
fn loader_misses_warn_and_fall_back_without_panic() {
    // Missing manifest file.
    let mut app = test_app();
    let report =
        load_ui_textures(app.world_mut(), &fixture_dir().join("no-such-manifest.ron"));
    assert!(!report.manifest_ok, "missing manifest reported");
    assert!(report.loaded.is_empty() && report.failed.is_empty());
    assert_eq!(
        app.world().resource::<UiTextures>().handles.len(),
        0,
        "no textures injected"
    );

    // Manifest present, one declared file missing on disk (a name that
    // never exists — no fixture renaming: tests run in parallel).
    let dir = fixture_dir();
    std::fs::create_dir_all(&dir).unwrap();
    write_fixtures(); // the panel fixture exists, the slot target does not
    std::fs::write(
        dir.join("uitextures-missing.ron"),
        "(textures: {\n    \"panel\": \"uitex-panel.png\",\n    \"slot\": \"never-exists-slot.png\",\n})\n",
    )
    .unwrap();
    let mut app = test_app();
    let report = load_ui_textures(app.world_mut(), &dir.join("uitextures-missing.ron"));
    assert!(report.manifest_ok, "the manifest itself parsed");
    assert_eq!(report.loaded, vec!["panel".to_string()], "good file loaded");
    assert_eq!(report.failed, vec!["slot".to_string()], "missing file reported (the warn)");
    let textures = app.world().resource::<UiTextures>();
    assert_eq!(textures.handles.len(), 1, "only the good handle injected");
    assert_eq!(textures.revision, 1, "revision bumped once");

    // The --art gate in main.rs: dirty report => no ArtMode(true) => the
    // app renders greybox (assertion: no ImageNodes appear even with
    // textures partially loaded).
    app.update();
    let world = app.world_mut();
    let mut imgs = world.query_filtered::<(), With<ImageNode>>();
    assert_eq!(imgs.iter(world).count(), 0, "greybox fallback, no ImageNodes");
}

/// Good fixtures round-trip through the real decode path: PNG bytes ->
/// `Image` (48x48) -> handle -> revision bumps per load.
#[test]
fn fixtures_round_trip_through_png_decode() {
    write_fixtures();
    let mut app = test_app();
    let report = load_ui_textures(app.world_mut(), &fixture_dir().join("uitextures.ron"));
    assert!(report.manifest_ok, "{report:?}");
    assert!(report.failed.is_empty(), "{report:?}");
    let mut loaded = report.loaded.clone();
    loaded.sort();
    assert_eq!(loaded, vec!["panel".to_string(), "slot".to_string()]);

    let world = app.world_mut();
    let textures = world.resource::<UiTextures>().clone();
    assert_eq!(textures.revision, 1);
    let handle = textures.handles[UI_TEX_PANEL].clone();
    let size = world.resource::<Assets<Image>>().get(&handle).expect("decoded").size();
    assert_eq!(size, bevy::math::UVec2::new(48, 48), "decoded dimensions");

    // Reload bumps the revision (hot-swap key).
    let report = load_ui_textures(app.world_mut(), &fixture_dir().join("uitextures.ron"));
    assert!(report.failed.is_empty());
    assert_eq!(app.world().resource::<UiTextures>().revision, 2);
}

/// The manifest itself is serde data: partial RON parses, round-trips.
#[test]
fn manifest_is_serde_data() {
    let m = UiTextureManifest::from_ron(r#"(textures: { "panel": "p.png" })"#).unwrap();
    assert_eq!(m.textures["panel"], "p.png");
    let out = ron::to_string(&m).unwrap();
    let back: UiTextureManifest = ron::from_str(&out).unwrap();
    assert_eq!(m, back);
}

// ---------------------------------------------------------------------------
// Offscreen evidence: greybox vs textured (GPU, #[ignore] — mmp pattern)
// ---------------------------------------------------------------------------

/// Round-4 UITEX evidence: two PNGs into `games/loop-td/docs/img/art-mode/`
/// — the same live HUD rendered OFFSCREEN (DefaultPlugins with
/// `primary_window: None` + WinitPlugin disabled + a Camera2d targeting a
/// GPU texture + `Screenshot::image`): first greybox (the default), then
/// with the fixtures loaded and ArtMode on. No window, no display server —
/// immune to the locked-session black-shot pitfall.
///   cargo test -p loop-td --test uitex_domain evidence -- --ignored --nocapture
#[test]
#[ignore = "GPU offscreen render (evidence capture only) — run with --ignored"]
fn evidence_offscreen_greybox_vs_art() {
    use bevy::window::{ExitCondition, WindowPlugin};
    use bevy::winit::WinitPlugin;

    write_fixtures(); // self-contained run
    let out_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("docs/img/art-mode");
    std::fs::create_dir_all(&out_dir).unwrap();

    let mut app = App::new();
    app.insert_resource(ClearColor(Color::srgb(0.08, 0.09, 0.12)))
        .add_plugins(
            DefaultPlugins
                .set(WindowPlugin {
                    primary_window: None,
                    exit_condition: ExitCondition::DontExit,
                    ..default()
                })
                .set(bevy::render::RenderPlugin {
                    synchronous_pipeline_compilation: true,
                    ..default()
                })
                .disable::<WinitPlugin>(),
        )
        .init_state::<loop_td::states::GameState>()
        // WarbasePlugin + the headless-safe game core (map, cards, Plan-A
        // panels) — no VisualPlugin: the HUD is the subject.
        .add_plugins((warbase::WarbasePlugin, GamePlugin))
        .add_systems(Startup, spawn_offscreen_ui_camera);
    app.finish();
    app.cleanup();

    // Startup settles: map + build card + font + panel layout + pipelines.
    for _ in 0..60 {
        app.update();
    }
    // Build mode: the BC command console faces the build card, every panel
    // carries live state.
    app.world_mut().insert_resource(warbase::ui::Selection {
        mode: warbase::ui::SelectionMode::Build,
    });
    for _ in 0..10 {
        app.update();
    }

    let target = app.world().resource::<OffscreenTarget>().0.clone();
    capture(&mut app, &target, &out_dir.join("greybox.png"));

    // Flip the seam: load the fixtures, art mode on. The revision bump
    // re-skins every panel on the next refresh pass.
    let report = load_ui_textures(app.world_mut(), &fixture_dir().join("uitextures.ron"));
    assert!(report.manifest_ok && report.failed.is_empty(), "{report:?}");
    app.insert_resource(ArtMode(true));
    for _ in 0..10 {
        app.update();
    }
    capture(&mut app, &target, &out_dir.join("art-textured.png"));

    // Real evidence only: PNG signature, non-trivial payload, and the two
    // forms must differ (textures visibly re-skinned the HUD).
    let grey = std::fs::read(out_dir.join("greybox.png")).expect("greybox shot");
    let art = std::fs::read(out_dir.join("art-textured.png")).expect("art shot");
    for (name, bytes) in [("greybox.png", &grey), ("art-textured.png", &art)] {
        assert!(bytes.starts_with(&[0x89, b'P', b'N', b'G']), "{name} PNG signature");
        assert!(bytes.len() > 4096, "{name} suspiciously small ({} B)", bytes.len());
    }
    assert_ne!(grey, art, "the two art forms must render differently");
    println!(
        "evidence: {} greybox.png ({} B), art-textured.png ({} B)",
        out_dir.display(),
        grey.len(),
        art.len()
    );
}

/// Offscreen render target (1280x720 — the design canvas the Plan-A
/// absolute-px rects lay out on), held for the later `Screenshot::image`.
#[derive(Resource)]
struct OffscreenTarget(Handle<Image>);

fn spawn_offscreen_ui_camera(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
    use bevy::camera::RenderTarget;
    use bevy::render::render_resource::{TextureFormat, TextureUsages};
    let mut image = Image::new_target_texture(1280, 720, TextureFormat::Rgba8UnormSrgb, None);
    image.texture_descriptor.usage |= TextureUsages::COPY_SRC;
    let handle = images.add(image);
    commands.spawn((
        Camera2d,
        RenderTarget::Image(handle.clone().into()),
        bevy::ui::IsDefaultUiCamera,
    ));
    commands.insert_resource(OffscreenTarget(handle));
}

/// Queue one `Screenshot::image` and tick until the PNG is on disk.
fn capture(app: &mut App, target: &Handle<Image>, path: &std::path::Path) {
    use bevy::render::view::screenshot::{save_to_disk, Screenshot};
    use std::time::{Duration, Instant};
    app.world_mut()
        .spawn(Screenshot::image(target.clone()))
        .observe(save_to_disk(path.to_path_buf()));
    let t0 = Instant::now();
    while t0.elapsed() < Duration::from_secs(10) {
        app.update();
        if let Ok(meta) = std::fs::metadata(path) {
            if meta.len() > 0 {
                return;
            }
        }
    }
    panic!("screenshot never landed: {}", path.display());
}
