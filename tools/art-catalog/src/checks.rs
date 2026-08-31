//! M2 check engine: rules R1-R7 over the scan result (card AC-1).

use crate::model::*;
use crate::scan::ScanResult;

const SNAKE_RE: &str = "^[a-z0-9]+(_[a-z0-9]+)*$";
const BIG_FILE_BYTES: u64 = 8 * 1024 * 1024; // 8 MiB

pub fn is_snake_case(name: &str) -> bool {
    // stem only, e.g. "green_blob" from "green_blob.glb"
    let stem = name.rsplit('.').nth(1).unwrap_or(name);
    let mut ok = !stem.is_empty();
    let mut prev_underscore = true;
    for c in stem.chars() {
        match c {
            'a'..='z' | '0'..='9' => {
                prev_underscore = false;
            }
            '_' => {
                if prev_underscore {
                    ok = false;
                    break;
                }
                prev_underscore = true;
            }
            _ => {
                ok = false;
                break;
            }
        }
    }
    ok && !prev_underscore
}

fn stem_of(file_name: &str) -> &str {
    // strip final extension only
    match file_name.rfind('.') {
        Some(i) => &file_name[..i],
        None => file_name,
    }
}

pub fn run(scan: &ScanResult) -> Vec<Finding> {
    let mut f = Vec::new();

    // Allowed clip names per style-bible §8 (superset reported, non-blocking)
    let allowed_clips = ["idle", "walk", "run", "attack", "hit", "death"];

    // Library candidate dirs (gallery) for R5 — only renders inside the gallery
    // section count, so raw working dirs cannot masquerade as candidates.
    let gallery_dir = scan.gallery_dir.clone().unwrap_or_default();
    let mut candidate_dirs: Vec<String> = Vec::new();
    for a in &scan.assets {
        if a.domain != Domain::Library || a.kind != Kind::Texture {
            continue;
        }
        if gallery_dir.is_empty() || !a.path.starts_with(&format!("{gallery_dir}/")) {
            continue;
        }
        // path like _library/gallery/<Name>/<Name>_0_45deg.png
        let parts: Vec<&str> = a.path.split('/').collect();
        if parts.len() >= 3 {
            let dir_name = parts[parts.len() - 2].to_string();
            if !candidate_dirs.contains(&dir_name) {
                candidate_dirs.push(dir_name);
            }
        }
    }

    let runtime_models: Vec<&Asset> = scan
        .assets
        .iter()
        .filter(|a| a.domain == Domain::Game && a.kind == Kind::Model)
        .collect();

    let adopted_stems: Vec<String> = runtime_models.iter().map(|a| a.id.clone()).collect();

    for a in &scan.assets {
        // R1 orphan: game-domain asset not referenced anywhere in src/tests
        if a.domain == Domain::Game && !a.is_referenced() && a.kind != Kind::Other {
            f.push(Finding {
                rule: "R1",
                severity: "warning",
                subject: a.path.clone(),
                evidence: format!(
                    "`{}` 在 `src/` 与 `tests/` 的字面量引用扫描中零命中（文件名 {}）",
                    a.path,
                    a.file_name()
                ),
                fix_hint: "确认后退役：清引用 → 移回库或归档；或补上代码引用".into(),
            });
        }

        // R2 stale
        for reason in &a.stale_reasons {
            f.push(Finding {
                rule: "R2",
                severity: "warning",
                subject: a.path.clone(),
                evidence: reason.clone(),
                fix_hint: "重跑 turntable.py 更新图册与 meta，或重洗模型".into(),
            });
        }

        // R3 naming: runtime models must be snake_case
        if a.domain == Domain::Game && a.kind == Kind::Model && !is_snake_case(&a.file_name()) {
            f.push(Finding {
                rule: "R3",
                severity: "error",
                subject: a.path.clone(),
                evidence: format!("运行时模型文件名 `{}` 不符合 snake_case（规则基线 {SNAKE_RE}）", a.file_name()),
                fix_hint: "git mv 改名为 snake_case 并同步代码引用".into(),
            });
        }

        // R4 clip convention: superset reported, non-blocking
        if let Some(meta) = &a.meta {
            let bad: Vec<&String> = meta
                .clips
                .iter()
                .filter(|c| {
                    let lc = c.to_lowercase();
                    !allowed_clips.iter().any(|k| lc.contains(k))
                })
                .collect();
            if !bad.is_empty() {
                let names: Vec<String> = bad.iter().map(|s| s.to_string()).collect();
                f.push(Finding {
                    rule: "R4",
                    severity: "info",
                    subject: a.path.clone(),
                    evidence: format!("clip 名不在 style-bible 约定集（idle/walk/run/attack/hit/death）：{}", names.join(", ")),
                    fix_hint: "洗白阶段 normalize.py CLIP_MAP 改名，或在 style-bible 扩充约定集".into(),
                });
            }
        }

        // R7 big files
        if a.size > BIG_FILE_BYTES {
            f.push(Finding {
                rule: "R7",
                severity: "info",
                subject: a.path.clone(),
                evidence: format!("文件 {} MiB，超过 8 MiB 预警线", a.size / 1024 / 1024),
                fix_hint: "确认入库策略：raw 素材可 gitignore；运行时资产考虑减面/贴图降采样".into(),
            });
        }
    }

    // R5 candidate never adopted (library gallery dirs with no runtime counterpart)
    for dir in &candidate_dirs {
        let stem = stem_of(dir);
        if !adopted_stems.iter().any(|s| s.eq_ignore_ascii_case(stem)) {
            f.push(Finding {
                rule: "R5",
                severity: "info",
                subject: dir.clone(),
                evidence: format!("库域候选图册 `{dir}` 从未上架：assets/models/ 无同名运行时模型"),
                fix_hint: "评审拍板：上架（adopt）或标记淘汰，勿长期挂起".into(),
            });
        }
    }

    // R6 meta missing: runtime model without matched meta
    for a in &runtime_models {
        if a.meta.is_none() {
            f.push(Finding {
                rule: "R6",
                severity: "warning",
                subject: a.path.clone(),
                evidence: "运行时模型没有匹配到 meta.json（washed/gallery 目录无同名目录）".into(),
                fix_hint: "对该模型重跑 turntable.py 生成图册与 meta.json".into(),
            });
        }
    }

    // R8 animation index↔name mismatch (card AC-5 hardening): a code symbol
    // says clip X lives at index N, but the glb's own animations[N] has a
    // different name — the exact bug class behind "跑步用的是 walk".
    for a in &runtime_models {
        let Some(g) = &a.glb else { continue };
        for r in &a.anim_refs {
            let expected = clip_token(&r.symbol);
            if expected.is_empty() {
                continue;
            }
            match g.animations.get(r.clip_index) {
                None => f.push(Finding {
                    rule: "R8",
                    severity: "error",
                    subject: a.path.clone(),
                    evidence: format!(
                        "{} 引用索引 {}，但文件只有 {} 条动画（索引超界）",
                        r.symbol,
                        r.clip_index,
                        g.animations.len()
                    ),
                    fix_hint: "改代码索引到有效范围，或重排导出顺序；用页面「引用」标签核对".into(),
                }),
                Some(name) => {
                    let name = name.trim();
                    if name.is_empty() {
                        continue; // anonymous clip in file: cannot verify by name
                    }
                    if !name.eq_ignore_ascii_case(&expected)
                        && !name.to_lowercase().contains(&expected)
                        && !expected.contains(&name.to_lowercase())
                    {
                        let dur = g.durations.get(r.clip_index).copied().unwrap_or(0.0);
                        f.push(Finding {
                            rule: "R8",
                            severity: "error",
                            subject: a.path.clone(),
                            evidence: format!(
                                "{} 引用索引 {}，但文件该位置是「{}」（{:.2}s）——名字对不上",
                                r.symbol, r.clip_index, name, dur
                            ),
                            fix_hint: format!(
                                "确认符号期望的 clip 与索引：要么改 {} 的数值，要么重排 glb 动画顺序（用页面「引用」标签核对）",
                                r.symbol
                            ),
                        });
                    }
                }
            }
        }
    }

    f
}

/// Expected clip-name token from a code symbol:
/// `HERO_CLIP_RUN` → "run", `walk_clip` → "walk", `attack_clip` → "attack".
fn clip_token(symbol: &str) -> String {
    let s = symbol.to_ascii_lowercase();
    let s = s.strip_suffix("_clip").unwrap_or(&s);
    s.rsplit('_').next().unwrap_or(s).to_string()
}
