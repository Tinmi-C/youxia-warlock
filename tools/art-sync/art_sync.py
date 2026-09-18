#!/usr/bin/env python3
"""art_sync — pull vault (ArtVault) assets into a game's assets/ dir.

Bridge between the asset manager (岩库 ArtVault, REST API) and this repo.
Reads <game>/art-assets.json (the lockfile: asset id + pinned version +
file mapping), downloads missing/changed files, reports drift.

Zero dependencies (Python 3 stdlib only). Human and AI run the same command.

Usage (from repo root):
    python3 tools/art-sync/art_sync.py games/warden            # sync
    python3 tools/art-sync/art_sync.py games/warden --dry-run  # plan only
    python3 tools/art-sync/art_sync.py games/warden --outdated # + version hints

Exit code = number of problems (0 = clean), same convention as art-catalog.
Design rationale: docs/decisions/0009-art-vault-to-game-asset-flow.md
"""
from __future__ import annotations

import argparse
import ipaddress
import json
import re
import socket
import sys
import urllib.error
import urllib.request
from pathlib import Path, PurePosixPath
from urllib.parse import urlparse

MANIFEST_NAME = "art-assets.json"

# Destination layout is fixed by construction: assets/<subdir>/<one file>.
# A manifest path may only name one file inside one of these subdirs.
ASSET_SUBDIRS = ("models", "textures", "audio", "fonts", "ui")
SAFE_NAME = re.compile(r"[A-Za-z0-9._-]+\Z")

# The vault is an internal service: allow loopback / private / link-local
# hosts only. Public hostnames are rejected by design.
class _NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        return None


_OPENER = urllib.request.build_opener(_NoRedirect)
_RESOLVED: dict[str, list[str]] = {}


def validated_source(raw: str) -> str:
    """Validate the manifest's source URL: http(s) only, internal host only."""
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


def http_json(url: str) -> dict:
    with _OPENER.open(url, timeout=10) as resp:
        return json.loads(resp.read().decode("utf-8"))


def dest_under(assets_root: Path, rel: str) -> Path:
    """Build assets/<subdir>/<name> from the manifest mapping, or raise.

    Containment comes from construction: exactly two fixed components, the
    first restricted to the five conventional subdirs, the second matching
    a conservative filename charset (no separators, no '..').
    """
    if "\\" in rel:
        raise ValueError(f"非法的目标路径 {rel!r}")
    parts = PurePosixPath(rel).parts
    if len(parts) != 2 or parts[0] not in ASSET_SUBDIRS or not SAFE_NAME.match(parts[1]):
        raise ValueError(f"目标路径必须是 {'/'.join(ASSET_SUBDIRS)} 之一的单文件名: {rel!r}")
    return assets_root.joinpath(parts[0], parts[1])


def download(url: str, dest: Path) -> None:
    """Stream to <dest>.part next to the validated dest, atomic rename."""
    dest.parent.mkdir(parents=True, exist_ok=True)
    tmp = dest.with_name(dest.name + ".part")
    with _OPENER.open(url, timeout=60) as resp, open(tmp, "wb") as out:
        length = int(resp.headers.get("Content-Length", "0"))
        copied = 0
        while True:
            chunk = resp.read(1 << 16)
            if not chunk:
                break
            out.write(chunk)
            copied += len(chunk)
    if length and copied != length:
        tmp.unlink(missing_ok=True)
        raise IOError(f"short read: {copied}/{length} bytes")
    tmp.replace(dest)


def main() -> int:
    ap = argparse.ArgumentParser(description="Sync ArtVault assets into a game dir")
    ap.add_argument("game", help="game directory, e.g. games/warden")
    ap.add_argument("--dry-run", action="store_true", help="report plan only, write nothing")
    ap.add_argument("--outdated", action="store_true",
                    help="also compare pinned versions against vault latest/stable")
    args = ap.parse_args()

    game = Path(args.game)
    manifest_path = game / MANIFEST_NAME
    if not manifest_path.is_file():
        print(f"[art-sync] 找不到 {manifest_path}（先建清单，格式见 tools/art-sync/README.md）")
        return 1
    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    try:
        source = validated_source(manifest["source"])
    except ValueError as e:
        print(f"[art-sync] {e}")
        return 1
    assets_root = game / "assets"

    problems: list[str] = []
    actions = 0

    for entry in manifest.get("assets", []):
        asset_id = entry["id"]
        pinned = entry["version"]
        label = entry.get("name", asset_id)
        try:
            detail = http_json(f"{source}/assets/{asset_id}")
        except urllib.error.HTTPError as e:
            problems.append(f"{label}: 岩库查询失败 HTTP {e.code}")
            continue
        except (urllib.error.URLError, ValueError) as e:
            problems.append(f"{label}: 岩库不可达（服务起了吗？）: {e}")
            continue

        if detail.get("name") != label:
            print(f"[art-sync] 提示: {label} 在岩库已改名为「{detail.get('name')}」")

        versions = {v["version_no"]: v for v in detail.get("versions", [])}
        v = versions.get(pinned)
        if v is None:
            problems.append(f"{label}: 清单锁定的 v{pinned} 在岩库不存在"
                            f"（现有: {sorted(versions)}）")
            continue

        if args.outdated:
            latest = max(versions)
            stables = [n for n, vv in versions.items() if vv.get("stable")]
            hint = f"最新 v{latest}"
            if stables:
                hint += f"，稳定 v{max(stables)}"
            if latest > pinned:
                print(f"[art-sync] 可升级: {label} 锁定 v{pinned}，岩库 {hint}")

        by_name = {f["filename"]: f for f in v.get("files", [])}
        for vault_name, rel in entry.get("files", {}).items():
            f = by_name.get(vault_name)
            if f is None:
                problems.append(f"{label}: v{pinned} 里没有文件 {vault_name}")
                continue
            try:
                dest = dest_under(assets_root, rel)
            except ValueError as e:
                problems.append(f"{label}/{vault_name}: {e}")
                continue
            if dest.is_file() and dest.stat().st_size == f["size"]:
                print(f"[art-sync] ok: {dest} (v{pinned}, {f['size']} bytes)")
                continue
            reason = "尺寸不一致" if dest.exists() else "本地缺失"
            if args.dry_run:
                print(f"[art-sync] 将下载: {dest} <- {vault_name} v{pinned} ({reason})")
            else:
                try:
                    download(f"{source}/files/{f['id']}/download", dest)
                    print(f"[art-sync] 已下载: {dest} (v{pinned}, {reason})")
                    actions += 1
                except (IOError, urllib.error.URLError) as e:
                    problems.append(f"{label}/{vault_name}: 下载失败 {e}")

    if problems:
        print(f"\n[art-sync] 完成，{len(problems)} 个问题：", file=sys.stderr)
        for p in problems:
            print(f"  - {p}", file=sys.stderr)
    else:
        verb = "已同步" if actions else "无需变更"
        print(f"\n[art-sync] {verb}（{len(manifest.get('assets', []))} 个资产，"
              f"清单 {manifest_path}）")
        if not args.dry_run and actions:
            print("[art-sync] 记得把更新的资产文件与 art-assets.json 一起提交")
    return len(problems)


if __name__ == "__main__":
    sys.exit(main())
