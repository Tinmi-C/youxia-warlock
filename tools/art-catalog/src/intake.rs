//! `art-catalog intake …` — raw-to-gallery promotion state machine (card AC-3).
//!
//! States: new → washing → review → landed; any non-terminal state may go to
//! rejected (with a note). Terminal states (landed / rejected) cannot change —
//! re-entry requires a fresh ticket. The rules live here so neither humans nor
//! AI can hand-edit a ticket into an illegal state via the tool; direct file
//! edits are still possible but visible on the page and in rescan output.

use crate::json::Json;
use crate::scan::Layout;
use std::path::{Path, PathBuf};
use std::process::Command;

fn intake_dir(root: &Path, game: &str, layout: &Layout) -> PathBuf {
    let rel = layout
        .intake
        .clone()
        .unwrap_or_else(|| format!("{game}/_art/intake"));
    let dir = root.join(rel);
    let _ = std::fs::create_dir_all(&dir);
    dir
}

fn flag(args: &[String], key: &str) -> Option<String> {
    args.iter()
        .position(|a| a == key)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

/// Overwrite or append a key inside a parsed `Json::Obj` document.
fn set_key(doc: &mut Json, key: &str, val: Json) {
    if let Json::Obj(pairs) = doc {
        for p in pairs.iter_mut() {
            if p.0 == key {
                p.1 = val.clone();
                return;
            }
        }
        pairs.push((key.to_string(), val));
    }
}

/// Legal status transitions of the promotion pipeline.
fn legal_transition(from: &str, to: &str) -> bool {
    matches!(
        (from, to),
        ("new", "washing")
            | ("washing", "review")
            | ("review", "washing") // 重洗
            | ("review", "landed")
            | ("new", "rejected")
            | ("washing", "rejected")
            | ("review", "rejected")
    )
}

fn is_known_state(s: &str) -> bool {
    matches!(s, "new" | "washing" | "review" | "landed" | "rejected")
}

/// Strip the `\\?\` verbatim prefix that std::fs::canonicalize leaks on Windows.
fn display_path(p: &Path) -> String {
    p.to_string_lossy().replace("\\\\?\\", "")
}

/// Drop global flag/value pairs so positional parsing in `set` cannot mistake
/// `--game games/...` for a ticket id.
fn strip_global_flags(args: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    let mut skip_value = false;
    for a in args {
        if skip_value {
            skip_value = false;
            continue;
        }
        match a.as_str() {
            "--root" | "--game" | "--library" | "--out" => skip_value = true,
            _ => out.push(a.clone()),
        }
    }
    out
}

pub fn run(root: &Path, game: &str, layout: &Layout, args: &[String], date: &str) -> i32 {
    let dir = intake_dir(root, game, layout);
    // The subcommand may appear after global flags (`intake --game X create …`)
    let sub = args
        .iter()
        .map(|s| s.as_str())
        .find(|a| matches!(*a, "create" | "set" | "list" | "wash"));
    let Some(sub) = sub else {
        eprintln!("用法: art-catalog intake create|set|list|wash …");
        return 2;
    };
    // hand the handlers their args without the subcommand token itself — and
    // without the family head word (`intake`) that main.rs passes through as
    // argv[0]; otherwise it leaks in as the first positional (e.g. ticket id)
    let mut rest: Vec<String> = args.to_vec();
    if rest.first().map(|s| s.as_str()) == Some("intake") {
        rest.remove(0);
    }
    rest.retain(|a| a.as_str() != sub);
    match sub {
        "create" => create(root, game, &dir, &rest, date),
        "set" => set(&dir, &rest, date),
        "list" => list(&dir),
        "wash" => wash(root, game, layout, &rest, date),
        _ => unreachable!(),
    }
}

fn create(root: &Path, game: &str, dir: &Path, args: &[String], date: &str) -> i32 {
    let file = match flag(args, "--file") {
        Some(f) => f,
        None => {
            eprintln!("缺少 --file <raw 路径（仓库相对）>");
            return 2;
        }
    };
    let raw_path = root.join(&file);
    if !raw_path.is_file() {
        eprintln!("raw 文件不存在：{file}");
        return 2;
    }
    // target name: --name wins, else the raw file stem; must be snake_case (R3)
    let stem = Path::new(&file)
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    let name = flag(args, "--name").unwrap_or(stem);
    if !crate::checks::is_snake_case(&name) {
        eprintln!("目标名 {name} 不符合 snake_case（R3）。用 --name 指定合法名，不要洗完再改名。");
        return 2;
    }
    let source = flag(args, "--source").unwrap_or_default();
    let license = flag(args, "--license").unwrap_or_default();
    if license.is_empty() {
        eprintln!("缺少 --license（许可留档是人工关卡，不可省略）");
        return 2;
    }
    let requester = flag(args, "--requester").unwrap_or_else(|| "youxia".into());
    let scenario = flag(args, "--scenario").unwrap_or_else(|| "SC1".into());
    let height = flag(args, "--height").unwrap_or_default();
    // repo-relative, matching Asset.path in the scan (game-prefixed)
    let target = format!("{game}/assets/models/{name}.glb");
    let mut notes: Vec<Json> = Vec::new();
    if root.join(&target).is_file() {
        notes.push(Json::s(format!(
            "{date} 覆盖冲突：{target} 已存在，洗白写入前需人显式确认"
        )));
    }
    let id = format!("{date}-{name}");
    let mut pairs = vec![
        ("id", Json::s(&id)),
        ("date", Json::s(date)),
        ("requester", Json::s(&requester)),
        ("source", Json::s(&source)),
        ("license", Json::s(&license)),
        ("target", Json::s(&target)),
        ("scenario", Json::s(&scenario)),
        ("status", Json::s("new")),
        ("raw_file", Json::s(&file)),
    ];
    // approved wash parameters stay on the ticket (audit trail)
    if !height.is_empty() {
        match height.parse::<f64>() {
            Ok(h) => pairs.push(("height_m", Json::n(h))),
            Err(_) => {
                eprintln!("--height 不是数字：{height}");
                return 2;
            }
        }
    }
    pairs.push(("notes", Json::Arr(notes)));
    let doc = Json::obj(pairs);
    let path = dir.join(format!("{id}.json"));
    match std::fs::write(&path, doc.to_string_compact()) {
        Ok(_) => {
            println!("工单已立案：{}", display_path(&path));
            println!("  status=new target={target}");
            println!("下一步：人批准参数（身高/三角预算/覆盖）→ intake set {id} --status washing");
            0
        }
        Err(e) => {
            eprintln!("工单写入失败：{e}");
            1
        }
    }
}

fn set(dir: &Path, args: &[String], date: &str) -> i32 {
    // positional id + flags; global flag/value pairs already stripped
    let args = &strip_global_flags(args);
    let mut id: Option<String> = None;
    let mut status: Option<String> = None;
    let mut note: Option<String> = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--status" => {
                status = args.get(i + 1).cloned();
                i += 2;
            }
            "--note" => {
                note = args.get(i + 1).cloned();
                i += 2;
            }
            a => {
                if id.is_none() {
                    id = Some(a.to_string());
                }
                i += 1;
            }
        }
    }
    let Some(id) = id else {
        eprintln!("用法: art-catalog intake set <工单id> --status <new后的状态> [--note 备注]");
        return 2;
    };
    let Some(status) = status else {
        eprintln!("缺少 --status");
        return 2;
    };
    if !is_known_state(&status) {
        eprintln!("未知状态 {status}（合法：new/washing/review/landed/rejected）");
        return 2;
    }
    let fname = if id.ends_with(".json") { id.clone() } else { format!("{id}.json") };
    let path = dir.join(&fname);
    let Ok(text) = std::fs::read_to_string(&path) else {
        eprintln!("找不到工单：{}", display_path(&path));
        return 2;
    };
    let mut doc = match crate::json::parse(&text) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("工单 JSON 解析失败：{e}");
            return 2;
        }
    };
    let old = doc
        .get("status")
        .and_then(|v| v.as_str())
        .unwrap_or("new")
        .to_string();
    if old == status {
        println!("状态无变化（{old}）");
        return 0;
    }
    if !legal_transition(&old, &status) {
        eprintln!("非法状态翻转：{old} → {status}（规则见 tools/art-catalog/src/intake.rs）");
        return 1;
    }
    set_key(&mut doc, "status", Json::s(&status));
    if let Json::Obj(pairs) = &mut doc {
        for p in pairs.iter_mut() {
            if p.0 == "notes" {
                if let Json::Arr(n) = &mut p.1 {
                    let mut line = format!("{date} {old}→{status}");
                    if let Some(extra) = &note {
                        line.push_str(&format!("：{extra}"));
                    }
                    n.push(Json::s(line));
                }
            }
        }
    }
    match std::fs::write(&path, doc.to_string_compact()) {
        Ok(_) => {
            println!("工单 {id}：{old} → {status}");
            match status.as_str() {
                "washing" => println!("洗白完成后：intake set {id} --status review"),
                "review" => println!("待人工翻图册拍板；通过 → intake set {id} --status landed"),
                "landed" => println!("已上架；复扫后图册画廊自动收录"),
                "rejected" => println!("已拒绝；raw 原件保留在库"),
                _ => {}
            }
            0
        }
        Err(e) => {
            eprintln!("工单写回失败：{e}");
            1
        }
    }
}

/// One-command wash (card AC-4, automation level L2): ticket + normalize +
/// turntable + anim strips + review + rescan, then STOP and wait for the
/// human gallery verdict (landing is never automatic). Any failure marks the
/// ticket rejected with the reason.
fn wash(root: &Path, game: &str, layout: &Layout, args: &[String], date: &str) -> i32 {
    // 1) resolve Blender: --blender > env BLENDER_EXE > team default path
    let blender = flag(args, "--blender")
        .or_else(|| std::env::var("BLENDER_EXE").ok())
        .unwrap_or_else(|| "D:\\Blender\\blender.exe".to_string());
    if !Path::new(&blender).is_file() {
        eprintln!("找不到 Blender：{blender}（用 --blender 或环境变量 BLENDER_EXE 指定）");
        return 2;
    }
    // 2) human-gated parameters, required up front
    let file = match flag(args, "--file") {
        Some(f) => f,
        None => {
            eprintln!("缺少 --file <raw 路径>");
            return 2;
        }
    };
    let height = match flag(args, "--height") {
        Some(h) => h,
        None => {
            eprintln!("缺少 --height <米>（参数批准是人工关卡）");
            return 2;
        }
    };
    if height.parse::<f64>().is_err() {
        eprintln!("--height 不是数字：{height}");
        return 2;
    }
    let license = match flag(args, "--license") {
        Some(l) => l,
        None => {
            eprintln!("缺少 --license <许可>（许可留档是人工关卡）");
            return 2;
        }
    };
    let stem = Path::new(&file)
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    let name = flag(args, "--name").unwrap_or(stem);
    if !crate::checks::is_snake_case(&name) {
        eprintln!("目标名 {name} 不符合 snake_case（R3）。用 --name 指定合法名。");
        return 2;
    }
    let yes = args.iter().any(|a| a == "--yes");
    let skip_anim = args.iter().any(|a| a == "--skip-anim");
    let scenario = flag(args, "--scenario").unwrap_or_else(|| "SC1".into());
    let target = format!("{game}/assets/models/{name}.glb");
    if root.join(&target).is_file() && !yes {
        eprintln!("覆盖冲突：{target} 已存在。确认覆盖请加 --yes（人工关卡）");
        return 2;
    }
    let dir = intake_dir(root, game, layout);
    let id = format!("{date}-{name}");
    let ticket = dir.join(format!("{id}.json"));
    if ticket.is_file() {
        eprintln!("同名工单已存在：{}（先处理或删除它）", display_path(&ticket));
        return 2;
    }
    // 3) ticket via the same create() rules as manual intake
    let mut cargs = vec![
        "--file".to_string(),
        file.clone(),
        "--name".to_string(),
        name.clone(),
        "--license".to_string(),
        license.clone(),
        "--height".to_string(),
        height.clone(),
        "--scenario".to_string(),
        scenario,
    ];
    if let Some(s) = flag(args, "--source") {
        cargs.push("--source".to_string());
        cargs.push(s);
    }
    if create(root, game, &dir, &cargs, date) != 0 {
        return 2;
    }
    // reject-and-exit helper for any pipeline failure
    macro_rules! bail {
        ($note:expr) => {{
            let _ = set(
                &dir,
                &[
                    id.clone(),
                    "--status".to_string(),
                    "rejected".to_string(),
                    "--note".to_string(),
                    ($note).to_string(),
                ],
                date,
            );
            return 1;
        }};
    }
    if set(
        &dir,
        &[
            id.clone(),
            "--status".to_string(),
            "washing".to_string(),
            "--note".to_string(),
            format!("wash：人已批参数（身高 {height} 米）"),
        ],
        date,
    ) != 0
    {
        bail!("washing 翻转失败");
    }
    // 4) normalize — absolute paths only (Blender resolves CWD unpredictably)
    let abs_in = root.join(&file);
    let abs_out = root.join(&target);
    let norm_script = root.join("tools/art/normalize.py");
    let mut cmd = Command::new(&blender);
    cmd.args(["-b", "-P"])
        .arg(&norm_script)
        .arg("--")
        .arg("--in")
        .arg(&abs_in)
        .arg("--out")
        .arg(&abs_out)
        .arg("--height")
        .arg(&height);
    if let Some(mt) = flag(args, "--max-tris") {
        cmd.arg("--max-tris").arg(&mt);
    }
    if let Some(ts) = flag(args, "--tex-size") {
        cmd.arg("--tex-size").arg(&ts);
    }
    println!("[wash] normalize …");
    match cmd.output() {
        Ok(out) => {
            let log = format!(
                "{}{}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            );
            if !out.status.success() || !log.contains("leaked=[]") {
                // print the tail so the failure cause is visible without rerunning
                let tail: String = log
                    .lines()
                    .filter(|l| {
                        !l.trim().is_empty() && !l.contains("Blender quit") && !l.contains("Read blend")
                    })
                    .rev()
                    .take(12)
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .collect::<Vec<_>>()
                    .join("\n");
                eprintln!("[wash] normalize 自检未过（exit={:?}），输出尾部：\n{tail}", out.status.code());
                bail!("normalize 自检未过");
            }
        }
        Err(e) => bail!(format!("无法启动 Blender：{e}")),
    }
    // 5) turntable into the washed shelf parent (produces <out>/<name>/)
    let Some(washed_rel) = layout.washed.clone() else {
        bail!("布局缺少 washed 图册目录");
    };
    let washed_abs = root.join(&washed_rel);
    println!("[wash] turntable …");
    let tt = Command::new(&blender)
        .args(["-b", "-P"])
        .arg(root.join("tools/art/turntable.py"))
        .arg("--")
        .arg("--in")
        .arg(&abs_out)
        .arg("--out")
        .arg(&washed_abs)
        .output();
    match tt {
        Ok(out) if out.status.success() => {}
        Ok(out) => {
            eprintln!(
                "[wash] turntable 失败：{}",
                String::from_utf8_lossy(&out.stderr)
            );
            bail!("turntable 失败");
        }
        Err(e) => bail!(format!("无法启动 Blender（turntable）：{e}")),
    }
    let model_dir_abs = washed_abs.join(&name);
    let meta_path = model_dir_abs.join("meta.json");
    if !meta_path.is_file() {
        eprintln!("[wash] 未找到 {}", display_path(&meta_path));
        bail!("turntable 未产出 meta.json");
    }
    // 6) animation strips only when the file really carries clips
    let mut clips = 0usize;
    if let Ok(text) = std::fs::read_to_string(&meta_path) {
        if let Ok(doc) = crate::json::parse(&text) {
            clips = doc
                .get("animation_clips")
                .and_then(|v| v.as_arr())
                .map(|a| a.len())
                .unwrap_or(0);
        }
    }
    if clips > 0 && !skip_anim {
        println!("[wash] anim_strip（{clips} 条 clip）…");
        let as_out = Command::new(&blender)
            .args(["-b", "-P"])
            .arg(root.join("tools/art/anim_strip.py"))
            .arg("--")
            .arg("--model")
            .arg(&abs_out)
            .arg("--meta-dir")
            .arg(&model_dir_abs)
            .output();
        let ok = matches!(&as_out, Ok(o) if o.status.success())
            && model_dir_abs.join("anim").join("anim_index.json").is_file();
        if !ok {
            eprintln!("[wash] anim_strip 失败");
            bail!("anim_strip 失败");
        }
    }
    // 7) review + rescan; landing stays with the human
    let note = format!(
        "wash：零泄漏/4视角/{}条带",
        if skip_anim { 0 } else { clips }
    );
    if set(
        &dir,
        &[
            id.clone(),
            "--status".to_string(),
            "review".to_string(),
            "--note".to_string(),
            note,
        ],
        date,
    ) != 0
    {
        return 1;
    }
    let exe = std::env::current_exe().unwrap_or_default();
    if let Ok(r) = Command::new(&exe).arg("--root").arg(root).arg("--game").arg(game).output() {
        print!("{}", String::from_utf8_lossy(&r.stdout));
    }
    println!("=== wash 完成：工单 {id} 已到 review，等你翻图册拍板 ===");
    println!("  模型: {}", display_path(&abs_out));
    println!("  图册: {}", display_path(&model_dir_abs));
    println!("  拍板过 → art-catalog intake --game {game} set {id} --status landed");
    println!("  不要 → 删上面两个产物和工单文件后复扫");
    0
}

fn list(dir: &Path) -> i32 {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|rd| rd.filter_map(|e| e.ok()).map(|e| e.path()).collect())
        .unwrap_or_default();
    files.retain(|p| p.extension().and_then(|e| e.to_str()) == Some("json"));
    files.sort();
    if files.is_empty() {
        println!("（无工单）");
        return 0;
    }
    for p in files {
        let text = std::fs::read_to_string(&p).unwrap_or_default();
        let doc = crate::json::parse(&text).ok();
        let get = |k: &str| {
            doc.as_ref()
                .and_then(|d| d.get(k))
                .and_then(|v| v.as_str())
                .unwrap_or("?")
        };
        println!("{}  status={}  target={}", get("id"), get("status"), get("target"));
    }
    0
}
