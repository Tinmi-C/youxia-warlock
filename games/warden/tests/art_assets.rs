//! Art asset gates (ART series): offline structural checks on the bundled
//! GLB files — no engine, no window, no renderer, so they run with every
//! `cargo test` in the background and never touch the desktop.
//!
//! The art test pyramid (docs/topics/engine/automated-game-testing-and-ai.md):
//!   L0 file/manifest pin — behavior.rs `arrow_tower_asset_is_pinned`
//!   L1 container + repo conventions — this file (GLB header sanity,
//!      snake_case naming, 15 MiB cap, manifest ⇄ directory cross-check)
//!   L2 geometry facts — this file (bbox normalization, mesh/texture
//!      counts, animation absence, ballista long axis ±X: the mounting
//!      assumption ART2's assembly math depends on)
//!   L3 engine assembly pin — behavior.rs `archer_tower_assembly_is_pinned`
//!   L4 visual regression — TEST2 offscreen screenshots (planned)
//!
//! L2 exists because an asset swap that silently breaks an assumption
//! (rotated geometry, un-normalized scale, new animations) looks fine at the
//! file level and only explodes visually — these tests make it fail fast.

use serde_json::Value;

/// A parsed GLB: the glTF 2.0 JSON document (binary chunk not needed here).
struct Glb {
    json: Value,
}

fn load_glb(path: &std::path::Path) -> Glb {
    let bytes = std::fs::read(path).unwrap_or_else(|e| panic!("read {path:?}: {e}"));
    assert!(bytes.len() >= 20, "GLB too short for header + first chunk");
    let u32at = |o: usize| u32::from_le_bytes(bytes[o..o + 4].try_into().unwrap());
    assert_eq!(u32at(0), 0x4654_6C67, "GLB magic must be 'glTF'");
    assert_eq!(u32at(4), 2, "container version must be 2");
    assert_eq!(
        u32at(8) as usize,
        bytes.len(),
        "declared total length must match the file size"
    );
    assert_eq!(u32at(16), 0x4E4F_534A, "first chunk must be JSON");
    let clen = u32at(12) as usize;
    let json =
        serde_json::from_slice(&bytes[20..20 + clen]).expect("JSON chunk must parse");
    Glb { json }
}

/// min/max of the first mesh primitive's POSITION accessor (f64 triples).
fn position_bbox(glb: &Glb) -> ([f64; 3], [f64; 3]) {
    let idx = glb.json["meshes"][0]["primitives"][0]["attributes"]["POSITION"]
        .as_u64()
        .expect("POSITION accessor index") as usize;
    let acc = &glb.json["accessors"][idx];
    let triple = |v: &Value| {
        let a = v.as_array().expect("min/max array");
        [
            a[0].as_f64().expect("f64"),
            a[1].as_f64().expect("f64"),
            a[2].as_f64().expect("f64"),
        ]
    };
    (triple(&acc["min"]), triple(&acc["max"]))
}

/// Index count of the first mesh primitive (0 when non-indexed).
fn index_count(glb: &Glb) -> u64 {
    match glb.json["meshes"][0]["primitives"][0]["indices"].as_u64() {
        Some(i) => glb.json["accessors"][i as usize]["count"].as_u64().unwrap_or(0),
        None => 0,
    }
}

fn models_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/models")
}

/// L1: every bundled model follows the ADR-0009 repo conventions — single
/// level under assets/models/, snake_case names, 15 MiB per-file cap — and
/// the art-sync manifest agrees with the directory in BOTH directions
/// (every listed file exists; every .glb is listed, so nothing sneaks in
/// outside the vault lock).
#[test]
fn models_dir_conventions_and_manifest_agree() {
    let mut glbs = Vec::new();
    for entry in std::fs::read_dir(models_dir()).expect("assets/models exists") {
        let entry = entry.expect("dir entry");
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        assert!(
            !entry.path().is_dir(),
            "models/ must stay single-level (ADR-0009): {name}"
        );
        assert!(
            name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '.'),
            "model names are snake_case: {name}"
        );
        assert!(name.ends_with(".glb"), "only .glb ships in models/: {name}");
        assert!(
            entry.metadata().unwrap().len() <= 15 * 1024 * 1024,
            "{name} exceeds the 15 MiB cap"
        );
        glbs.push(name);
    }
    assert!(!glbs.is_empty(), "the game ships at least one model");

    let manifest: Value = serde_json::from_str(
        &std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("art-assets.json"),
        )
        .expect("art-assets.json"),
    )
    .expect("manifest JSON");
    let listed: Vec<String> = manifest["assets"]
        .as_array()
        .expect("assets array")
        .iter()
        .flat_map(|a| {
            a["files"]
                .as_object()
                .expect("files map")
                .values()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect::<Vec<_>>()
        })
        .collect();
    for rel in &listed {
        let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("assets")
            .join(rel);
        assert!(p.is_file(), "manifest lists {rel} but the file is missing");
    }
    for name in &glbs {
        let rel = format!("models/{name}");
        assert!(
            listed.contains(&rel),
            "{name} exists in models/ but is NOT in art-assets.json — assets enter only via the vault lock (ADR-0009)"
        );
    }
}

/// L2: the arrow tower's geometry facts (ArtVault v1 handoff: Rodin
/// image-to-3D, ~23332 tris, one mesh, three textures, bbox ±0.95, no
/// animations). bbox tolerance 1.0 absorbs exporter noise.
#[test]
fn arrow_tower_geometry_facts() {
    let glb = load_glb(&models_dir().join("tower_arrow.glb"));
    let meshes = glb.json["meshes"].as_array().map(Vec::len).unwrap_or(0);
    assert_eq!(meshes, 1, "handoff says one mesh, got {meshes}");
    let images = glb.json["images"].as_array().map(Vec::len).unwrap_or(0);
    assert_eq!(images, 3, "handoff says three textures, got {images}");
    let anims = glb.json["animations"].as_array().map(Vec::len).unwrap_or(0);
    assert_eq!(anims, 0, "no baked animations expected, got {anims}");
    let (min, max) = position_bbox(&glb);
    for axis in 0..3 {
        assert!(
            min[axis] >= -1.0 && max[axis] <= 1.0,
            "bbox must stay normalized (±0.95 handoff, tol 1.0): min={min:?} max={max:?}"
        );
    }
    let tris = index_count(&glb) / 3;
    assert!(
        (20_000..=30_000).contains(&tris),
        "~23332 tris expected (tolerance band), got {tris}"
    );
}

/// L2: the ballista's geometry facts (ArtVault v2 handoff). The critical
/// one is the long axis: ART2's assembly assumes the weapon's length runs
/// along ±X — a future vault version that ships rotated geometry keeps every
/// file-level check green but breaks the mounting, so pin it here.
#[test]
fn ballista_geometry_facts() {
    let glb = load_glb(&models_dir().join("weapon_ballista.glb"));
    let meshes = glb.json["meshes"].as_array().map(Vec::len).unwrap_or(0);
    assert!(meshes >= 1, "at least one mesh expected");
    let anims = glb.json["animations"].as_array().map(Vec::len).unwrap_or(0);
    assert_eq!(anims, 0, "assembly assumes no baked animations, got {anims}");
    let (min, max) = position_bbox(&glb);
    let extent = |a: usize| max[a] - min[a];
    assert!(
        extent(0) > extent(2),
        "long axis must be ±X (mounting assumption): X={} Z={}",
        extent(0),
        extent(2)
    );
    assert!(
        extent(0).is_finite() && extent(1).is_finite() && extent(2).is_finite(),
        "bbox must be finite: min={min:?} max={max:?}"
    );
    let tris = index_count(&glb);
    assert!(tris > 0, "indexed geometry expected");
}

fn textures_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/textures")
}

/// L1 for textures (ART3): the same ADR-0009 repo conventions as models/ —
/// single level, snake_case, jpg-only, 15 MiB cap, JPEG magic bytes — and
/// the art-sync manifest agrees with the directory in BOTH directions.
#[test]
fn textures_dir_conventions_and_manifest_agree() {
    let mut jpgs = Vec::new();
    for entry in std::fs::read_dir(textures_dir()).expect("assets/textures exists") {
        let entry = entry.expect("dir entry");
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        assert!(
            !entry.path().is_dir(),
            "textures/ must stay single-level (ADR-0009): {name}"
        );
        assert!(
            name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '.'),
            "texture names are snake_case: {name}"
        );
        assert!(name.ends_with(".jpg"), "only .jpg ships in textures/: {name}");
        assert!(
            entry.metadata().unwrap().len() <= 15 * 1024 * 1024,
            "{name} exceeds the 15 MiB cap"
        );
        let bytes = std::fs::read(entry.path()).unwrap();
        assert_eq!(&bytes[0..2], &[0xFF, 0xD8], "{name} is not a JPEG");
        jpgs.push(name);
    }
    assert!(!jpgs.is_empty(), "the game ships at least one texture");

    let manifest: Value = serde_json::from_str(
        &std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("art-assets.json"),
        )
        .expect("art-assets.json"),
    )
    .expect("manifest JSON");
    let listed: Vec<String> = manifest["assets"]
        .as_array()
        .expect("assets array")
        .iter()
        .flat_map(|a| {
            a["files"]
                .as_object()
                .expect("files map")
                .values()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect::<Vec<_>>()
        })
        .collect();
    for name in &jpgs {
        let rel = format!("textures/{name}");
        assert!(
            listed.contains(&rel),
            "{name} exists in textures/ but is NOT in art-assets.json — assets enter only via the vault lock (ADR-0009)"
        );
    }
}

/// L2: ART4 masonry geometry facts. The mounting assumptions `spawn_structures`
/// depends on: the gatehouse passage runs along ±X (so rotation 0 faces the
/// arriving path), heights stay human-scale for the arena, and the arch spans
/// X locally (rotated a quarter turn in-game to straddle the path).
///
/// Multi-mesh assemblies (16 objects in the gatehouse): aggregate the
/// accessor min/max of every primitive's POSITION, not just the first mesh.
fn scene_bbox(glb: &Glb) -> ([f64; 3], [f64; 3]) {
    let mut min = [f64::MAX; 3];
    let mut max = [f64::MIN; 3];
    let mut used = 0;
    for mesh in glb.json["meshes"].as_array().into_iter().flatten() {
        for prim in mesh["primitives"].as_array().into_iter().flatten() {
            let Some(idx) = prim["attributes"]["POSITION"].as_u64() else {
                continue;
            };
            let acc = &glb.json["accessors"][idx as usize];
            let pick = |v: &Value, i: usize| {
                v.as_array().and_then(|a| a.get(i)).and_then(|x| x.as_f64())
            };
            for i in 0..3 {
                if let Some(lo) = pick(&acc["min"], i) {
                    min[i] = min[i].min(lo);
                }
                if let Some(hi) = pick(&acc["max"], i) {
                    max[i] = max[i].max(hi);
                }
            }
            used += 1;
        }
    }
    assert!(used > 0, "no POSITION accessors found");
    (min, max)
}

#[test]
fn structures_geometry_facts() {
    let (gh_min, gh_max) = scene_bbox(&load_glb(&models_dir().join("structure_gatehouse.glb")));
    let (ea_min, ea_max) = scene_bbox(&load_glb(&models_dir().join("structure_entry_arch.glb")));
    let extent = |min: &[f64; 3], max: &[f64; 3], a: usize| max[a] - min[a];
    assert!(
        extent(&gh_min, &gh_max, 0) > extent(&gh_min, &gh_max, 2),
        "gatehouse passage must run along ±X: X={} Z={}",
        extent(&gh_min, &gh_max, 0),
        extent(&gh_min, &gh_max, 2)
    );
    assert!(
        (3.0..=5.5).contains(&extent(&gh_min, &gh_max, 1)),
        "gatehouse height band 3.0-5.5 (flags included), got {}",
        extent(&gh_min, &gh_max, 1)
    );
    assert!(
        extent(&ea_min, &ea_max, 0) > extent(&ea_min, &ea_max, 2),
        "entry arch must span ±X locally, got X={} Z={}",
        extent(&ea_min, &ea_max, 0),
        extent(&ea_min, &ea_max, 2)
    );
    assert!(
        (1.5..=4.0).contains(&extent(&ea_min, &ea_max, 1)),
        "entry arch height band 1.5-4.0, got {}",
        extent(&ea_min, &ea_max, 1)
    );
}
