"""DRILL texture generator — procedural "art-grade" nine-grid PNGs for the
round-4 DRILL rehearsal (warbase round4 plan §Phase 2 / ADR-0020).

NOT real art. These stand in for what 岩库 (ArtVault) delivers on the first
art day: a panel base and a slot face (the two texture seams the engine
actually consumes: UI_TEX_PANEL / UI_TEX_SLOT), plus one icon that has NO
engine seam yet (archived here as evidence for the gap record).

Zero dependencies (stdlib zlib/struct/json only). Deterministic: same script
bytes in, same PNG bytes out — committed files never churn.

Slicing contract (the reason ring widths are what they are): the engine
slices panel textures at UiTheme spacing.panel_border_w and slot faces at
slot_border_w (plan-a: 2.0 / 1.0 SOURCE px). A drill texture whose border
ring is thicker than the token would visibly distort when nine-sliced —
that mismatch between the art-ui.json spec (>=24px corner blocks @2x) and
the theme token is DRILL defect F5 (see the runbook's defect list).

Outputs (all inside this drill dir; the real pipeline never reads here):
  vault/drill_panel_v1.png  96x96  palette A (gold ring 2px, 4-hue corners)
  vault/drill_panel_v2.png  96x96  palette A permuted (stripes inverted,
                                   corner hues rotated) — SAME byte size,
                                   different content: exists to demonstrate
                                   the art-sync size-only check pitfall (F1)
  vault/drill_panel_v3.png  112x112 palette B — the "real" upgrade: size
                                   change forces art-sync to re-pull
  vault/drill_slot_v1.png   40x40  palette A (gold ring 1px, corner dots)
  vault/drill_icon_v1.png   128x128 palette A — NOT wired (no icon seam)
  vault/vault_db.json       the mock vault's asset/version/file table
"""

from __future__ import annotations

import json
import struct
import zlib
from pathlib import Path

VAULT_DIR = Path(__file__).resolve().parent / "vault"

# Mock vault ids: 32-hex-shaped like real ArtVault ids, with a readable
# drill marker baked in (d0i1.../d0i2... = drill 01/02) so a drill ref can
# never be confused with a production asset id.
PANEL_ASSET_ID = "d0110000000000000000000000000001"
SLOT_ASSET_ID = "d01200000000000000000000000000002"
PANEL_DEMAND_ID = "dem-drill-0001"
SLOT_DEMAND_ID = "dem-drill-0002"

PALETTE_A = {  # "iron & gold" — visually kin to plan-a tokens
    "ring": (214, 176, 92),
    "inner": (140, 112, 58),
    "stripe_a": (26, 23, 19),
    "stripe_b": (38, 34, 28),
    "corners": [(224, 90, 90), (96, 200, 110), (96, 150, 224), (230, 210, 90)],
}
PALETTE_B = {  # "copper & teal" — unmistakably a different delivery
    "ring": (92, 200, 190),
    "inner": (52, 124, 118),
    "stripe_a": (34, 26, 22),
    "stripe_b": (50, 38, 32),
    "corners": [(224, 96, 200), (240, 150, 70), (170, 230, 80), (150, 120, 235)],
}


def _permuted(pal: dict) -> dict:
    """A permutation of an existing palette: stripes inverted + corner hues
    rotated. Visibly a different delivery, but the pixel PATTERN shape (run
    lengths / frequencies) is identical — which is exactly what makes the
    compressed byte LENGTH identical. v2 exists to demonstrate the art-sync
    size-only check pitfall (F1): same dimensions + same byte count =>
    art-sync believes the local v1 file is already current and never pulls.
    """
    return {
        "ring": pal["ring"],
        "inner": pal["inner"],
        "stripe_a": pal["stripe_b"],
        "stripe_b": pal["stripe_a"],
        "corners": [pal["corners"][3], pal["corners"][2],
                    pal["corners"][1], pal["corners"][0]],
    }


def nine_grid(w: int, h: int, ring: int, pal: dict, corner: int) -> bytes:
    """RGBA buffer: `ring`-px frame + inner line, four hue blocks in the
    corners (corner-unscaled nine-grid behavior reads at a glance), diagonal
    stripes in the stretch center."""
    ra, ga, ba = pal["ring"]
    ri, gi, bi = pal["inner"]
    sa, sb = pal["stripe_a"], pal["stripe_b"]
    px = bytearray(w * h * 4)
    for y in range(h):
        for x in range(w):
            i = (y * w + x) * 4
            on_ring = x < ring or y < ring or x >= w - ring or y >= h - ring
            on_inner = not on_ring and (
                x < ring + 1 or y < ring + 1 or x >= w - ring - 1 or y >= h - ring - 1
            )
            if on_ring:
                c = (ra, ga, ba)
            elif on_inner:
                c = (ri, gi, bi)
            else:
                # Corner hue blocks (corner quadrants, inset by the ring).
                in_tl = x < ring + corner and y < ring + corner
                in_tr = x >= w - ring - corner and y < ring + corner
                in_bl = x < ring + corner and y >= h - ring - corner
                in_br = x >= w - ring - corner and y >= h - ring - corner
                if in_tl:
                    c = pal["corners"][0]
                elif in_tr:
                    c = pal["corners"][1]
                elif in_bl:
                    c = pal["corners"][2]
                elif in_br:
                    c = pal["corners"][3]
                else:
                    c = sa if (x + y) % 8 < 4 else sb
            px[i], px[i + 1], px[i + 2], px[i + 3] = *c, 255
    return bytes(px)


def icon_128(pal: dict) -> bytes:
    """128x128 transparent icon with an 8px safe margin (the art-ui.json icon
    spec) — a chunky X on a plated square. Archived only: the engine has no
    icon surface yet (defect F4)."""
    w = h = 128
    m = 8  # safe margin
    ra, ga, ba = pal["ring"]
    px = bytearray(w * h * 4)
    for y in range(h):
        for x in range(w):
            i = (y * w + x) * 4
            in_plate = m + 6 <= x < w - m - 6 and m + 6 <= y < h - m - 6
            on_plate_edge = (
                m + 6 <= x < w - m - 6 and m + 6 <= y < h - m - 6
            ) and (
                x < m + 8 or y < m + 8 or x >= w - m - 8 or y >= h - m - 8
            )
            rel_x, rel_y = x - (m + 6), y - (m + 6)
            span = w - 2 * (m + 6)
            on_x = in_plate and abs(rel_x - rel_y) <= 7 or in_plate and abs(
                rel_x + rel_y - span
            ) <= 7
            if on_plate_edge:
                c = (ra, ga, ba)
            elif on_x:
                c = pal["corners"][0]
            elif in_plate:
                c = pal["stripe_b"]
            else:
                continue  # transparent
            px[i], px[i + 1], px[i + 2], px[i + 3] = *c, 255
    return bytes(px)


# ---- minimal PNG encoder (RGBA8, filter 0, zlib-compressed IDAT) ----------


def _chunk(tag: bytes, data: bytes) -> bytes:
    return (
        struct.pack(">I", len(data))
        + tag
        + data
        + struct.pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF)
    )


def write_png(path: Path, w: int, h: int, rgba: bytes) -> int:
    ihdr = struct.pack(">IIBBBBB", w, h, 8, 6, 0, 0, 0)
    stride = w * 4
    raw = b"".join(b"\x00" + rgba[y * stride : (y + 1) * stride] for y in range(h))
    png = (
        b"\x89PNG\r\n\x1a\n"
        + _chunk(b"IHDR", ihdr)
        + _chunk(b"IDAT", zlib.compress(raw, 6))
        + _chunk(b"IEND", b"")
    )
    path.write_bytes(png)
    return len(png)


def main() -> None:
    vault = VAULT_DIR
    vault.mkdir(parents=True, exist_ok=True)

    files: dict[str, int] = {}
    files["drill_panel_v1.png"] = write_png(
        vault / "drill_panel_v1.png", 96, 96, nine_grid(96, 96, 2, PALETTE_A, 14)
    )
    files["drill_panel_v2.png"] = write_png(
        vault / "drill_panel_v2.png", 96, 96, nine_grid(96, 96, 2, _permuted(PALETTE_A), 14)
    )
    files["drill_panel_v3.png"] = write_png(
        vault / "drill_panel_v3.png", 112, 112, nine_grid(112, 112, 2, PALETTE_B, 14)
    )
    files["drill_slot_v1.png"] = write_png(
        vault / "drill_slot_v1.png", 40, 40, nine_grid(40, 40, 1, PALETTE_A, 5)
    )
    files["drill_icon_v1.png"] = write_png(
        vault / "drill_icon_v1.png", 128, 128, icon_128(PALETTE_A)
    )

    # The mock vault's world: assets (versions -> files) + demands, exactly
    # the shapes tools/art-sync and tools/art-needs consume. "path" is mock
    # -internal (where the bytes live in this dir); the API projects it away.
    db = {
        "note": (
            "DRILL mock vault store — NOT production data. Stands in for the "
            "real ArtVault REST service during the first-art-day rehearsal. "
            "Served by mock_vault.py on 127.0.0.1:8799."
        ),
        "demands": [
            {
                "id": PANEL_DEMAND_ID,
                "name": "ui-panel-command-card（演练）",
                "status": "fulfilled",
                "assets": [{"asset_id": PANEL_ASSET_ID}],
            },
            {
                "id": SLOT_DEMAND_ID,
                "name": "ui-slot-command（演练；正式需求板无此卡——缺陷 F3）",
                "status": "fulfilled",
                "assets": [{"asset_id": SLOT_ASSET_ID}],
            },
        ],
        "assets": [
            {
                "id": PANEL_ASSET_ID,
                "name": "drill_panel nine-grid",
                "versions": [
                    {
                        "version_no": 1,
                        "stable": True,
                        "files": [
                            {
                                "id": "file-drill-panel-v1",
                                "filename": "drill_panel.png",
                                "size": files["drill_panel_v1.png"],
                                "path": "drill_panel_v1.png",
                            }
                        ],
                    },
                    {
                        "version_no": 2,
                        "stable": False,
                        "files": [
                            {
                                "id": "file-drill-panel-v2",
                                "filename": "drill_panel.png",
                                "size": files["drill_panel_v2.png"],
                                "path": "drill_panel_v2.png",
                            }
                        ],
                    },
                    {
                        "version_no": 3,
                        "stable": True,
                        "files": [
                            {
                                "id": "file-drill-panel-v3",
                                "filename": "drill_panel.png",
                                "size": files["drill_panel_v3.png"],
                                "path": "drill_panel_v3.png",
                            }
                        ],
                    },
                ],
            },
            {
                "id": SLOT_ASSET_ID,
                "name": "drill_slot nine-grid",
                "versions": [
                    {
                        "version_no": 1,
                        "stable": True,
                        "files": [
                            {
                                "id": "file-drill-slot-v1",
                                "filename": "drill_slot.png",
                                "size": files["drill_slot_v1.png"],
                                "path": "drill_slot_v1.png",
                            }
                        ],
                    }
                ],
            },
        ],
    }
    (vault / "vault_db.json").write_text(
        json.dumps(db, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
    )
    for name, size in sorted(files.items()):
        print(f"wrote vault/{name} ({size} B)")


if __name__ == "__main__":
    main()
