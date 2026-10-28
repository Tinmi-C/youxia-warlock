"""mock_vault — a miniature stand-in for the real ArtVault REST service
(FastAPI + SQLite in another repo), used ONLY by the round-4 DRILL first-
art-day rehearsal. Serves the exact endpoint shapes tools/art-sync and
tools/art-needs consume, from vault/vault_db.json + the drill PNGs.

Why this exists: art-sync has no offline/local mode — it hard-requires a
live vault API (README: "前置：岩库服务在本机跑着"). Rehearsing the lock
chain (fulfilled -> art-mapping -> art-assets -> art-sync -> --art) without
the real internal vault therefore needs a loopback stand-in (DRILL defect
F8: suggest an --offline / local-verify mode in art-sync itself).

Run:   python3 games/loop-td/assets-art-drill/mock_vault.py   (Ctrl-C stops)
Serves http://127.0.0.1:8799/api/v1 — the drill manifests point here.
Zero dependencies (stdlib http.server). Locks POSTed by art-needs sync are
kept in memory only; restart resets them (the drill re-runs sync anyway).
"""

from __future__ import annotations

import json
import sys
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from urllib.parse import parse_qs, urlparse

HOST, PORT = "127.0.0.1", 8799
VAULT_DIR = Path(__file__).resolve().parent / "vault"
DB = json.loads((VAULT_DIR / "vault_db.json").read_text(encoding="utf-8"))

ASSETS = {a["id"]: a for a in DB["assets"]}
DEMANDS = {d["id"]: d for d in DB["demands"]}
LOCKS: dict[str, list[dict]] = {}  # demand_id -> posted lock bodies (memory)


def file_row(aid: str, version_no: int) -> dict | None:
    for v in ASSETS[aid]["versions"]:
        if v["version_no"] == version_no:
            return v["files"][0]
    return None


def demand_public(d: dict) -> dict:
    return {
        "id": d["id"],
        "name": d["name"],
        "status": d["status"],
        "game_key": "loop-td",
        "asset_count": len(d["assets"]),
        "assets": d["assets"],
        "locked": bool(LOCKS.get(d["id"])),
    }


class Handler(BaseHTTPRequestHandler):
    def _json(self, code: int, body) -> None:
        raw = json.dumps(body, ensure_ascii=False).encode("utf-8")
        self.send_response(code)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(raw)))
        self.end_headers()
        self.wfile.write(raw)

    def _bytes(self, code: int, raw: bytes, ctype: str) -> None:
        self.send_response(code)
        self.send_header("Content-Type", ctype)
        self.send_header("Content-Length", str(len(raw)))
        self.end_headers()
        self.wfile.write(raw)

    def log_message(self, fmt: str, *args) -> None:
        print(f"[mock-vault] {self.address_string()} {fmt % args}", flush=True)

    def do_GET(self) -> None:  # noqa: N802 (http.server naming)
        url = urlparse(self.path)
        parts = [p for p in url.path.split("/") if p]
        # parts == ["api", "v1", ...]
        if len(parts) < 3:
            self._json(404, {"detail": "not found"})
            return
        tail = parts[2:]
        if tail[0] == "assets" and len(tail) == 2 and tail[1] in ASSETS:
            a = ASSETS[tail[1]]
            self._json(
                200,
                {
                    "id": a["id"],
                    "name": a["name"],
                    "status": "usable",
                    "versions": [
                        {
                            "version_no": v["version_no"],
                            "stable": v["stable"],
                            "files": [
                                {k: f[k] for k in ("id", "filename", "size")}
                                for f in v["files"]
                            ],
                        }
                        for v in a["versions"]
                    ],
                },
            )
        elif tail[0] == "files" and len(tail) == 3 and tail[2] == "download":
            fid = tail[1]
            for a in DB["assets"]:
                for v in a["versions"]:
                    for f in v["files"]:
                        if f["id"] == fid:
                            raw = (VAULT_DIR / f["path"]).read_bytes()
                            self._bytes(200, raw, "application/octet-stream")
                            return
            self._json(404, {"detail": f"no file {fid}"})
        elif tail[0] == "demands" and len(tail) == 1:
            q = parse_qs(url.query)
            game = (q.get("game_key") or [""])[0]
            items = [
                demand_public(d) for d in DB["demands"] if game in ("", "loop-td")
            ]
            self._json(200, {"items": items, "total": len(items)})
        elif tail[0] == "demands" and len(tail) == 2 and tail[1] == "stats":
            by: dict[str, int] = {}
            for d in DB["demands"]:
                by[d["status"]] = by.get(d["status"], 0) + 1
            self._json(200, {"total": len(DB["demands"]), "by_status": by})
        elif tail[0] == "demands" and len(tail) == 2 and tail[1] in DEMANDS:
            self._json(200, demand_public(DEMANDS[tail[1]]))
        else:
            self._json(404, {"detail": f"no route {url.path}"})

    def do_POST(self) -> None:  # noqa: N802
        url = urlparse(self.path)
        parts = [p for p in url.path.split("/") if p]
        if (
            len(parts) == 5
            and parts[2] == "demands"
            and parts[4] == "locks"
            and parts[3] in DEMANDS
        ):
            n = int(self.headers.get("Content-Length", "0"))
            body = json.loads(self.rfile.read(n) or b"{}")
            LOCKS.setdefault(parts[3], []).append(body)
            self._json(200, {**body, "demand_id": parts[3], "ok": True})
            return
        self._json(404, {"detail": f"no route {url.path}"})


def main() -> int:
    print(
        f"[mock-vault] DRILL stand-in serving http://{HOST}:{PORT}/api/v1 "
        f"({len(ASSETS)} assets, {len(DEMANDS)} demands from vault/vault_db.json)",
        flush=True,
    )
    ThreadingHTTPServer((HOST, PORT), Handler).serve_forever()


if __name__ == "__main__":
    sys.exit(main())
