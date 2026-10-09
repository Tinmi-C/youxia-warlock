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
    python3 tools/art-needs/art_needs.py games/loop-td submit         # drafts -> vault
    python3 tools/art-needs/art_needs.py games/loop-td status         # state projection

Exit code = number of problems (0 = clean), same convention as art-sync.
Zero dependencies (Python 3 stdlib only). Human and AI run the same command.
"""
from __future__ import annotations

import argparse
import ipaddress
import json
import socket
import subprocess
import sys
import urllib.error
import urllib.request
from pathlib import Path
from urllib.parse import urlparse

NEEDS_NAME = "docs/art-needs.json"
MAPPING_NAME = "art-mapping.json"

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


def diff_demands(existing: list[dict], groups: list[dict]) -> dict:
    """Diff a needs file's entries against freshly scanned groups.

    Returns {'new': [groups], 'removed': [local_keys], 'changed': [local_keys]}.
    'changed' = source_defs drift on an entry — if it is already submitted,
    the vault side wants a `changed` transition there (its ADR-0006).
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
    # concept art, …) own no defs and are removed by humans, never by scan.
    removed = [k for k, d in by_key.items()
               if d.get("source_defs")
               and not (set(d["source_defs"]) & scanned_defs)]
    changed = [k for k, d in by_key.items()
               if k in scanned_by_key
               and set(d.get("source_defs", [])) != scanned_by_key[k]]
    return {"new": new, "removed": removed, "changed": changed}


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

def cmd_scan(game_dir: Path, write: bool) -> int:
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
    d = diff_demands(needs.get("demands", []), groups)
    print(f"[art-needs] 扫描 {len(needs['source_maps'])} 张图，"
          f"{len(units)} 个 def → {len(groups)} 组需求；"
          f"新增 {len(d['new'])}、消失 {len(d['removed'])}、def 集变化 {len(d['changed'])}")

    for g in d["new"]:
        print(f"  + {g['local_key']}  ({'+'.join(g['source_defs'])}, kind={g['kind']})")
    for k in d["removed"]:
        print(f"  - {k}  (def 已从地图消失)")
    for k in d["changed"]:
        entry = next(x for x in needs["demands"] if x["local_key"] == k)
        flag = "已提报，需走岩库 changed 流转" if entry.get("demand_id") else "本地草稿，直接改"
        print(f"  ~ {k}  def 集变化（{flag}）")
        problems.append(f"{k}: def 集变化")

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
        needs["demands"] = sorted(by_key.values(), key=lambda x: x["local_key"])
        save_needs(game_dir, needs)
        print(f"[art-needs] 已写 {game_dir / NEEDS_NAME}（记得 review 后一并提交；"
              f"name/spec 是模板预填，按需改）")

    if d["removed"] and not write:
        problems.append(f"{len(d['removed'])} 组需求 def 已消失未处理（--write 标记 removed）")
    if d["new"] and not write:
        problems.append(f"{len(d['new'])} 组新需求未写入（--write 生成草稿）")
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
            api(base, "POST", f"/demands/{did}/transition",
                {"to": "submitted", "actor": f"art-needs/{needs['game']}",
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
        cov = api(base, "GET", f"/demands/coverage?game_key={needs['game']}")
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
        if r.get("asset_ids") and not d.get("asset_ref"):
            d["asset_ref"] = r["asset_ids"][0]
            changed_projection = True
            print(f"  ✓ {d['local_key']}: fulfilled 资产 {r['asset_ids'][0]}"
                  f"（可进 art-mapping.json / art-assets.json）")
    orphan = set(remote_by_id) - local_ids
    for oid in orphan:
        print(f"  ? 岩库有 {oid}「{remote_by_id[oid]['name']}」但本地清单没有")

    if changed_projection:
        save_needs(game_dir, needs)
        print(f"[art-needs] 投影已更新（{NEEDS_NAME}），记得提交")
    return problems


def main() -> int:
    ap = argparse.ArgumentParser(
        description="MapBundle unit defs -> vault art demands (ADR-0015)")
    ap.add_argument("game", help="game directory, e.g. games/loop-td")
    sub = ap.add_subparsers(dest="cmd", required=True)
    scan = sub.add_parser("scan", help="scan maps and diff the needs file")
    scan.add_argument("--write", action="store_true", help="write updates")
    sub.add_parser("submit", help="submit drafts to the vault")
    sub.add_parser("status", help="project vault state locally")
    args = ap.parse_args()

    game = Path(args.game)
    if not (game / "maps").is_dir():
        print(f"[art-needs] {game} 不是游戏目录（找不到 maps/）")
        return 1

    if args.cmd == "scan":
        return cmd_scan(game, args.write)
    if args.cmd == "submit":
        return cmd_submit(game)
    return cmd_status(game)


if __name__ == "__main__":
    sys.exit(main())
