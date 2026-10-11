# ART9 ballista part-surgery (AI-surgeon, docs/topics/art/surgery-workflow.md):
# split the vault's single-mesh ballista (v2) into body + string parts without
# moving a vertex in world space. The string is the rod riding the rail that
# ART10 animates along X; the model has no bolt (documented in the ART9 record).
#
# Why an edge-connected island: v2 is 766 edge-connected islands (the
# generator splits verts for UVs). Position-welding is useless here — the
# islands share bit-identical vertices across the WHOLE model, so any
# position merge collapses everything into one blob. The string rod,
# however, is a single clean 48-vert island (its isolation render shows a
# complete rod: shaft, ends, full cross-section), so the pinned bbox picks
# exactly that island. Completeness is re-verified visually at acceptance.
#
# Output layout mirrors v2's own convention: the rotation sits ON the part
# nodes (v2's node_0 carries it), never on an intermediate empty — tools that
# drop non-mesh nodes (turntable.py's KEEP_TYPES) would discard an empty's
# transform and topple the model:
#   body   (rotation = v2 node_0's Rx(90°); verts = v2 accessor coords)
#   string (same rotation + translation = pivot; verts centered on the pivot)
#
# Usage:
#   blender -b -P surgery_ballista.py -- --input weapon_ballista.glb --out v3.glb
#   blender -b -P surgery_ballista.py -- --input ... --out ... --dry-run
#
# Exit codes: 0 ok; 2 string not found / ambiguous; 3 bad input or frame
# drift; 4 conservation or self-check failure.

import argparse
import json
import struct
import sys
from pathlib import Path

import bpy
from mathutils import Matrix, Vector

# v2 measured facts (accessor space) — the contract this surgery must keep.
V2_BBOX_ACC = ((-0.5776, -0.3047, -0.4634), (0.5814, 0.2738, 0.0))
# Same boxes after v2's node rotation Rx(90°) — the world-space zero-drift pins.
V2_BBOX_WORLD = ((-0.5776, 0.0, -0.3047), (0.5814, 0.4634, 0.2738))
STRING_BBOX_WORLD = ((-0.204, 0.334, -0.107), (0.323, 0.353, -0.032))
V2_TOTAL_INDICES = 30000
V2_VERT_COUNT = 9625
# The string rod, v2 accessor frame (x, y, z).
STRING_BBOX_ACC = ((-0.204, -0.107, -0.353), (0.323, -0.032, -0.334))
BBOX_TOL = 0.01
ACC_TOL = 2e-3


def fail(code, msg):
    print(f"FAIL[{code}]: {msg}", file=sys.stderr)
    sys.exit(code)


def parse_args():
    p = argparse.ArgumentParser(description="ballista part surgery v2 -> v3")
    p.add_argument("--input", required=True)
    p.add_argument("--out", required=True)
    p.add_argument("--dry-run", action="store_true")
    return p.parse_args(sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else [])


def acc_bbox_to_blender(abbox):
    """Range-map an accessor bbox into the importer's v.co frame.
    v.co = (x, -z_acc, y_acc): the negated axis swaps min/max, so corners
    cannot be converted independently."""
    (ax0, ay0, az0), (ax1, ay1, az1) = abbox
    return ((ax0, -az1, ay0), (ax1, -az0, ay1))


def close(a, b, tol):
    return all(abs(a[i] - b[i]) <= tol for i in range(3))


def bbox_of_verts(verts):
    minv = Vector((min(v.co.x for v in verts), min(v.co.y for v in verts), min(v.co.z for v in verts)))
    maxv = Vector((max(v.co.x for v in verts), max(v.co.y for v in verts), max(v.co.z for v in verts)))
    return minv, maxv


def edge_components(mesh):
    """Union-find over mesh edges only (islands)."""
    import bmesh
    bm = bmesh.new()
    bm.from_mesh(mesh)
    bm.verts.ensure_lookup_table()
    parent = list(range(len(bm.verts)))

    def find(x):
        while parent[x] != x:
            parent[x] = parent[parent[x]]
            x = parent[x]
        return x

    def union(a, b):
        ra, rb = find(a), find(b)
        if ra != rb:
            parent[ra] = rb

    for e in bm.edges:
        union(e.verts[0].index, e.verts[1].index)
    comps = {}
    for v in bm.verts:
        comps.setdefault(find(v.index), []).append(v)
    return bm, comps, find


def main():
    args = parse_args()
    src = Path(args.input)
    if not src.is_file():
        fail(3, f"input missing: {src}")

    # Import frame self-check: the importer's Y-up bake is a measured fact this
    # script depends on; a future Blender changing it must fail loudly (exit 3).
    bpy.ops.wm.read_factory_settings(use_empty=True)
    bpy.ops.import_scene.gltf(filepath=str(src))
    meshes = [o for o in bpy.data.objects if o.type == "MESH"]
    if len(meshes) != 1:
        fail(3, f"expected exactly one mesh object, got {len(meshes)}")
    obj = meshes[0]
    mesh = obj.data
    if len(mesh.vertices) != V2_VERT_COUNT:
        fail(3, f"vertex count {len(mesh.vertices)} != {V2_VERT_COUNT} (wrong asset version?)")

    minv, maxv = Vector((1e9, 1e9, 1e9)), Vector((-1e9, -1e9, -1e9))
    for v in mesh.vertices:
        minv.x = min(minv.x, v.co.x); maxv.x = max(maxv.x, v.co.x)
        minv.y = min(minv.y, v.co.y); maxv.y = max(maxv.y, v.co.y)
        minv.z = min(minv.z, v.co.z); maxv.z = max(maxv.z, v.co.z)
    want_min, want_max = acc_bbox_to_blender(V2_BBOX_ACC)
    if not (close(minv, want_min, BBOX_TOL) and close(maxv, want_max, BBOX_TOL)):
        fail(3, f"import frame drift: v.co bbox [{minv},{maxv}] want [{want_min},{want_max}]")

    M = obj.matrix_world
    loc, rot, scale = M.decompose()
    if any(abs(s - 1.0) > 1e-4 for s in scale) or loc.length > 1e-4:
        fail(3, f"v2 node transform must be pure rotation, got loc={loc} scale={scale}")

    # Locate the string: the edge-island whose bbox matches the pinned rod
    # position (acc frame, range-mapped). Not found / ambiguous -> exit 2.
    bm, comps, find = edge_components(mesh)
    s_min, s_max = acc_bbox_to_blender(STRING_BBOX_ACC)
    seeds = []
    for root, verts in comps.items():
        lo, hi = bbox_of_verts(verts)
        if close(lo, s_min, BBOX_TOL) and close(hi, s_max, BBOX_TOL):
            seeds.append((root, verts))
    if len(seeds) != 1:
        fail(2, f"string seed matches: {len(seeds)} (want 1); the pinned bbox no longer picks a unique rod")
    string_root, string_verts = seeds[0]
    string_faces = [f for f in bm.faces if find(f.verts[0].index) == string_root]
    lo, hi = bbox_of_verts(string_verts)
    pivot = (lo + hi) * 0.5
    body_faces = [f for f in bm.faces if find(f.verts[0].index) != string_root]
    if len(string_faces) + len(body_faces) != len(bm.faces):
        fail(4, "face partition mismatch")

    print("plan:")
    print(f"  edge islands      : {len(comps)}")
    print(f"  string verts/faces: {len(string_verts)} / {len(string_faces)}")
    print(f"  body   verts/faces: {len(mesh.vertices) - len(string_verts)} / {len(body_faces)}")
    print(f"  string bbox (v.co): [{lo}, {hi}]")
    print(f"  pivot (v.co)      : {pivot}")
    print(f"  root rotation     : {rot.to_euler()}")

    if args.dry_run:
        print("dry-run: nothing written")
        return

    # Build the v3 scene: both parts carry v2's node rotation themselves.
    import bmesh

    def parted_mesh(keep_string):
        b2 = bmesh.new()
        b2.from_mesh(mesh)
        drop = []
        for f in b2.faces:
            is_string = find(f.verts[0].index) == string_root
            if is_string != keep_string:
                drop.append(f)
        bmesh.ops.delete(b2, geom=drop, context="FACES")
        out = bpy.data.meshes.new("body" if not keep_string else "string")
        b2.to_mesh(out)
        b2.free()
        return out

    body = bpy.data.objects.new("body", parted_mesh(False))
    body.matrix_world = M
    bpy.context.scene.collection.objects.link(body)

    string_mesh = parted_mesh(True)
    for v in string_mesh.vertices:
        v.co -= pivot
    string = bpy.data.objects.new("string", string_mesh)
    string.matrix_world = M @ Matrix.Translation(pivot)
    bpy.context.scene.collection.objects.link(string)

    for m in (body.data, string.data):
        for slot in mesh.materials:
            m.materials.append(slot)

    bpy.data.objects.remove(obj, do_unlink=True)
    bpy.context.view_layer.update()

    out_path = Path(args.out)
    if out_path.exists():
        out_path.unlink()
    bpy.ops.export_scene.gltf(filepath=str(out_path), export_format="GLB",
                              export_apply=False, export_animations=False,
                              export_yup=True)

    # Self-check the exported container against the same contract the Rust
    # gate pins: named parts, translation-only, bbox conservation, indices.
    def read_glb_json(p):
        b = p.read_bytes()
        clen = struct.unpack_from("<I", b, 12)[0]
        return json.loads(b[20:20 + clen])

    doc = read_glb_json(out_path)
    anims = len(doc.get("animations", []))
    if anims:
        out_path.unlink()
        fail(4, f"export produced {anims} animations")
    named = {n.get("name"): n for n in doc["nodes"]}
    named = {n.get("name"): n for n in doc["nodes"]}
    quat = (rot.x, rot.y, rot.z, rot.w)  # glTF order; mathutils iterates (w,x,y,z)
    for part in ("body", "string"):
        n = named.get(part)
        if n is None or "mesh" not in n or "scale" in n:
            out_path.unlink()
            fail(4, f"node {part} missing or scaled: {n}")
        r = n.get("rotation", [0, 0, 0, 1])
        if any(abs(r[i] - quat[i]) > 1e-3 for i in range(4)):
            out_path.unlink()
            fail(4, f"node {part} rotation {r} != v2's {quat}")

    def acc_bbox(node):
        mn = [1e9] * 3
        mx = [-1e9] * 3
        for prim in doc["meshes"][node["mesh"]]["primitives"]:
            acc = doc["accessors"][prim["attributes"]["POSITION"]]
            for i in range(3):
                mn[i] = min(mn[i], acc["min"][i])
                mx[i] = max(mx[i], acc["max"][i])
        # corners through the node rotation (axis-aligned 90s: exact), + translate
        r = node.get("rotation", [0, 0, 0, 1])
        x, y, z, w = r
        # rotation matrix from quaternion
        m = [
            [1 - 2 * (y * y + z * z), 2 * (x * y - z * w), 2 * (x * z + y * w)],
            [2 * (x * y + z * w), 1 - 2 * (x * x + z * z), 2 * (y * z - x * w)],
            [2 * (x * z - y * w), 2 * (y * z + x * w), 1 - 2 * (x * x + y * y)],
        ]
        col_sums = [sum(abs(m[i][j]) for i in range(3)) for j in range(3)]
        if any(abs(s - 1.0) > 1e-3 for s in col_sums):
            out_path.unlink()
            fail(4, f"node {node.get('name')} rotation not axis-aligned: {r}")
        rmn, rmx = [1e9] * 3, [-1e9] * 3
        for cx in (mn[0], mx[0]):
            for cy in (mn[1], mx[1]):
                for cz in (mn[2], mx[2]):
                    c = (cx, cy, cz)
                    for i in range(3):
                        v = sum(m[i][j] * c[j] for j in range(3))
                        rmn[i] = min(rmn[i], v)
                        rmx[i] = max(rmx[i], v)
        t = node.get("translation")
        if t:
            rmn = [rmn[i] + t[i] for i in range(3)]
            rmx = [rmx[i] + t[i] for i in range(3)]
        return rmn, rmx

    umin = [1e9] * 3
    umax = [-1e9] * 3
    for part in ("body", "string"):
        mn, mx = acc_bbox(named[part])
        for i in range(3):
            umin[i] = min(umin[i], mn[i])
            umax[i] = max(umax[i], mx[i])
    if not (close(umin, list(V2_BBOX_WORLD[0]), ACC_TOL) and close(umax, list(V2_BBOX_WORLD[1]), ACC_TOL)):
        out_path.unlink()
        fail(4, f"union bbox drift: [{umin},{umax}] want {V2_BBOX_WORLD}")

    smn, smx = acc_bbox(named["string"])
    if not (close(smn, list(STRING_BBOX_WORLD[0]), 5e-3) and close(smx, list(STRING_BBOX_WORLD[1]), 5e-3)):
        out_path.unlink()
        fail(4, f"string bbox drift: [{smn},{smx}] want {STRING_BBOX_WORLD}")

    total = sum(doc["accessors"][p["indices"]]["count"]
                for m in doc["meshes"] for p in m["primitives"] if "indices" in p)
    if total != V2_TOTAL_INDICES:
        out_path.unlink()
        fail(4, f"index conservation: {total} != {V2_TOTAL_INDICES}")

    print(f"OK: {out_path} body+string, indices={total}, bbox pinned")


main()
