#!/usr/bin/env python3
"""art_needs — turn MapBundle unit defs into vault (ArtVault) art demands.

Game side of the demand board (ADR-0015). The map is the single content
source: every unit def in games/<g>/maps/*.ron is a potential art demand.
This tool scans defs via warbase's mapbundle-dump (serde-validated JSON,
never text guessing), diffs them into games/<g>/docs/art-needs.json, and
syncs demand state with the vault's /demands API (its ADR-0006).

Decoupling red line (ADR-0015): the vault being down never blocks builds.
`scan` is fully local; `submit`/`status` report and exit non-zero instead.

Usage (from repo root):
    python3 tools/art-needs/art_needs.py games/loop-td scan           # diff report
    python3 tools/art-needs/art_needs.py games/loop-td scan --write   # update file
    python3 tools/art-needs/art_needs.py games/loop-td scan --ui --audio --write
    python3 tools/art-needs/art_needs.py games/loop-td scan --strict  # full-field audit
    python3 tools/art-needs/art_needs.py games/loop-td submit         # drafts -> vault
    python3 tools/art-needs/art_needs.py games/loop-td status         # state projection
    python3 tools/art-needs/art_needs.py games/loop-td sync           # report locks

Contract: vault docs/design/demand-board-contract.md v1 (co-signed by ADR-0015) —
zero-validation status via PATCH, locks are DemandLock records (not a state).

Exit code = number of problems (0 = clean), same convention as art-sync.
Zero dependencies (Python 3 stdlib only). Human and AI run the same command.
"""
from __future__ import annotations

import argparse
import ipaddress
import json
import re
import socket
import subprocess
import sys
import urllib.error
import urllib.request
from pathlib import Path
from urllib.parse import urlparse

NEEDS_NAME = "docs/art-needs.json"
UI_MANIFEST_NAME = "docs/art-ui.json"
MAPPING_NAME = "art-mapping.json"
# SfxKind projection source (relative to repo root) — parsed at scan time,
# never hardcoded, so names/count cannot drift from the engine enum.
SFX_MOD_REL = "engine/warbase/src/sfx/mod.rs"

# Spec templates keyed by dump kind; humans refine them in the json afterwards.
# Field shape follows the ai-asset-pipeline spec section (8-station flow).
_TOWER_SPEC = {
    "base": "底模一套，升阶为同底模变体（体积/件数/特效递进，不换骨架）",
    "poly_budget": "≤ 12k tris/阶",
    "texture": "2048 albedo",
    "animations": ["idle", "attack"],
}
_CREEP_SPEC = {
    "base": "单模型",
    "poly_budget": "≤ 8k tris",
    "texture": "1024 albedo",
    "animations": ["idle", "walk", "death"],
}
_HERO_SPEC = {
    "base": "单模型（成长塔主角，辨识度优先）",
    "poly_budget": "≤ 15k tris",
    "texture": "2048 albedo",
    "animations": ["idle", "attack", "victory"],
}

# Appearance-relevance filter for `changed` detection (round 4 NEEDS): only
# these def fields, when they drift between scans, mark a demand changed —
# they change what the artist must deliver. Numeric gameplay tuning is
# SILENT (hp / move_speed / damage / interval / range / bounty / cost /
# hp_scale / leak / wave_refs / …): it must not spam the demand board with
# changed transitions. Field names are the mapbundle-dump projection
# (bundle dump_units_json); the Rust source path each maps to is noted.
# `skills` is kept for forward-compat — the dump does not project it yet;
# the day it does, it joins the whitelist automatically.
_APPEARANCE_FIELDS = (
    "id",           # def identity (also covered by source_defs drift)
    "hero",         # def.hero — hero silhouette / quality tier
    "armor_type",   # def.armor_type — visible armor material cues
    "traits",       # def.traits — trait set may add visual features
    "has_aura",     # def.auras (dumped as has_aura) — aura VFX demand
    "attack_type",  # def.attack.attack_type — attack anim / muzzle semantics
    "body_radius",  # def.attrs.body_radius — visual volume target
    "skills",       # (not projected by the dump yet — forward-compat)
)

# Chinese usage semantics per SfxKind variant (sfx/mod.rs is the channel
# contract; these lines are the human-facing demand text). Sync choice: the
# VARIANT LIST is parsed from source at scan time (zero drift), but Chinese
# semantics need a human — an unmapped variant falls back to a placeholder
# usage + a scan problem, and the unit test asserts the table stays in
# sync with the real enum (anti-drift guard both ways).
_SFX_USAGE = {
    "ClickAck": "点击确认（命令域 Ack 契约音）",
    "Select": "选中单位",
    "OrderAck": "下令确认（移动/攻击指令 Ack）",
    "Error": "指令拒绝/资源不足错误提示",
    "Cast": "施法/技能释放",
    "Hit": "战斗命中（同类 0.1s 节流）",
    "Death": "单位死亡",
    "WaveStart": "波次开始",
    "WaveCleared": "波次全部清空",
    "BuildComplete": "建造完成",
    "ResearchComplete": "研究完成",
    "LevelUp": "升级（英雄/塔升阶）",
    "MatchEnd": "对局结束胜负 sting（win/lose）",
}


# ---------- pure: grouping & diff (unit-tested) ----------

def group_units(units: list[dict]) -> list[dict]:
    """Group unit defs into demand candidates.

    Towers follow their upgrade_to chain (one demand per chain: shared base
    model, tiers as variants). Everything else (aura towers, hero, creeps)
    is one demand per def. Returns groups sorted by local_key.
    """
    by_id = {u["id"]: u for u in units}
    pointed_at = {u["upgrade_to"] for u in units if u.get("upgrade_to")}

    groups: list[dict] = []
    consumed: set[str] = set()

    def walk_chain(start: dict) -> None:
        chain = [start]
        seen = {start["id"]}
        cur = start
        while cur.get("upgrade_to") and cur["upgrade_to"] in by_id:
            nxt = by_id[cur["upgrade_to"]]
            if nxt["id"] in seen:  # cycle guard — loader wouldn't allow it anyway
                break
            chain.append(nxt)
            seen.add(nxt["id"])
            cur = nxt
        consumed.update(seen)
        kind = chain[0]["kind"]
        groups.append({
            "local_key": f"{kind}-{start['id']}",
            "kind": kind,
            "source_defs": [c["id"] for c in chain],
            "meta": {
                "tiers": len(chain),
                "has_aura": any(c.get("has_aura") for c in chain),
                "wave_refs": sum(c.get("wave_refs", 0) for c in chain),
                "attack_type": next(
                    (c["attack_type"] for c in chain if c.get("attack_type")), None),
            },
        })

    # Pass 1: chain heads (defs nobody upgrades into). Pass 2: leftovers
    # (cycle-malformed data the loader would have rejected) each become
    # their own single-def group instead of being silently dropped.
    heads = [u for u in sorted(units, key=lambda x: x["id"])
             if u["id"] not in pointed_at]
    for u in heads:
        if u["id"] not in consumed:
            walk_chain(u)
    for u in sorted(units, key=lambda x: x["id"]):
        if u["id"] not in consumed:
            walk_chain(u)
    return sorted(groups, key=lambda g: g["local_key"])


def default_spec(group: dict) -> dict:
    kind = group["kind"]
    spec = dict(_HERO_SPEC if kind == "hero" else
                _TOWER_SPEC if kind == "tower" else _CREEP_SPEC)
    if kind == "tower" and group["meta"]["tiers"] > 1:
        spec["variants"] = [f"{d} (tier {i + 1})"
                            for i, d in enumerate(group["source_defs"])]
    if group["meta"]["wave_refs"] >= 50:  # heavy-rotation creep: loosen budget
        spec["poly_budget"] = "≤ 5k tris（高频出场，压预算）"
    return spec


def default_acceptance(group: dict) -> str:
    spec = default_spec(group)
    anims = "、".join(spec["animations"])
    return (f"三角面 {spec['poly_budget']}、动画 {anims} 齐全、"
            f"showroom turntable 过审")


def diff_demands(existing: list[dict], groups: list[dict],
                 snapshots: dict | None = None,
                 scanned_units: list[dict] | None = None,
                 strict: bool = False) -> dict:
    """Diff a needs file's entries against freshly scanned groups.

    Returns {'new': [groups], 'removed': [local_keys], 'changed': [local_keys],
    'silent': [local_keys]}.
    'changed' = source_defs drift on an entry, OR appearance-relevant field
    drift vs the stored baseline (`snapshots` = the file's `unit_snapshots`)
    — if it is already submitted, the vault side wants a `changed`
    transition there (its ADR-0006). 'silent' = baseline drift confined to
    non-appearance fields (numeric tuning): reported for visibility, never
    counted as changed. `strict=True` turns the appearance filter off
    (manual full-field audit mode, `scan --strict`).
    """
    by_key = {d["local_key"]: d for d in existing if not d.get("removed")}
    defs_to_key: dict[str, str] = {}
    for key, d in by_key.items():
        for s in d.get("source_defs", []):
            defs_to_key[s] = key
    # Removed entries still occupy their defs: a human decided these defs
    # need no demand (or they vanished from the map) — never let scan
    # resurrect them as "new" while the entry stays in the file.
    for d in existing:
        if d.get("removed"):
            for s in d.get("source_defs", []):
                defs_to_key.setdefault(s, d["local_key"])
    scanned_defs = {s for g in groups for s in g["source_defs"]}
    scanned_by_key = {g["local_key"]: set(g["source_defs"]) for g in groups}

    new = [g for g in groups if g["local_key"] not in by_key
           and not (set(g["source_defs"]) & set(defs_to_key))]
    # Only entries with source_defs can "disappear" — manual entries (terrain,
    # concept art, ui, audio, …) own no defs and are removed by humans,
    # never by scan.
    removed = [k for k, d in by_key.items()
               if d.get("source_defs")
               and not (set(d["source_defs"]) & scanned_defs)]
    changed: list[str] = []
    silent: list[str] = []
    scanned_fields = (unit_snapshots(scanned_units, groups)
                      if scanned_units is not None else {})
    for k, d in by_key.items():
        if k not in scanned_by_key:
            continue
        if set(d.get("source_defs", [])) != scanned_by_key[k]:
            changed.append(k)  # def set drift — id is appearance by definition
            continue
        base = (snapshots or {}).get(k)
        if base is None:
            # No stored baseline (entry predates the filter): def-set drift
            # only — the write path backfills a baseline for next time.
            continue
        drift = [i for i, cur in scanned_fields.get(k, {}).items()
                 if appearance_differs(base.get(i), cur, strict)]
        if drift:
            changed.append(k)
        elif base != scanned_fields.get(k, {}):
            silent.append(k)  # drifted, but nothing an artist must re-do
    return {"new": new, "removed": removed, "changed": changed, "silent": silent}


def _appearance_projection(unit: dict) -> dict:
    """Project a dump unit onto the appearance whitelist fields."""
    return {f: unit.get(f) for f in _APPEARANCE_FIELDS if f in unit}


def appearance_differs(old: dict | None, new: dict | None,
                       strict: bool = False) -> bool:
    """Field-level drift test between two unit dump snapshots.

    Default: only _APPEARANCE_FIELDS count. strict=True compares every dump
    field (audit mode). None (def missing on one side) always differs.
    """
    if strict:
        return (old or {}) != (new or {})
    return _appearance_projection(old or {}) != _appearance_projection(new or {})


def unit_snapshots(units: list[dict], groups: list[dict]) -> dict:
    """Full per-def dump snapshots keyed by group local_key — the diff
    baseline stored in the needs file's top-level `unit_snapshots` (top
    level on purpose: the human-authored demand entries stay untouched).
    Snapshots store ALL dump fields so `--strict` audits can compare
    against the same baseline."""
    by_id = {u["id"]: u for u in units}
    return {g["local_key"]: {d: by_id[d] for d in g["source_defs"]
                             if d in by_id}
            for g in groups}


# ---------- UI 2D declaration manifest (art-ui.json -> manual cards) ------

def load_ui_manifest(game_dir: Path) -> list[dict]:
    """Load + validate <game>/docs/art-ui.json — the UI 2D declaration
    manifest (panel bases & icon slots; spec authority:
    docs/topics/art/asset-taxonomy-spec.md §1 + ui-asset-spec.md).

    Item schema: {id, kind: panel|icon, size_px, usage, acceptance} —
    size_px is the SOURCE texture size (icons one int, panels [w, h]);
    in-UI consumption sizes are noted inside `usage` (they follow
    UiTheme spacing, e.g. icon_size). Raises ValueError on bad items so
    scan reports the manifest instead of writing garbage demands.
    """
    path = game_dir / UI_MANIFEST_NAME
    manifest = json.loads(path.read_text(encoding="utf-8"))
    items = manifest.get("items", [])
    seen: set[str] = set()
    for it in items:
        missing = [f for f in ("id", "kind", "size_px", "usage", "acceptance")
                   if f not in it]
        if missing:
            raise ValueError(f"art-ui.json 条目缺字段 {missing}: {it!r}")
        if it["kind"] not in ("panel", "icon"):
            raise ValueError(
                f"art-ui.json 条目 kind 必须是 panel|icon: {it['id']!r}")
        if it["id"] in seen:
            raise ValueError(f"art-ui.json 条目 id 重复: {it['id']!r}")
        seen.add(it["id"])
    return items


def ui_demand_cards(items: list[dict]) -> list[dict]:
    """Turn manifest items into manual-lane demand cards.

    origin='manual_ui' + empty source_defs = the never-deleted manual lane:
    diff only ever touches entries that own source_defs. Cards are created
    once; afterwards the entry is human-owned — refine it in art-needs.json
    directly, or delete it there (and in art-ui.json, or the next
    `scan --write --ui` re-creates it: the manifest is the declaration
    source, the projection follows it).
    """
    cards = []
    for it in items:
        spec = {
            "kind": it["kind"],
            "size_px": it["size_px"],
            "format": "PNG 透明通道",
        }
        if it["kind"] == "panel":
            spec["nine_slice"] = ("角块 ≥24px、切分线对称、"
                                  "同游戏全部面板共用一套切分比例")
        else:
            spec["grid"] = ("统一栅格源图（8px 安全边），引擎内缩放至 "
                            "UiTheme spacing.icon_size 网格")
        cards.append({
            "local_key": f"ui-{it['id']}",
            "origin": "manual_ui",
            "name": f"ui-{it['id']}（{it['usage']}）",
            "category": "ui",
            "source_defs": [],
            "spec": spec,
            "acceptance": it["acceptance"],
            "priority": "P0",  # taxonomy §1: UI 2D is the P0 batch
            "demand_id": None,
            "status": "draft",
            "removed": False,
        })
    return cards


# ---------- SfxKind projection (sfx/mod.rs -> manual audio cards) ---------

def parse_sfx_kinds(source: str) -> list[str]:
    """Extract SfxKind variant names from warbase's sfx/mod.rs source text.

    Sync choice (documented per round-4 NEEDS): PARSE THE SOURCE at scan
    time — variant names/count can never drift from the engine enum. The
    Chinese semantics live in _SFX_USAGE (hardcoded by nature); an unmapped
    variant gets a placeholder usage + a scan problem, and the unit test
    fails until the table is updated (anti-drift in both directions).
    """
    m = re.search(r"pub enum SfxKind\b.*?\{(.*?)\n\}", source, re.S)
    if not m:
        raise ValueError("sfx/mod.rs 中找不到 SfxKind 枚举（引擎侧重构了？）")
    names: list[str] = []
    for line in m.group(1).splitlines():
        s = line.strip()
        if not s or s.startswith("//"):
            continue  # doc comments / attribute lines
        name = s.split(",")[0].strip()
        if re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]*", name):
            names.append(name)
    if not names:
        raise ValueError("SfxKind 枚举解析为空")
    return names


def sfx_snake(name: str) -> str:
    """ClickAck -> click_ack (stable local_key fragment)."""
    return re.sub(r"(?<!^)(?=[A-Z])", "_", name).lower()


def sfx_demand_cards(variants: list[str]) -> list[dict]:
    """Manual-lane audio demand cards, one per SfxKind variant (taxonomy
    §6: the enum IS the demand list). Same never-deleted lane as ui cards:
    empty source_defs, human-owned after creation."""
    cards = []
    for v in variants:
        usage = _SFX_USAGE.get(v, "新变体——usage 语义待补（_SFX_USAGE 表）")
        cards.append({
            "local_key": f"sfx-{sfx_snake(v)}",
            "origin": "sfx",
            "name": f"SFX {v}（{usage}）",
            "category": "audio",
            "source_defs": [],
            "spec": {
                "format": "ogg",
                "duration": "Ack/反馈类 ≤0.5s；波次/胜负类 1-2s",
                "loudness": "音量归一（成批一致）",
                "throttle": "同类 0.1s 节流（THROTTLE_WINDOW）——开头 30ms 内给到辨识点",
                "usage": usage,
            },
            "acceptance": "SfxRequest 通道 round-trip 断言 + 人工听感过审；时长/响度按 spec",
            "priority": "P2",  # taxonomy §6: audio is the P2 batch
            "demand_id": None,
            "status": "draft",
            "removed": False,
        })
    return cards


def merge_manual_cards(demands: list[dict],
                       cards: list[dict]) -> tuple[list[dict], int]:
    """Create-if-missing merge for manual-lane cards (ui/audio projections).

    Never modifies existing entries (humans own them after creation) and
    never deletes (deletion is a human act on the projection file; entries
    marked removed=true are respected and not resurrected). Returns
    (added cards, count already present).
    """
    have = {d["local_key"] for d in demands}
    added = [c for c in cards if c["local_key"] not in have]
    return added, len(cards) - len(added)


# ---------- dump bridge ----------

def repo_root(game_dir: Path) -> Path:
    """Walk up from the game dir to the workspace root (has Cargo.toml + tools/)."""
    for cand in [game_dir.resolve(), *game_dir.resolve().parents]:
        if (cand / "tools" / "art-needs").is_dir() and (cand / "Cargo.toml").is_file():
            return cand
    return Path.cwd()


def dump_maps(repo: Path, game_dir: Path, maps: list[str]) -> list[dict]:
    """Run warbase's mapbundle-dump over each map (paths relative to the
    game dir; passed to cargo as absolute since it runs from the repo root)."""
    units: list[dict] = []
    for m in maps:
        proc = subprocess.run(
            ["cargo", "run", "-q", "-p", "warbase", "--bin", "mapbundle-dump", "--",
             str((game_dir / m).resolve())],
            cwd=repo, capture_output=True, text=True, check=False,
        )
        if proc.returncode != 0:
            raise RuntimeError(f"mapbundle-dump 失败（{m}）:\n{proc.stderr.strip()}")
        units.extend(json.loads(proc.stdout)["units"])
    return units


# ---------- needs file io ----------

def load_needs(game_dir: Path) -> dict:
    path = game_dir / NEEDS_NAME
    if not path.is_file():
        return {"schema_version": 1, "game": game_dir.name, "source": None,
                "source_maps": [], "demands": []}
    return json.loads(path.read_text(encoding="utf-8"))


def save_needs(game_dir: Path, needs: dict) -> None:
    path = game_dir / NEEDS_NAME
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(needs, ensure_ascii=False, indent=2) + "\n",
                    encoding="utf-8")


# ---------- vault api (same-source validation as art-sync) ----------

class _NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        return None


_OPENER = urllib.request.build_opener(_NoRedirect)
_RESOLVED: dict[str, list[str]] = {}


def validated_source(raw: str) -> str:
    """Validate the needs file's source URL (same policy as art-sync):
    http(s) only, internal host only (loopback/private/link-local), no
    credentials/query/fragment, no redirects. The vault is an internal
    service by design — public hosts are rejected.
    """
    parts = urlparse(raw)
    if parts.scheme not in ("http", "https") or not parts.hostname:
        raise ValueError(f"source 必须是 http(s) URL: {raw!r}")
    if parts.username or parts.password or parts.query or parts.fragment:
        raise ValueError(f"source 不允许携带凭据/query/fragment: {raw!r}")
    host = parts.hostname
    if host not in _RESOLVED:
        try:
            infos = socket.getaddrinfo(host, None)
        except socket.gaierror as e:
            raise ValueError(f"source 主机解析失败: {host} ({e})")
        _RESOLVED[host] = [i[4][0] for i in infos]
    for ip in _RESOLVED[host]:
        addr = ipaddress.ip_address(ip)
        if not (addr.is_loopback or addr.is_private or addr.is_link_local):
            raise ValueError(f"source 主机 {host} 解析到公网地址 {ip}，已拒绝"
                             "（岩库应为内网/本机服务）")
    return f"{parts.scheme}://{parts.netloc}/api/v1"


def api(base: str, method: str, path: str, body: dict | None = None) -> dict:
    """Call the vault API under a pre-validated base (see validated_source)."""
    url = f"{base}{path}"
    data = json.dumps(body, ensure_ascii=False).encode() if body is not None else None
    req = urllib.request.Request(url, data=data, method=method,
                                 headers={"Content-Type": "application/json"})
    with _OPENER.open(req, timeout=10) as resp:
        return json.loads(resp.read().decode("utf-8"))


# ---------- commands ----------

def cmd_scan(game_dir: Path, write: bool, with_ui: bool, with_audio: bool,
             strict: bool) -> int:
    needs = load_needs(game_dir)
    problems: list[str] = []

    if not needs.get("source_maps"):
        print(f"[art-needs] {NEEDS_NAME} 没有 source_maps，先补（如 [\"maps/loop.ron\"]）")
        return 1
    try:
        units = dump_maps(repo_root(game_dir), game_dir, needs["source_maps"])
    except RuntimeError as e:
        print(f"[art-needs] {e}")
        return 1

    groups = group_units(units)
    d = diff_demands(needs.get("demands", []), groups,
                     snapshots=needs.get("unit_snapshots"),
                     scanned_units=units, strict=strict)
    mode = "全字段对账（--strict）" if strict else "外观字段过滤（数值静默）"
    print(f"[art-needs] 扫描 {len(needs['source_maps'])} 张图，"
          f"{len(units)} 个 def → {len(groups)} 组需求；"
          f"新增 {len(d['new'])}、消失 {len(d['removed'])}、"
          f"变化 {len(d['changed'])}、静默漂移 {len(d['silent'])}（{mode}）")

    for g in d["new"]:
        print(f"  + {g['local_key']}  ({'+'.join(g['source_defs'])}, kind={g['kind']})")
    for k in d["removed"]:
        print(f"  - {k}  (def 已从地图消失)")
    for k in d["changed"]:
        entry = next(x for x in needs["demands"] if x["local_key"] == k)
        flag = "已提报，需走岩库 changed 流转" if entry.get("demand_id") else "本地草稿，直接改"
        print(f"  ~ {k}  外观相关变化（{flag}）")
        problems.append(f"{k}: 外观相关变化")
    for k in d["silent"]:
        print(f"  . {k}  仅静默字段漂移（数值调参，需求不动）")

    # --- manual-lane merges: UI declaration manifest + SfxKind projection ---
    ui_added: list[dict] = []
    audio_added: list[dict] = []
    if with_ui:
        try:
            ui_cards = ui_demand_cards(load_ui_manifest(game_dir))
        except (OSError, ValueError, json.JSONDecodeError) as e:
            print(f"[art-needs] {UI_MANIFEST_NAME} 读取失败: {e}")
            problems.append("art-ui.json 读取失败")
        else:
            ui_added, kept = merge_manual_cards(needs.get("demands", []), ui_cards)
            print(f"[art-needs] UI 清单 {len(ui_cards)} 项：新增 {len(ui_added)}、"
                  f"已存在 {kept}（manual_ui 通道：只增不改不删）")
    if with_audio:
        try:
            sfx_src = (repo_root(game_dir) / SFX_MOD_REL).read_text(encoding="utf-8")
            variants = parse_sfx_kinds(sfx_src)
            unmapped = [v for v in variants if v not in _SFX_USAGE]
            if unmapped:
                print(f"[art-needs] SfxKind 新变体缺 usage 语义表: {unmapped}"
                      "（补 tools/art-needs _SFX_USAGE）")
                problems.append(f"SfxKind usage 表待补: {unmapped}")
            audio_cards = sfx_demand_cards(variants)
        except (OSError, ValueError) as e:
            print(f"[art-needs] SfxKind 投影失败（{SFX_MOD_REL}）: {e}")
            problems.append("SfxKind 投影失败")
        else:
            audio_added, kept = merge_manual_cards(needs.get("demands", []), audio_cards)
            print(f"[art-needs] SfxKind {len(variants)} 变体：新增 {len(audio_added)}、"
                  f"已存在 {kept}（manual 通道：只增不改不删）")

    if write:
        by_key = {x["local_key"]: x for x in needs["demands"]}
        for g in d["new"]:
            by_key[g["local_key"]] = {
                "local_key": g["local_key"],
                "origin": "scan",
                "name": g["local_key"],
                "category": "model",
                "source_defs": g["source_defs"],
                "spec": default_spec(g),
                "acceptance": default_acceptance(g),
                "priority": "P2" if g["kind"] == "creep" else "P1",
                "demand_id": None,
                "status": "draft",
                "removed": False,
            }
        for k in d["removed"]:
            if k in by_key:
                by_key[k]["removed"] = True
        for c in ui_added + audio_added:
            by_key[c["local_key"]] = c
        needs["demands"] = sorted(by_key.values(), key=lambda x: x["local_key"])
        # Snapshot baselines for the appearance filter: refresh for new and
        # changed groups, backfill missing ones (first run after this
        # feature), keep silently-drifted baselines STALE on purpose
        # (--strict audits read them), prune keys with no live demand.
        snaps = dict(needs.get("unit_snapshots", {}))
        scanned_snaps = unit_snapshots(units, groups)
        for g in d["new"]:
            snaps[g["local_key"]] = scanned_snaps[g["local_key"]]
        for k in d["changed"]:
            snaps[k] = scanned_snaps[k]
        for key, snap in scanned_snaps.items():
            snaps.setdefault(key, snap)
        live = {x["local_key"] for x in needs["demands"]}
        needs["unit_snapshots"] = {k: snaps[k] for k in sorted(snaps)
                                   if k in live}
        save_needs(game_dir, needs)
        print(f"[art-needs] 已写 {game_dir / NEEDS_NAME}（记得 review 后一并提交；"
              f"name/spec 是模板预填，按需改）")

    if d["removed"] and not write:
        problems.append(f"{len(d['removed'])} 组需求 def 已消失未处理（--write 标记 removed）")
    if d["new"] and not write:
        problems.append(f"{len(d['new'])} 组新需求未写入（--write 生成草稿）")
    if ui_added and not write:
        problems.append(f"{len(ui_added)} 项 UI 需求未写入（--write 生成草稿）")
    if audio_added and not write:
        problems.append(f"{len(audio_added)} 项 SFX 需求未写入（--write 生成草稿）")
    return len(problems)


def cmd_submit(game_dir: Path) -> int:
    needs = load_needs(game_dir)
    if not needs.get("source"):
        print(f"[art-needs] {NEEDS_NAME} 缺 source（岩库地址），先补")
        return 1
    try:
        base = validated_source(needs["source"])
    except ValueError as e:
        print(f"[art-needs] {e}")
        return 1

    pending = [d for d in needs["demands"]
               if not d.get("removed") and not d.get("demand_id")]
    if not pending:
        print("[art-needs] 没有待提报的草稿（全部已提报或 removed）")
        return 0

    problems = 0
    for d in pending:
        body = {
            "game_key": needs["game"],
            "name": d["name"],
            "category": d.get("category", "model"),
            "spec": d.get("spec", {}),
            "source_defs": d.get("source_defs", []),
            "acceptance": d.get("acceptance", ""),
            "priority": d.get("priority", "P1"),
        }
        try:
            created = api(base, "POST", "/demands", body)
            did = created["id"]
            # Contract v1: zero-validation status via PATCH (status_change event).
            api(base, "PATCH", f"/demands/{did}",
                {"status": "submitted", "actor": f"art-needs/{needs['game']}",
                 "note": f"scan source_defs={body['source_defs']}"})
            d["demand_id"] = did
            d["status"] = "submitted"
            print(f"[art-needs] 已提报 {d['local_key']} → {did}")
            # Reuse hint: the dedup value point, printed at submit time.
            try:
                from urllib.parse import quote
                sim = api(base, "GET",
                          f"/demands/similar?name={quote(d['name'])}"
                          f"&category={body['category']}")
                for item in sim.get("items", [])[:3]:
                    print(f"           复用提示: 岩库已有「{item['name']}」"
                          f"({item['status']}) {item['id']}")
            except (urllib.error.URLError, ValueError):
                pass  # hint is best-effort, never blocks submit
        except urllib.error.URLError as e:
            print(f"[art-needs] 岩库不可达（服务起了吗？）: {e}")
            problems = len(pending)
            break
        except urllib.error.HTTPError as e:
            print(f"[art-needs] 提报失败 {d['local_key']}: HTTP {e.code} {e.read().decode()[:200]}")
            problems += 1
        # Persist after each submit: a crash mid-batch must not orphan
        # already-submitted demands (idempotency keys off demand_id).
        if d.get("demand_id"):
            save_needs(game_dir, needs)

    if any(d.get("demand_id") for d in pending):
        print(f"[art-needs] 已回填 demand_id 到 {NEEDS_NAME}，记得提交")
    return problems


def cmd_status(game_dir: Path) -> int:
    needs = load_needs(game_dir)
    if not needs.get("source"):
        print(f"[art-needs] {NEEDS_NAME} 缺 source（岩库地址），先补")
        return 1
    try:
        base = validated_source(needs["source"])
    except ValueError as e:
        print(f"[art-needs] {e}")
        return 1
    try:
        remote = api(base, "GET", f"/demands?game_key={needs['game']}&page_size=100")
        cov = api(base, "GET", f"/demands/stats?game_key={needs['game']}")
    except urllib.error.URLError as e:
        print(f"[art-needs] 岩库不可达，本地投影可能过期: {e}")
        return 1

    remote_by_id = {d["id"]: d for d in remote.get("items", [])}
    problems = 0
    print(f"[art-needs] {needs['game']} 覆盖度: 共 {cov['total']} 条")
    for status, n in cov["by_status"].items():
        if n:
            print(f"  {status:10} {n}")

    local_ids = {d["demand_id"] for d in needs["demands"] if d.get("demand_id")}
    changed_projection = False
    for d in needs["demands"]:
        if not d.get("demand_id"):
            continue
        r = remote_by_id.get(d["demand_id"])
        if r is None:
            print(f"  ? {d['local_key']}: 岩库查无此 demand（被删？）")
            problems += 1
            continue
        if r["status"] != d.get("status"):
            print(f"  ~ {d['local_key']}: {d['status']} → {r['status']}")
            d["status"] = r["status"]
            changed_projection = True
        # Zero-validation quality guard (contract v1): the vault never rejects
        # a lying fulfilled — the CLI projection reports it instead.
        if r["status"] == "fulfilled" and r.get("asset_count", 0) == 0:
            print(f"  ! {d['local_key']}: fulfilled 但无关联资产"
                  f"（岩库零校验——补 POST /demands/{d['demand_id']}/assets 或纠正状态）")
            problems += 1
        if r.get("asset_count") and not d.get("asset_ref"):
            detail = api(base, "GET", f"/demands/{d['demand_id']}")
            asset_id = detail["assets"][0]["asset_id"]
            d["asset_ref"] = asset_id
            changed_projection = True
            print(f"  ✓ {d['local_key']}: fulfilled 资产 {asset_id}"
                  f"（可进 art-mapping.json / art-assets.json）")
        if r.get("locked") and not d.get("locked_by_self"):
            d["locked_by_self"] = True
            changed_projection = True
            print(f"  ⊙ {d['local_key']}: 已有锁定记录（三层追溯链闭合）")
    orphan = set(remote_by_id) - local_ids
    for oid in orphan:
        print(f"  ? 岩库有 {oid}「{remote_by_id[oid]['name']}」但本地清单没有")

    if changed_projection:
        save_needs(game_dir, needs)
        print(f"[art-needs] 投影已更新（{NEEDS_NAME}），记得提交")
    return problems


def cmd_sync(game_dir: Path) -> int:
    """Report locks to the vault (contract §6): read art-assets.json entries
    with a `demand` back-link field, POST /demands/{id}/locks per game.
    Idempotent on both sides — re-run after version bumps to refresh."""
    needs = load_needs(game_dir)
    if not needs.get("source"):
        print(f"[art-needs] {NEEDS_NAME} 缺 source（岩库地址），先补")
        return 1
    manifest_path = game_dir / "art-assets.json"
    if not manifest_path.is_file():
        print(f"[art-needs] {manifest_path} 不存在——还没有锁定的岩库资产，无可上报")
        return 0
    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    try:
        base = validated_source(manifest.get("source") or needs["source"])
    except ValueError as e:
        print(f"[art-needs] {e}")
        return 1

    linked = [a for a in manifest.get("assets", []) if a.get("demand")]
    if not linked:
        print(f"[art-needs] 清单 {len(manifest.get('assets', []))} 条资产均无 demand 回链字段，无可上报")
        print("[art-needs] 提示: fulfilled 后在条目加 \"demand\": \"<demand_id>\" 再跑 sync")
        return 0

    problems = 0
    for entry in linked:
        body = {
            "game_key": needs["game"],
            "asset_id": entry["id"],
            "version_no": entry["version"],
            "actor": f"art-needs({needs['game']})",
            "note": f"lockfile {manifest_path.name}",
        }
        try:
            lk = api(base, "POST", f"/demands/{entry['demand']}/locks", body)
            print(f"[art-needs] 锁定上报: {entry.get('name', entry['id'])}"
                  f" v{lk['version_no']} → demand {entry['demand']}")
        except urllib.error.HTTPError as e:
            print(f"[art-needs] 上报失败 {entry.get('name')}: HTTP {e.code}"
                  f" {e.read().decode()[:200]}")
            problems += 1
        except urllib.error.URLError as e:
            print(f"[art-needs] 岩库不可达（服务起了吗？）: {e}")
            problems = len(linked)
            break
    return problems


def main() -> int:
    ap = argparse.ArgumentParser(
        description="MapBundle unit defs -> vault art demands (ADR-0015)")
    ap.add_argument("game", help="game directory, e.g. games/loop-td")
    sub = ap.add_subparsers(dest="cmd", required=True)
    scan = sub.add_parser("scan", help="scan maps and diff the needs file")
    scan.add_argument("--write", action="store_true", help="write updates")
    scan.add_argument("--ui", action="store_true",
                      help="merge docs/art-ui.json declarations as manual_ui "
                           "demand cards (create-only, never removed by scan)")
    scan.add_argument("--audio", action="store_true",
                      help="project SfxKind variants (sfx/mod.rs) as manual "
                           "audio demand cards")
    scan.add_argument("--strict", action="store_true",
                      help="full-field diff audit: every dump-field drift "
                           "counts as changed (appearance filter off)")
    sub.add_parser("submit", help="submit drafts to the vault")
    sub.add_parser("status", help="project vault state locally")
    sub.add_parser("sync", help="report art-assets.json locks to the vault")
    args = ap.parse_args()

    game = Path(args.game)
    if not (game / "maps").is_dir():
        print(f"[art-needs] {game} 不是游戏目录（找不到 maps/）")
        return 1

    if args.cmd == "scan":
        return cmd_scan(game, args.write, args.ui, args.audio, args.strict)
    if args.cmd == "submit":
        return cmd_submit(game)
    if args.cmd == "sync":
        return cmd_sync(game)
    return cmd_status(game)


if __name__ == "__main__":
    sys.exit(main())
