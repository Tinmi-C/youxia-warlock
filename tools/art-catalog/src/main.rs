//! art-catalog: read-only art asset catalog (card AC-1).
//! Scan both domains -> checks R1-R7 -> write index.html + catalog.json +
//! report.json into the output dir. Exit code = finding count (cap 100).

mod checks;
mod html;
mod intake;
mod json;
mod model;
mod scan;

use json::Json;
use model::*;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// three.js r147 (UMD) + GLTFLoader + OrbitControls, inlined into the page so
/// the 3D animation viewer works from file:// with zero network/vendor setup.
const VENDOR3D: &str = concat!(
    include_str!("../assets/three.min.js"),
    "\n",
    include_str!("../assets/GLTFLoader.js"),
    "\n",
    include_str!("../assets/OrbitControls.js")
);

/// Max glb size to embed as base64 (matches R7 big-file threshold intent).
const EMBED_MAX_BYTES: u64 = 6 * 1024 * 1024;

fn b64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = chunk.get(1).copied().unwrap_or(0) as u32;
        let b2 = chunk.get(2).copied().unwrap_or(0) as u32;
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 { T[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if chunk.len() > 2 { T[n as usize & 63] as char } else { '=' });
    }
    out
}

struct Args {
    root: Option<String>,
    game: Option<String>,
    library: Option<String>,
    out: Option<String>,
    scan_only: bool,
    scenario_check: bool,
}

fn parse_args() -> Args {
    let mut a = Args {
        root: None,
        game: None,
        library: None,
        out: None,
        scan_only: false,
        scenario_check: false,
    };
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let mut it = std::env::args().skip(1);
    // `art-catalog intake …` and `art-catalog wash …` are separate subcommand
    // families (cards AC-3/AC-4): resolve root/game the same way, then
    // delegate to the promotion state machine.
    if matches!(argv.first().map(|s| s.as_str()), Some("intake") | Some("wash")) {
        let rest: Vec<String> = argv.clone();
        let get = |k: &str| {
            rest.iter()
                .position(|a| a == k)
                .and_then(|i| rest.get(i + 1))
                .cloned()
        };
        let root = find_root(get("--root").as_deref());
        let root = root.canonicalize().unwrap_or(root);
        // strip the verbatim `\\?\` prefix — Blender and other children choke
        // on it in argv
        let root_str = root.to_string_lossy().replace("\\\\?\\", "");
        let root = PathBuf::from(root_str);
        let game = get("--game").or_else(|| discover_game(&root));
        let Some(game) = game else {
            eprintln!("intake 需要 --game（仓库内有多个或零个游戏时无法自动判断）");
            std::process::exit(2);
        };
        let layout = scan::resolve_layout(&root, &game);
        let date: String = now_string().chars().take(10).collect();
        let code = intake::run(&root, &game, &layout, &rest, &date);
        std::process::exit(code);
    }
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--root" => a.root = it.next(),
            "--game" => a.game = it.next(),
            "--library" => a.library = it.next(),
            "--out" => a.out = it.next(),
            "--scan-only" => a.scan_only = true,
            "--scenario-check" => a.scenario_check = true,
            "--help" | "-h" => {
                print_help();
                std::process::exit(0);
            }
            other => {
                eprintln!("未知参数：{other}（--help 查看用法）");
                std::process::exit(2);
            }
        }
    }
    a
}

fn print_help() {
    println!(
        "art-catalog — 只读美术资产目录（人读 HTML / 机读 JSON 双出口）\n\
         用法:\n  art-catalog [--root <repo>] [--game <dir>] [--library <dir>] [--out <dir>]\n\
         \x20             [--scan-only] [--scenario-check]\n\
         默认: 自动发现 games/ 下唯一含 assets/ 的游戏；库优先 _library/，\n\
         \x20     不存在则回落到 <game>/_art/ 迁移前布局。退出码 = 检查问题数。"
    );
}

fn find_root(explicit: Option<&str>) -> PathBuf {
    if let Some(r) = explicit {
        return PathBuf::from(r);
    }
    let mut cur = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    for _ in 0..6 {
        if cur.join("games").is_dir() {
            return cur;
        }
        if !cur.pop() {
            break;
        }
    }
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

fn discover_game(root: &Path) -> Option<String> {
    let rd = std::fs::read_dir(root.join("games")).ok()?;
    let mut found = Vec::new();
    for e in rd.filter_map(|e| e.ok()) {
        let p = e.path();
        if p.is_dir() && p.join("assets").is_dir() && p.join("Cargo.toml").is_file() {
            found.push(p.file_name()?.to_string_lossy().to_string());
        }
    }
    if found.len() == 1 {
        Some(format!("games/{}", found[0]))
    } else {
        None
    }
}

fn now_string() -> String {
    let d = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
    let secs = d.as_secs() + 8 * 3600; // team works in UTC+8
    // civil-from-days (Howard Hinnant algorithm), no external chrono needed
    let days = (secs / 86400) as i64;
    let rem = secs % 86400;
    let (h, m, s) = (rem / 3600, (rem % 3600) / 60, rem % 60);
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z.rem_euclid(146097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if month <= 2 { y + 1 } else { y };
    format!("{year:04}-{month:02}-{day:02} {h:02}:{m:02}:{s:02} (UTC+8)")
}

fn rel_prefix(out_rel: &str) -> String {
    let depth = out_rel.split('/').filter(|s| !s.is_empty()).count();
    vec![".."; depth].join("/")
}

fn main() {
    let args = parse_args();
    let root = find_root(args.root.as_deref());
    let root = root.canonicalize().unwrap_or(root);

    let builtin_scenarios = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("scenarios");

    // --scenario-check: validate cards and exit
    if args.scenario_check {
        let game = args.game.clone().or_else(|| discover_game(&root));
        let project_scen = game
            .as_ref()
            .map(|g| root.join(g).join("_art").join("scenarios"))
            .filter(|p| p.is_dir());
        let cards = scan::load_scenarios(&root, &builtin_scenarios, project_scen.as_deref());
        let mut failures = 0;
        println!("场景卡校验（dry-run）：共 {} 张", cards.len());
        for c in &cards {
            match scan::scenario_dry_run(&root, c) {
                Ok(notes) => {
                    println!("  ✅ {} {}（{} 步 / 人拍板 {}）", c.id, c.name, c.steps, c.human_steps);
                    for n in notes {
                        println!("     ⚠️ {n}");
                    }
                }
                Err(e) => {
                    failures += 1;
                    println!("  ❌ {} {e}", c.id);
                }
            }
        }
        std::process::exit(if failures > 0 { 1 } else { 0 });
    }

    let game = match args.game.clone().or_else(|| discover_game(&root)) {
        Some(g) => g,
        None => {
            eprintln!("未能自动发现游戏（games/ 下需有含 assets/ 与 Cargo.toml 的目录），用 --game 指定");
            std::process::exit(2);
        }
    };

    let mut layout = scan::resolve_layout(&root, &game);
    if let Some(lib) = &args.library {
        let lib_path = root.join(lib);
        if lib_path.is_dir() {
            layout = scan::Layout::explicit(&root, lib);
        } else {
            eprintln!("警告：--library {lib} 不存在，回退自动布局");
        }
    }

    let result = scan::scan(&root, &game, &layout);
    let findings = checks::run(&result);

    // ---- output dir + per-model 3D data files (lazy-loaded by the viewer) ----
    let out_rel = args
        .out
        .clone()
        .unwrap_or_else(|| format!("{game}/_art/catalog"));
    let out_dir = root.join(&out_rel);
    if std::fs::create_dir_all(&out_dir).is_err() {
        eprintln!("无法创建输出目录 {}", out_dir.display());
        std::process::exit(2);
    }
    let md_dir = out_dir.join("modeldata");
    // modeldata is fully derived output — wipe stale packages each run so
    // deleted models do not linger here
    if md_dir.is_dir() {
        let _ = std::fs::remove_dir_all(&md_dir);
    }
    let _ = std::fs::create_dir_all(&md_dir);
    let mut embedded: Vec<String> = Vec::new();
    for a in &result.assets {
        if a.domain != Domain::Game || a.kind != Kind::Model || a.size > EMBED_MAX_BYTES {
            continue;
        }
        let Ok(bytes) = std::fs::read(root.join(&a.path)) else { continue };
        let js = format!(
            "window.__ART_MODELS=window.__ART_MODELS||{{}};window.__ART_MODELS[{}]={};",
            Json::s(&a.id).to_string_compact(),
            Json::s(b64(&bytes)).to_string_compact()
        );
        if std::fs::write(md_dir.join(format!("{}.js", a.id)), js).is_ok() {
            embedded.push(a.id.clone());
        }
    }
    if !embedded.is_empty() {
        println!(
            "  3D 数据：{} 个模型内嵌至 {}/modeldata/（>6MiB 跳过）",
            embedded.len(),
            out_rel
        );
    }

    // scenarios for the page
    let project_scen = root
        .join(&game)
        .join("_art")
        .join("scenarios");
    let project_scen = if project_scen.is_dir() { Some(project_scen) } else { None };
    let scenarios = scan::load_scenarios(&root, &builtin_scenarios, project_scen.as_deref());

    // ---- build catalog.json ----
    let game_models: Vec<&Asset> = result
        .assets
        .iter()
        .filter(|a| a.domain == Domain::Game && a.kind == Kind::Model)
        .collect();
    let referenced = game_models.iter().filter(|a| a.is_referenced()).count();
    let lib_candidates = result
        .assets
        .iter()
        .filter(|a| a.domain == Domain::Library && a.kind == Kind::Texture)
        .filter(|a| a.path.to_lowercase().contains("/gallery/"))
        .count();
    let lib_washed = result
        .assets
        .iter()
        .filter(|a| {
            a.domain == Domain::Library
                && a.kind == Kind::Model
                && a.path
                    .to_lowercase()
                    .contains(if layout.legacy { "/gallery-washed/" } else { "/washed/" })
        })
        .count();
    let stats = Json::obj(vec![
        ("library_candidates", Json::n(lib_candidates as f64)),
        ("library_washed", Json::n(lib_washed as f64)),
        ("game_models", Json::n(game_models.len() as f64)),
        ("referenced", Json::n(referenced as f64)),
        (
            "orphans",
            Json::n(
                result
                    .assets
                    .iter()
                    .filter(|a| a.domain == Domain::Game && !a.is_referenced() && a.kind != Kind::Other)
                    .count() as f64,
            ),
        ),
        (
            "stale",
            Json::n(result.assets.iter().filter(|a| !a.stale_reasons.is_empty()).count() as f64),
        ),
        ("findings", Json::n(findings.len() as f64)),
        (
            "intake_open",
            Json::n(
                result
                    .intake
                    .iter()
                    .filter(|t| {
                        let s = t.raw.get("status").and_then(|v| v.as_str()).unwrap_or("new");
                        s != "landed" && s != "rejected"
                    })
                    .count() as f64,
            ),
        ),
    ]);

    let catalog = Json::obj(vec![
        ("schema_version", Json::n(1.0)),
        ("generated_at", Json::s(now_string())),
        ("game", Json::s(&game)),
        (
            "library",
            layout.library_dir.clone().map(Json::s).unwrap_or(Json::Null),
        ),
        ("legacy_layout", Json::b(layout.legacy)),
        ("stats", stats.clone()),
        ("embedded", Json::Arr(embedded.iter().map(|s| Json::s(s)).collect())),
        (
            "assets",
            Json::Arr(result.assets.iter().map(|a| a.to_json()).collect()),
        ),
        ("findings", Json::Arr(findings.iter().map(|f| f.to_json()).collect())),
        (
            "intake",
            Json::Arr(
                result
                    .intake
                    .iter()
                    .map(|t| {
                        let mut o = match &t.raw {
                            Json::Obj(pairs) => Json::Obj(pairs.clone()),
                            other => other.clone(),
                        };
                        if let Json::Obj(pairs) = &mut o {
                            pairs.push(("file".into(), Json::s(&t.file)));
                        }
                        o
                    })
                    .collect(),
            ),
        ),
        (
            "scenarios",
            Json::Arr(
                scenarios
                    .iter()
                    .map(|s| {
                        Json::obj(vec![
                            ("id", Json::s(&s.id)),
                            ("name", Json::s(&s.name)),
                            ("trigger", Json::s(&s.trigger)),
                            ("steps", Json::n(s.steps as f64)),
                            ("human_steps", Json::n(s.human_steps as f64)),
                            ("file", Json::s(&s.file)),
                        ])
                    })
                    .collect(),
            ),
        ),
    ]);
    let catalog_str = catalog.to_string_compact();

    // ---- report.json ----
    let report = Json::obj(vec![
        ("schema_version", Json::n(1.0)),
        ("generated_at", Json::s(now_string())),
        ("total", Json::n(findings.len() as f64)),
        ("findings", Json::Arr(findings.iter().map(|f| f.to_json()).collect())),
    ]);
    let report_str = report.to_string_compact();

    // ---- outputs ----
    let write = |name: &str, content: &str| -> bool {
        let p = out_dir.join(name);
        match std::fs::write(&p, content) {
            Ok(_) => {
                println!("  写出 {out_rel}/{name}");
                true
            }
            Err(e) => {
                eprintln!("  写出失败 {out_rel}/{name}: {e}");
                false
            }
        }
    };

    println!("art-catalog：root={} game={} library={:?}", root.display(), game, layout.library_dir);
    let ok_json = write("catalog.json", &catalog_str);
    let ok_report = write("report.json", &report_str);
    let mut page = html::generate(
        &catalog_str,
        &rel_prefix(&out_rel),
        &now_string(),
        &game,
        layout.library_dir.as_deref().unwrap_or("（无）"),
        if layout.legacy { "迁移前布局" } else { "" },
    );
    page = page.replace("__VENDOR3D__", VENDOR3D);
    let ok_html = write("index.html", &page);
    let _ = ok_html;

    // ---- console summary ----
    println!(
        "\n概览：库候选 {lib_candidates} ｜ 库成品 {lib_washed} ｜ 游戏模型 {} ｜ 被引用 {referenced} ｜ 孤儿 {} ｜ 过时 {} ｜ 工单开 {}",
        game_models.len(),
        stats.get("orphans").and_then(|v| v.as_f64()).unwrap_or(0.0) as i64,
        stats.get("stale").and_then(|v| v.as_f64()).unwrap_or(0.0) as i64,
        stats.get("intake_open").and_then(|v| v.as_f64()).unwrap_or(0.0) as i64,
    );
    for f in &findings {
        println!("  [{}] {} {}", f.severity, f.rule, f.subject);
    }
    println!(
        "\n检查问题：{}（退出码同数）",
        findings.len(),
    );

    let exit = findings.len().min(100) as i32;
    std::process::exit(exit);
}
