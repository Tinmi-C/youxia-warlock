//! M1 scan engine (read-only): discover assets in both domains, parse
//! meta.json, grep code references, detect stale mtimes.

use crate::json;
use crate::json::Json;
use crate::model::*;
use std::path::{Path, PathBuf};

pub struct ScanResult {
    pub assets: Vec<Asset>,
    pub intake: Vec<IntakeRequest>,
    pub gallery_dir: Option<String>,
}

const SKIP_DIRS: &[&str] = &[".git", "target", "node_modules", ".obsidian", ".dsh", "catalog"];

fn to_rel(root: &Path, p: &Path) -> String {
    p.strip_prefix(root)
        .unwrap_or(p)
        .to_string_lossy()
        .replace('\\', "/")
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<PathBuf> = rd.filter_map(|e| e.ok()).map(|e| e.path()).collect();
    entries.sort();
    for p in entries {
        if p.is_dir() {
            let name = p.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
            if SKIP_DIRS.contains(&name.as_str()) {
                continue;
            }
            walk(&p, out);
        } else {
            out.push(p.clone());
        }
    }
}

fn kind_for(rel: &str, is_model: bool) -> Kind {
    if is_model {
        return Kind::Model;
    }
    let ext = rel.rsplit('.').next().unwrap_or("").to_lowercase();
    match ext.as_str() {
        "png" | "jpg" | "jpeg" | "webp" | "svg" | "ktx2" => Kind::Texture,
        "mp3" | "ogg" | "wav" | "flac" => Kind::Audio,
        "ttf" | "otf" => Kind::Font,
        _ => {
            // ui category is directory-based: anything under assets/ui/
            if rel.to_lowercase().contains("/assets/ui/") {
                Kind::Ui
            } else {
                Kind::Other
            }
        }
    }
}

fn is_model(p: &Path) -> bool {
    matches!(
        p.extension().and_then(|e| e.to_str()).map(|s| s.to_lowercase()).as_deref(),
        Some("glb") | Some("gltf")
    )
}

fn mtime(p: &Path) -> u64 {
    std::fs::metadata(p)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn parse_meta(root: &Path, meta_path: &Path) -> Option<MetaInfo> {
    let text = std::fs::read_to_string(meta_path).ok()?;
    let doc = json::parse(&text).ok()?;
    let dir = meta_path.parent()?;
    let renders: Vec<String> = doc
        .get("images")
        .and_then(|v| v.as_arr())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str())
                .map(|name| to_rel(root, &dir.join(name)))
                .collect()
        })
        .unwrap_or_default();
    let clips: Vec<String> = doc
        .get("animation_clips")
        .and_then(|v| v.as_arr())
        .map(|arr| {
            arr.iter()
                .filter_map(|c| c.get("name").and_then(|n| n.as_str()).map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();
    let materials: Vec<String> = doc
        .get("material_names")
        .and_then(|v| v.as_arr())
        .map(|arr| arr.iter().filter_map(|v| v.as_str()).map(|s| s.to_string()).collect())
        .unwrap_or_default();
    Some(MetaInfo {
        source_gltf: doc.get("source").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        meta_path: to_rel(root, meta_path),
        height_m: doc.get("height_m").and_then(|v| v.as_f64()),
        triangles: doc.get("triangles").and_then(|v| v.as_f64()),
        materials,
        clips,
        has_armature: doc.get("has_armature").and_then(|v| v.as_bool()).unwrap_or(false),
        renders,
        anim: load_anim_index(root, dir),
    })
}

/// Per-clip preview strips produced by tools/art/anim_strip.py (card AC-2).
/// Lives at <meta-dir>/anim/anim_index.json; strip paths are meta-dir relative.
fn load_anim_index(root: &Path, meta_dir: &Path) -> Vec<AnimClip> {
    let idx = meta_dir.join("anim").join("anim_index.json");
    let Ok(text) = std::fs::read_to_string(&idx) else {
        return Vec::new();
    };
    let Ok(doc) = json::parse(&text) else {
        return Vec::new();
    };
    doc.get("clips")
        .and_then(|v| v.as_arr())
        .map(|arr| {
            arr.iter()
                .filter_map(|c| {
                    let name = c.get("name").and_then(|v| v.as_str())?.to_string();
                    let strip_rel = c.get("strip").and_then(|v| v.as_str())?;
                    let frames = c.get("frames").and_then(|v| v.as_f64()).unwrap_or(0.0);
                    let strip = to_rel(root, &meta_dir.join(strip_rel));
                    Some(AnimClip { name, strip, frames })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Search a literal (file name) across all .rs files under the given dirs.
/// Line `//` comments are stripped (heuristic: `//` not preceded by `:`),
/// so stale mentions inside comments do not count as references.
fn strip_line_comment(line: &str) -> &str {
    let b = line.as_bytes();
    let mut i = 0;
    while i + 1 < b.len() {
        if b[i] == b'/' && b[i + 1] == b'/' && (i == 0 || b[i - 1] != b':') {
            return &line[..i];
        }
        i += 1;
    }
    line
}

fn find_refs(root: &Path, dirs: &[String], needle: &str) -> Vec<RefHit> {
    let mut hits = Vec::new();
    for d in dirs {
        let mut files = Vec::new();
        walk(&root.join(d), &mut files);
        for f in files {
            if f.extension().and_then(|e| e.to_str()) != Some("rs") {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&f) else { continue };
            let rel = to_rel(root, &f);
            for (i, line) in text.lines().enumerate() {
                if strip_line_comment(line).contains(needle) {
                    hits.push(RefHit {
                        file: rel.clone(),
                        line: i + 1,
                        snippet: line.trim().chars().take(160).collect(),
                    });
                }
            }
        }
    }
    hits
}

fn push_file_asset(root: &Path, assets: &mut Vec<Asset>, p: &Path, domain: Domain, ref_dirs: &[String]) {
    let name = p.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    if name == ".gitkeep" || name.is_empty() {
        return;
    }
    let rel = to_rel(root, p);
    let id = p
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| name.clone());
    let md = std::fs::metadata(p).ok();
    // ground-truth glb facts: runtime game models only (keeps the scan cheap)
    let is_glb_model = domain == Domain::Game && rel.to_lowercase().ends_with(".glb");
    assets.push(Asset {
        id,
        kind: kind_for(&rel, is_model(p)),
        domain,
        path: rel.clone(),
        size: md.as_ref().map(|m| m.len()).unwrap_or(0),
        modified: mtime(p),
        meta: None,
        glb: if is_glb_model { glb_info(p) } else { None },
        anim_refs: Vec::new(),
        pipeline_status: None,
        refs: find_refs(root, ref_dirs, &name),
        stale_reasons: Vec::new(),
    });
}

// --- animation clip reference heuristics (card feedback #4) -----------------
//
// This codebase references clips by INDEX, not by name:
//   - hero side:   const HERO_CLIP_RUN: usize = 3;  bound to HERO_GLB
//   - monster side: fn walk_clip(self) -> usize => 7  (shared 9-clip layout
//     across all models listed in the definition table's model() match)
// We mirror both patterns with plain string ops (no regex dependency).

struct ClipAssign {
    symbol: String,
    index: usize,
    line: usize,
}

/// First identifier containing `pos` (walk over [A-Za-z0-9_]).
fn ident_at(s: &str, pos: usize) -> String {
    let b = s.as_bytes();
    let mut start = pos;
    while start > 0 && (b[start - 1].is_ascii_alphanumeric() || b[start - 1] == b'_') {
        start -= 1;
    }
    let mut end = pos;
    while end < b.len() && (b[end].is_ascii_alphanumeric() || b[end] == b'_') {
        end += 1;
    }
    s[start..end].to_string()
}

/// First `= <digits>` after `pos` (for `HERO_CLIP_X: usize = 3` style lines).
/// Float assignments (e.g. `WALK_CLIP_AUTHORED_SPEED = 1.4`) are rejected —
/// they are speed constants, not clip indices.
fn index_after_assign(s: &str, pos: usize) -> Option<usize> {
    let eq = s[pos..].find('=')? + pos;
    let rest = &s[eq + 1..];
    let mut digits = String::new();
    for c in rest.chars() {
        if c.is_ascii_digit() {
            digits.push(c);
        } else if digits.is_empty() {
            continue; // skip whitespace / casts before the number
        } else {
            if c == '.' {
                return None; // float literal → not a clip index
            }
            break;
        }
    }
    if digits.is_empty() {
        None
    } else {
        digits.parse().ok()
    }
}

/// For a `fn xxx_clip(self) -> usize` line at `i`, find the first `=> N`
/// within the next few lines (match-arm bodies).
fn index_in_match_ahead(lines: &[&str], i: usize) -> Option<usize> {
    for j in i + 1..(i + 12).min(lines.len()) {
        let code = strip_line_comment(lines[j]);
        if let Some(p) = code.find("=>") {
            let ds: String = code[p + 2..]
                .chars()
                .skip_while(|c| !c.is_ascii_digit())
                .take_while(|c| c.is_ascii_digit())
                .collect();
            if let Ok(n) = ds.parse::<usize>() {
                return Some(n);
            }
        }
    }
    None
}

fn find_anim_refs(root: &Path, dirs: &[String], assets: &mut [Asset]) {
    use std::collections::BTreeMap;
    let mut by_name: BTreeMap<String, usize> = BTreeMap::new();
    for (i, a) in assets.iter().enumerate() {
        if a.domain == Domain::Game && a.kind == Kind::Model {
            by_name.insert(a.file_name(), i);
        }
    }
    if by_name.is_empty() {
        return;
    }
    for d in dirs {
        let mut files = Vec::new();
        walk(&root.join(d), &mut files);
        for f in files {
            if f.extension().and_then(|e| e.to_str()) != Some("rs") {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&f) else { continue };
            let rel = to_rel(root, &f);
            let lines: Vec<&str> = text.lines().collect();
            let mut managed: Vec<String> = Vec::new(); // model file names in this file
            let mut hero_model: Option<String> = None;
            let mut assigns: Vec<ClipAssign> = Vec::new();
            for (i, raw) in lines.iter().enumerate() {
                let code = strip_line_comment(raw);
                if let Some(pos) = code.find("models/") {
                    if let Some(end) = code[pos..].find(".glb") {
                        let name = code[pos + 7..pos + end + 4].to_string();
                        if !managed.contains(&name) {
                            managed.push(name.clone());
                        }
                        if code.contains("HERO_GLB") {
                            hero_model = Some(name);
                        }
                    }
                }
                if let Some(pos) = code.find("_CLIP_") {
                    if let Some(idx) = index_after_assign(code, pos) {
                        assigns.push(ClipAssign { symbol: ident_at(code, pos), index: idx, line: i + 1 });
                    }
                } else if code.contains("fn ") {
                    if let Some(fp) = code.find("_clip(") {
                        let fname = ident_at(code, fp);
                        if let Some(idx) = index_in_match_ahead(&lines, i) {
                            assigns.push(ClipAssign { symbol: fname, index: idx, line: i + 1 });
                        }
                    }
                }
            }
            if assigns.is_empty() {
                continue;
            }
            for ca in &assigns {
                let mut targets: Vec<String> = Vec::new();
                if ca.symbol.contains("HERO_CLIP") {
                    if let Some(h) = &hero_model {
                        targets.push(h.clone());
                    }
                } else {
                    // definition-table clip fns apply to every managed model
                    targets.extend(managed.iter().cloned());
                }
                for t in targets {
                    if let Some(&ai) = by_name.get(&t) {
                        assets[ai].anim_refs.push(AnimRef {
                            clip_index: ca.index,
                            symbol: ca.symbol.clone(),
                            file: rel.clone(),
                            line: ca.line,
                        });
                    }
                }
            }
        }
    }
    // keep refs stable per asset
    for a in assets.iter_mut() {
        a.anim_refs.sort_by(|x, y| (&x.file, x.line).cmp(&(&y.file, y.line)));
    }
}

/// Parse the GLB container's JSON chunk for ground-truth facts (animations,
/// skins, meshes) — what is really inside the runtime file, not what
/// meta.json claims. Returns None for non-GLB files or malformed containers.
fn glb_info(path: &Path) -> Option<GlbInfo> {
    let data = std::fs::read(path).ok()?;
    if data.len() < 24 || &data[0..4] != b"glTF" {
        return None;
    }
    let clen = u32::from_le_bytes([data[12], data[13], data[14], data[15]]) as usize;
    if &data[16..20] != b"JSON" || data.len() < 20 + clen {
        return None;
    }
    let text = String::from_utf8_lossy(&data[20..20 + clen]);
    let doc = json::parse(&text).ok()?;
    let accessors = doc.get("accessors").and_then(|v| v.as_arr());
    let mut animations = Vec::new();
    let mut durations = Vec::new();
    if let Some(arr) = doc.get("animations").and_then(|v| v.as_arr()) {
        for a in arr {
            animations.push(a.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string());
            durations.push(anim_duration(a, accessors));
        }
    }
    let count = |key: &str| {
        doc.get(key)
            .and_then(|v| v.as_arr())
            .map(|a| a.len() as f64)
            .unwrap_or(0.0)
    };
    Some(GlbInfo {
        animations,
        durations,
        skins: count("skins"),
        meshes: count("meshes"),
    })
}

/// Animation duration in seconds = max over samplers of the INPUT accessor's
/// declared max[0] (glTF stores the time range there; no buffer decoding).
fn anim_duration(anim: &Json, accessors: Option<&[Json]>) -> f64 {
    let mut max_t = 0f64;
    if let Some(samplers) = anim.get("samplers").and_then(|v| v.as_arr()) {
        for s in samplers {
            let input_idx = match s.get("input").and_then(|v| v.as_f64()) {
                Some(i) => i as usize,
                None => continue,
            };
            let t = accessors
                .and_then(|accs| accs.get(input_idx))
                .and_then(|acc| acc.get("max"))
                .and_then(|v| v.as_arr())
                .and_then(|m| m.first())
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0);
            if t > max_t {
                max_t = t;
            }
        }
    }
    max_t
}

pub struct Layout {
    pub library_dir: Option<String>,
    pub raw: Option<String>,
    pub gallery: Option<String>,
    pub washed: Option<String>,
    pub intake: Option<String>,
    pub legacy: bool,
}

/// Resolve library layout: prefer workspace `_library/`, fall back to the
/// pre-migration per-game `_art/` layout so the tool works before/after the move.
/// All section paths are repo-relative.
pub fn resolve_layout(root: &Path, game_dir: &str) -> Layout {
    if root.join("_library").is_dir() {
        return Layout {
            library_dir: Some("_library".into()),
            raw: rel_if_dir(root, "_library/raw"),
            gallery: rel_if_dir(root, "_library/gallery"),
            washed: rel_if_dir(root, "_library/washed"),
            intake: rel_if_dir(root, "_library/intake"),
            legacy: false,
        };
    }
    // legacy per-game layout
    let art_rel = format!("{game_dir}/_art");
    if root.join(&art_rel).is_dir() {
        return Layout {
            library_dir: Some(art_rel.clone()),
            raw: rel_if_dir(root, &format!("{art_rel}/raw")),
            gallery: rel_if_dir(root, &format!("{art_rel}/gallery")),
            washed: rel_if_dir(root, &format!("{art_rel}/gallery-washed")),
            intake: rel_if_dir(root, &format!("{art_rel}/intake")),
            legacy: true,
        };
    }
    Layout { library_dir: None, raw: None, gallery: None, washed: None, intake: None, legacy: false }
}

fn rel_if_dir(root: &Path, rel: &str) -> Option<String> {
    if root.join(rel).is_dir() {
        Some(rel.to_string())
    } else {
        None
    }
}

impl Layout {
    /// Explicit --library override: sections live directly under the given dir.
    pub fn explicit(root: &Path, lib: &str) -> Layout {
        Layout {
            library_dir: Some(lib.to_string()),
            raw: rel_if_dir(root, &format!("{lib}/raw")),
            gallery: rel_if_dir(root, &format!("{lib}/gallery")),
            washed: rel_if_dir(root, &format!("{lib}/washed")),
            intake: rel_if_dir(root, &format!("{lib}/intake")),
            legacy: false,
        }
    }
}

fn scan_domain_dir(root: &Path, dir: &str, domain: Domain, ref_dirs: &[String], assets: &mut Vec<Asset>) {
    let mut files = Vec::new();
    walk(&root.join(dir), &mut files);
    for f in files {
        push_file_asset(root, assets, &f, domain, ref_dirs);
    }
}

pub fn scan(root: &Path, game_dir: &str, layout: &Layout) -> ScanResult {
    let ref_dirs = vec![format!("{game_dir}/src"), format!("{game_dir}/tests")];
    let mut assets: Vec<Asset> = Vec::new();

    // Game domain
    scan_domain_dir(root, &format!("{game_dir}/assets"), Domain::Game, &ref_dirs, &mut assets);

    // Library domain (raw / gallery / washed shelf)
    for section in [&layout.raw, &layout.gallery, &layout.washed] {
        if let Some(d) = section {
            scan_domain_dir(root, d, Domain::Library, &ref_dirs, &mut assets);
        }
    }

    // Attach meta by stem: prefer washed shelf, then candidate gallery
    let mut meta_by_stem: std::collections::HashMap<String, MetaInfo> = std::collections::HashMap::new();
    for section in [&layout.washed, &layout.gallery] {
        if let Some(d) = section {
            let base = root.join(d);
            let Ok(rd) = std::fs::read_dir(&base) else { continue };
            for e in rd.filter_map(|e| e.ok()) {
                let p = e.path();
                if !p.is_dir() {
                    continue;
                }
                let stem = p.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
                let meta_path = p.join("meta.json");
                if meta_path.is_file() {
                    if let Some(m) = parse_meta(root, &meta_path) {
                        meta_by_stem.entry(stem).or_insert(m);
                    }
                }
            }
        }
    }
    for a in assets.iter_mut() {
        if a.domain == Domain::Game && a.kind == Kind::Model {
            if let Some(m) = meta_by_stem.get(&a.id) {
                a.meta = Some(m.clone());
                let glb_mtime = a.modified;
                let meta_p = root.join(&m.meta_path);
                if mtime(&meta_p) < glb_mtime {
                    a.stale_reasons.push("meta.json 比运行时模型旧（图册可能过期，重跑 turntable）".into());
                }
                for r in &m.renders {
                    if mtime(&root.join(r)) < glb_mtime {
                        a.stale_reasons.push(format!("渲染图比模型旧：{r}"));
                    }
                }
            }
        }
        // library shelf: same-named raw newer than the shelf copy
        if a.domain == Domain::Library && a.kind == Kind::Model {
            if let Some(raw_dir) = &layout.raw {
                let raw_same = root.join(raw_dir).join(a.file_name());
                if raw_same.is_file() && mtime(&raw_same) > a.modified {
                    a.stale_reasons.push("库内原始同名文件比该成品新（需要重洗）".into());
                }
            }
        }
    }

    // Intake requests
    let mut intake = Vec::new();
    if let Some(d) = &layout.intake {
        let mut files = Vec::new();
        walk(&root.join(d), &mut files);
        for f in files {
            if f.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&f) else { continue };
            let Ok(doc) = json::parse(&text) else { continue };
            intake.push(IntakeRequest {
                file: to_rel(root, &f),
                raw: doc,
            });
        }
    }

    // Pipeline status from intake tickets (card AC-3): match by raw file path
    // or promotion target path so both raw originals and landed models show
    // where they sit in the raw→gallery flow.
    {
        use std::collections::BTreeMap;
        let mut by_path: BTreeMap<String, String> = BTreeMap::new();
        for t in &intake {
            let status = t
                .raw
                .get("status")
                .and_then(|v| v.as_str())
                .unwrap_or("new")
                .to_string();
            for key in ["raw_file", "target"] {
                if let Some(p) = t.raw.get(key).and_then(|v| v.as_str()) {
                    by_path.insert(p.to_string(), status.clone());
                }
            }
        }
        for a in assets.iter_mut() {
            a.pipeline_status = by_path.get(&a.path).cloned();
        }
    }

    find_anim_refs(root, &ref_dirs, &mut assets);

    ScanResult {
        assets,
        intake,
        gallery_dir: layout.gallery.clone(),
    }
}

/// Load scenario cards; project dir entries override built-ins by id.
pub fn load_scenarios(root: &Path, builtin_dir: &Path, project_dir: Option<&Path>) -> Vec<ScenarioCard> {
    let mut map: std::collections::BTreeMap<String, ScenarioCard> = std::collections::BTreeMap::new();
    for dir in [Some(builtin_dir), project_dir] {
        let Some(d) = dir else { continue };
        let Ok(rd) = std::fs::read_dir(d) else { continue };
        let mut files: Vec<PathBuf> = rd.filter_map(|e| e.ok()).map(|e| e.path()).collect();
        files.sort();
        for f in files {
            if f.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&f) else { continue };
            let Ok(doc) = json::parse(&text) else { continue };
            let id = doc.get("id").and_then(|v| v.as_str()).unwrap_or("?").to_string();
            let steps = doc.get("steps").and_then(|v| v.as_arr()).map(|a| a.len()).unwrap_or(0);
            let human_steps = doc
                .get("steps")
                .and_then(|v| v.as_arr())
                .map(|a| {
                    a.iter()
                        .filter(|s| s.get("executor").and_then(|e| e.as_str()) == Some("human"))
                        .count()
                })
                .unwrap_or(0);
            map.insert(
                id.clone(),
                ScenarioCard {
                    file: to_rel(root, &f),
                    id,
                    name: doc.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                    trigger: doc.get("trigger").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                    steps,
                    human_steps,
                    raw: doc,
                },
            );
        }
    }
    map.into_values().collect()
}

/// Dry-run validation of a scenario card: required fields + check_paths existence.
pub fn scenario_dry_run(root: &Path, card: &ScenarioCard) -> Result<Vec<String>, String> {
    let mut notes = Vec::new();
    let d = &card.raw;
    for field in ["id", "name", "trigger", "steps", "acceptance"] {
        if d.get(field).is_none() {
            return Err(format!("卡 {} 缺少必填字段 `{field}`", card.id));
        }
    }
    let steps = d.get("steps").and_then(|v| v.as_arr()).ok_or("steps 必须是数组")?;
    if steps.is_empty() {
        return Err(format!("卡 {} steps 为空", card.id));
    }
    for (i, s) in steps.iter().enumerate() {
        let ex = s.get("executor").and_then(|e| e.as_str()).unwrap_or("");
        if !matches!(ex, "human" | "ai-assist" | "auto") {
            return Err(format!("卡 {} 第 {} 步 executor 非法：`{ex}`", card.id, i + 1));
        }
        if s.get("do").and_then(|v| v.as_str()).unwrap_or("").is_empty() {
            return Err(format!("卡 {} 第 {} 步缺少 do", card.id, i + 1));
        }
        if let Some(paths) = s.get("check_paths").and_then(|v| v.as_arr()) {
            for p in paths {
                let ps = p.as_str().unwrap_or("");
                let abs = root.join(ps);
                if !abs.exists() {
                    notes.push(format!("卡 {} 第 {} 步 check_paths 不存在：{ps}", card.id, i + 1));
                }
            }
        }
    }
    Ok(notes)
}
