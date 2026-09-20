//! asset-viewer — the zero-integration consumer of the showroom crate
//! (ADR-0010): point it at a directory and browse every .glb/.gltf in it,
//! in the same neutral studio the games use.
//!
//!     cargo run -p showroom-viewer --bin asset-viewer -- <dir>
//!
//! The directory becomes the bevy asset root, so scenes load by their
//! path relative to it (subdirectories included). Each file becomes one
//! catalog item; the file name is the label. Works on any folder — a
//! game's assets/, an ArtVault export, a download dir.

use bevy::prelude::*;
use showroom_viewer::{
    ShowroomCatalog, ShowroomItem, ShowroomPlugin, ShowroomState, SpawnCtx, enter_studio,
};

fn main() {
    let mut dir = None;
    let mut gallery = false;
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "--gallery" => gallery = true,
            other => dir = Some(other.to_string()),
        }
    }
    let dir = dir.unwrap_or_else(|| {
        eprintln!("usage: asset-viewer <directory-with-glb-files> [--gallery]");
        std::process::exit(2);
    });
    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .set(AssetPlugin {
                file_path: dir.clone(),
                ..default()
            })
            .set(WindowPlugin {
                primary_window: Some(Window {
                    title: format!("Showroom — {dir}"),
                    resolution: (1280, 720).into(),
                    ..default()
                }),
                ..default()
            }),
    )
    .add_plugins(ShowroomPlugin)
    .insert_resource(ScanDir(dir))
    .insert_resource(StartInGallery(gallery))
    .add_systems(Startup, scan_and_enter);
    app.run();
}

/// Startup flag: land straight in the grid overview (--gallery).
#[derive(Resource)]
struct StartInGallery(bool);

#[derive(Resource)]
struct ScanDir(String);

/// One found file: its path relative to the scanned dir (the asset key)
/// plus its label (file name without extension).
struct Found {
    rel: String,
    label: String,
}

fn walk(dir: &std::path::Path, prefix: &str, out: &mut Vec<Found>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        let Ok(ft) = e.file_type() else {
            continue;
        };
        let name = e.file_name().to_string_lossy().into_owned();
        if ft.is_dir() {
            walk(&e.path(), &format!("{prefix}{name}/"), out);
            continue;
        }
        let is_gltf = e
            .path()
            .extension()
            .map(|x| {
                let x = x.to_ascii_lowercase();
                x == "glb" || x == "gltf"
            })
            .unwrap_or(false);
        if !is_gltf {
            continue;
        }
        let label = name
            .strip_suffix(".glb")
            .or_else(|| name.strip_suffix(".gltf"))
            .or_else(|| name.strip_suffix(".GLB"))
            .or_else(|| name.strip_suffix(".GLTF"))
            .unwrap_or(&name)
            .to_string();
        out.push(Found {
            rel: format!("{prefix}{name}"),
            label,
        });
    }
}

/// Scan the directory on startup, register every glTF file as a catalog
/// item and enter the studio. No custom font => the egui list names the
/// items (no world-space plates in the standalone bin).
fn scan_and_enter(world: &mut World) {
    let dir = world.resource::<ScanDir>().0.clone();
    let mut found = Vec::new();
    walk(std::path::Path::new(&dir), "", &mut found);
    if found.is_empty() {
        eprintln!("no .glb/.gltf files found in {dir}");
        std::process::exit(2);
    }
    found.sort_by(|a, b| a.rel.cmp(&b.rel));
    info!("[sm] asset-viewer: {} files from {}", found.len(), dir);
    let root_dir = std::path::Path::new(&dir)
        .canonicalize()
        .unwrap_or_else(|_| std::path::PathBuf::from(&dir));
    let server = world.resource::<AssetServer>();
    let mut catalog = ShowroomCatalog::build().category("Files");
    for f in found {
        let size = std::fs::metadata(root_dir.join(&f.rel))
            .map(|m| format!("{:.1} MiB", m.len() as f64 / (1 << 20) as f64))
            .unwrap_or_else(|_| "? MiB".into());
        let info_rows = vec![("Size".to_string(), size)];
        let tags = vec![f.label.clone()];
        let rel = f.rel.clone();
        // Whole-file handle: named clips feed the studio's clip browser.
        let gltf = server.load::<bevy::gltf::Gltf>(rel.clone());
        catalog = catalog.item(ShowroomItem {
            name: f.label.clone(),
            info_rows,
            tags,
            origin: vec![],
            plate: None,
            anchor_y: 0.0,
            plate_y: 2.0,
            gltf: Some(gltf),
            parts: vec![],
            spawn: Box::new(
                move |commands: &mut Commands, root: Entity, ctx: &mut SpawnCtx| {
                    let Some(server) = ctx.server else {
                        return;
                    };
                    let scene = server.load(
                        bevy::gltf::GltfAssetLabel::Scene(0).from_asset(rel.clone()),
                    );
                    commands.entity(root).with_children(|p| {
                        p.spawn((
                            bevy::world_serialization::WorldAssetRoot(scene),
                            Transform::default(),
                        ));
                    });
                },
            ),
        });
    }
    let catalog = catalog.finish();
    // The bin has its own camera; the studio expects exactly that.
    world.spawn((
        Camera3d::default(),
        Transform::from_xyz(4.6, 4.4, 4.6).looking_at(Vec3::Y * 1.0, Vec3::Y),
    ));
    enter_studio(world, catalog, FontSource::default());
    if world
        .remove_resource::<StartInGallery>()
        .map(|f| f.0)
        .unwrap_or(false)
    {
        world.resource_mut::<ShowroomState>().gallery = true;
    }
}
