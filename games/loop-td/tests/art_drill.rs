//! DRILL (round-4 plan §Phase 2) — the first-art-day rehearsal's executable
//! evidence. Everything here walks the REAL chain with REAL files under
//! `assets-art-drill/` (the drill's mock-vault store + the art-synced
//! pseudo-game root), so every test reads asset files and is therefore
//! `#[ignore]`d — the headless discipline (default `cargo test` stays
//! asset-free) gets a double gate: file-gated contract/rebuild tests plus a
//! GPU-gated offscreen evidence capture, mirroring `uitex_domain.rs`.
//!   cargo test -p loop-td --test art_drill -- --ignored --nocapture
//!
//! Rehearsal story (full log in docs/art-first-day-runbook.md):
//!   fulfilled -> art-mapping.json -> art-assets.json (demand back-link)
//!   -> art-sync (byte-verified pull) -> uitextures.ron -> --art
//!   -> upgrade v2 (same byte size: the art-sync size-only pitfall) and
//!   v3 (size change: auto re-pull) -> revision bump re-skins every panel.

use bevy::prelude::*;
use loop_td::plugins::game::GamePlugin;
use loop_td::systems::panels::{PanelKind, PanelRoot};
use warbase::prelude::*;
use warbase::testing::test_app;
use warbase::ui::{load_ui_textures, ArtMode, UiTextures, UI_TEX_PANEL};

fn drill_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets-art-drill")
}

fn synced_manifest() -> std::path::PathBuf {
    drill_dir().join("first-day/assets/ui/uitextures.ron")
}

fn vault_manifest_v1() -> std::path::PathBuf {
    drill_dir().join("vault/uitextures-v1.ron")
}

fn panel_root_of(app: &mut App, kind: PanelKind) -> Entity {
    let world = app.world_mut();
    let mut q = world.query_filtered::<Entity, With<PanelRoot>>();
    q.iter(world)
        .find(|&e| world.get::<PanelRoot>(e).map(|r| r.0 == kind).unwrap_or(false))
        .expect("panel root exists")
}

// ---------------------------------------------------------------------------
// Contract assertions: mapping / lockfile / synced bytes (file-gated)
// ---------------------------------------------------------------------------

/// The three-file contract the rehearsal locked in: (1) `art-mapping.json`
/// carries the two `ui-*` drill entries in the `岩库:<id>@vN` form; (2) the
/// drill lockfile pins versions WITH demand back-links and `ui/` dest paths
/// (ADR-0009 + ADR-0015 shapes); (3) the files art-sync landed are exactly
/// the vault store's bytes for the pinned versions (the byte verification
/// the tool itself can only approximate with sizes — see pitfall F1).
#[test]
#[ignore = "reads drill asset files under assets-art-drill/ (rehearsal evidence) — run with --ignored"]
fn drill_mapping_and_lockfile_contracts() {
    let mapping: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("art-mapping.json")).unwrap(),
    )
    .unwrap();
    let panel_ref = mapping["mapping"]["ui-panel-command-card"].as_str().unwrap();
    let slot_ref = mapping["mapping"]["ui-slot-command"].as_str().unwrap();
    assert!(panel_ref.starts_with("岩库:d011"), "panel drill ref: {panel_ref}");
    assert!(panel_ref.ends_with("@v3"), "mapping @vN follows the lockfile after the upgrade walk: {panel_ref}");
    assert!(slot_ref.starts_with("岩库:d012") && slot_ref.ends_with("@v1"), "{slot_ref}");

    let lock: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(drill_dir().join("first-day/art-assets.json")).unwrap(),
    )
    .unwrap();
    let assets = lock["assets"].as_array().unwrap();
    assert_eq!(assets.len(), 2, "two drill assets");
    let panel = &assets[0];
    assert_eq!(panel["version"], 3, "end state pins the upgraded v3");
    assert_eq!(panel["demand"], "dem-drill-0001", "demand back-link (three-layer traceability)");
    assert_eq!(panel["files"]["drill_panel.png"], "ui/drill_panel.png", "single flat file under assets/ui/");
    assert_eq!(assets[1]["demand"], "dem-drill-0002");

    // Byte verification beyond the tool's size check: the synced files are
    // EXACTLY the vault store bytes for their pinned versions.
    let synced = drill_dir().join("first-day/assets/ui");
    assert_eq!(
        std::fs::read(synced.join("drill_panel.png")).unwrap(),
        std::fs::read(drill_dir().join("vault/drill_panel_v3.png")).unwrap(),
        "synced panel == vault v3 bytes"
    );
    assert_eq!(
        std::fs::read(synced.join("drill_slot.png")).unwrap(),
        std::fs::read(drill_dir().join("vault/drill_slot_v1.png")).unwrap(),
        "synced slot == vault v1 bytes"
    );
}

// ---------------------------------------------------------------------------
// The upgrade rehearsal through the real --art loader path (file-gated)
// ---------------------------------------------------------------------------

/// Load the synced drill manifest into the real loop-td game: every load
/// decodes the CURRENT bytes (v3 = 112x112), bumps the texture revision,
/// and the revision alone re-skins the Plan-A panel roots — the D-phase
/// assertion that a re-synced upgrade visibly lands with zero code change.
/// Swapping to the v1-era vault manifest re-skins again (hot texture swap).
#[test]
#[ignore = "reads drill asset files under assets-art-drill/ (rehearsal evidence) — run with --ignored"]
fn drill_manifest_loads_and_upgrade_revision_rebuilds() {
    let mut app = test_app();
    app.add_plugins(GamePlugin);
    app.insert_resource(MatchRng::seeded(42));
    app.update();
    app.update();

    // First load of the synced (v3) manifest: both slots decode, revision 1.
    let report = load_ui_textures(app.world_mut(), &synced_manifest());
    assert!(report.manifest_ok && report.failed.is_empty(), "{report:?}");
    let mut loaded = report.loaded.clone();
    loaded.sort();
    assert_eq!(loaded, vec!["panel".to_string(), "slot".to_string()]);
    app.insert_resource(ArtMode(true));
    app.update();
    let textures = app.world().resource::<UiTextures>().clone();
    assert_eq!(textures.revision, 1);
    let handle_v3 = textures.handles[UI_TEX_PANEL].clone();
    let size_v3 = app
        .world()
        .resource::<Assets<Image>>()
        .get(&handle_v3)
        .unwrap()
        .size();
    assert_eq!(size_v3, bevy::math::UVec2::new(112, 112), "v3 bytes actually decode at the upgraded size");
    let root = panel_root_of(&mut app, PanelKind::Resource);
    assert_eq!(app.world().get::<ImageNode>(root).unwrap().image, handle_v3);

    // Reload the SAME manifest: new handles, revision 2 -> keyed rebuild
    // swaps the root onto the fresh handle (the sync-then-restart path).
    let report = load_ui_textures(app.world_mut(), &synced_manifest());
    assert!(report.failed.is_empty(), "{report:?}");
    let textures = app.world().resource::<UiTextures>().clone();
    assert_eq!(textures.revision, 2, "reload bumps the revision");
    let handle_reload = textures.handles[UI_TEX_PANEL].clone();
    assert_ne!(handle_reload, handle_v3, "a fresh handle per load");
    app.update();
    let root = panel_root_of(&mut app, PanelKind::Resource);
    assert_eq!(
        app.world().get::<ImageNode>(root).unwrap().image,
        handle_reload,
        "revision bump re-skinned the panel root (keyed rebuild)"
    );

    // Hot texture swap back to the v1-era vault manifest: revision 3, the
    // 96x96 v1 bytes decode, and the root re-skins again on revision alone.
    let report = load_ui_textures(app.world_mut(), &vault_manifest_v1());
    assert!(report.manifest_ok && report.failed.is_empty(), "{report:?}");
    let textures = app.world().resource::<UiTextures>().clone();
    assert_eq!(textures.revision, 3);
    let handle_v1 = textures.handles[UI_TEX_PANEL].clone();
    let size_v1 = app
        .world()
        .resource::<Assets<Image>>()
        .get(&handle_v1)
        .unwrap()
        .size();
    assert_eq!(size_v1, bevy::math::UVec2::new(96, 96), "v1-era bytes decode at the original size");
    app.update();
    let root = panel_root_of(&mut app, PanelKind::Resource);
    assert_eq!(
        app.world().get::<ImageNode>(root).unwrap().image,
        handle_v1,
        "the swap re-skinned on the texture revision alone"
    );
}

// ---------------------------------------------------------------------------
// Offscreen evidence: greybox vs v1 vs upgraded v3 (GPU-gated)
// ---------------------------------------------------------------------------

/// The rehearsal's visual evidence, three PNGs into `docs/img/art-drill/`:
/// the same live HUD rendered OFFSCREEN (the mmp pattern — no window, no
/// display server) first greybox, then textured with the v1-era drill set,
/// then with the upgraded (re-synced v3) drill set. The three forms must
/// render differently: greybox = token colors, v1 = gold ring + hue corners
/// at 96px, v3 = teal palette at 112px.
///   cargo test -p loop-td --test art_drill evidence -- --ignored --nocapture
#[test]
#[ignore = "GPU offscreen render (drill evidence capture only) — run with --ignored"]
fn evidence_offscreen_greybox_v1_v3() {
    use bevy::window::{ExitCondition, WindowPlugin};
    use bevy::winit::WinitPlugin;

    let out_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("docs/img/art-drill");
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
        .add_plugins((warbase::WarbasePlugin, GamePlugin))
        .add_systems(Startup, spawn_offscreen_ui_camera);
    app.finish();
    app.cleanup();

    for _ in 0..60 {
        app.update();
    }
    app.world_mut().insert_resource(warbase::ui::Selection {
        mode: warbase::ui::SelectionMode::Build,
    });
    for _ in 0..10 {
        app.update();
    }

    let target = app.world().resource::<OffscreenTarget>().0.clone();
    capture(&mut app, &target, &out_dir.join("greybox.png"));

    // v1-era drill set (the first art day's morning form).
    let report = load_ui_textures(app.world_mut(), &vault_manifest_v1());
    assert!(report.manifest_ok && report.failed.is_empty(), "{report:?}");
    app.insert_resource(ArtMode(true));
    for _ in 0..10 {
        app.update();
    }
    capture(&mut app, &target, &out_dir.join("art-drill-v1.png"));

    // The upgraded set: re-synced v3 bytes, revision bump, re-skinned.
    let report = load_ui_textures(app.world_mut(), &synced_manifest());
    assert!(report.manifest_ok && report.failed.is_empty(), "{report:?}");
    for _ in 0..10 {
        app.update();
    }
    capture(&mut app, &target, &out_dir.join("art-drill-v3.png"));

    let grey = std::fs::read(out_dir.join("greybox.png")).expect("greybox shot");
    let v1 = std::fs::read(out_dir.join("art-drill-v1.png")).expect("v1 shot");
    let v3 = std::fs::read(out_dir.join("art-drill-v3.png")).expect("v3 shot");
    for (name, bytes) in
        [("greybox.png", &grey), ("art-drill-v1.png", &v1), ("art-drill-v3.png", &v3)]
    {
        assert!(bytes.starts_with(&[0x89, b'P', b'N', b'G']), "{name} PNG signature");
        assert!(bytes.len() > 4096, "{name} suspiciously small ({} B)", bytes.len());
    }
    assert_ne!(grey, v1, "art day visibly changed the HUD");
    assert_ne!(v1, v3, "the upgrade visibly re-skinned the HUD");
    println!(
        "evidence: {} greybox.png ({} B), art-drill-v1.png ({} B), art-drill-v3.png ({} B)",
        out_dir.display(),
        grey.len(),
        v1.len(),
        v3.len()
    );
}

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
