# Blender headless asset wash (card 17). Normalizes a raw glTF/GLB into a
# bible-compliant single .glb:
#   - uniform target height (scale applied to scene roots)
#   - origin moved to feet center (bbox min-Z = 0, centered on X/Y)
#   - Y-up glTF export (Blender handles the Z-up <-> Y-up conversion)
#   - animation clips renamed to pipeline conventions: idle/walk/attack/death/hit
# Usage:
#   blender -b -P normalize.py -- --in <raw.gltf|raw.glb> --out <clean.glb> --height <meters>
import argparse
import json
import struct
import sys
from pathlib import Path

import bpy
from mathutils import Vector

# substring (lowercased) -> pipeline clip name; first match wins
CLIP_MAP = [
    ("bite", "attack"),
    ("idle", "idle"),
    ("walk", "walk"),
    ("death", "death"),
    ("hit", "hit"),
]
KEEP_TYPES = {"MESH", "ARMATURE", "EMPTY"}  # preserve rig hierarchy


def parse_args():
    argv = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []
    parser = argparse.ArgumentParser(description="Normalize raw glTF into pipeline glb")
    parser.add_argument("--in", dest="input", required=True)
    parser.add_argument("--out", dest="out", required=True)
    parser.add_argument("--height", type=float, required=True, help="target height in meters")
    parser.add_argument("--max-tris", type=int, default=0,
                        help="decimate meshes to fit this triangle budget (0 = keep)")
    parser.add_argument("--tex-size", type=int, default=0,
                        help="downscale embedded textures to this pixel size per side (0 = keep)")
    return parser.parse_args(argv)


def reset_scene():
    bpy.ops.wm.read_factory_settings(use_empty=True)


def import_model(filepath):
    # Track by NAME: stale StructRNA wrappers raise ReferenceError after removals.
    names_before = {o.name for o in bpy.data.objects}
    bpy.ops.import_scene.gltf(filepath=str(filepath))
    keep_names = []
    for name in [o.name for o in bpy.data.objects if o.name not in names_before]:
        o = bpy.data.objects.get(name)
        if o is None:
            continue
        if o.type in KEEP_TYPES:
            keep_names.append(name)
        else:
            bpy.data.objects.remove(o, do_unlink=True)
    return [bpy.data.objects[n] for n in keep_names if n in bpy.data.objects]


def world_bbox(objects):
    points = []
    deps = bpy.context.evaluated_depsgraph_get()
    for o in objects:
        if o.type != "MESH":
            continue
        oe = o.evaluated_get(deps)
        points.extend(oe.matrix_world @ Vector(c) for c in oe.bound_box)
    if not points:
        points = [o.matrix_world.translation for o in objects]
    lo = Vector((min(p.x for p in points), min(p.y for p in points), min(p.z for p in points)))
    hi = Vector((max(p.x for p in points), max(p.y for p in points), max(p.z for p in points)))
    return lo, hi


def rename_clips():
    renamed = []
    for action in bpy.data.actions:
        low = action.name.lower()
        for key, target in CLIP_MAP:
            if key in low:
                if action.name != target:
                    action.name = target
                    renamed.append(target)
                break
    return renamed


def purge_helper_meshes(objects):
    """Quaternius-style packs hide free-floating helper meshes (e.g. a 2m
    Icosphere at the world origin). If the model has skinned meshes, drop
    every mesh OUTSIDE the armature hierarchy so the wash only sees the
    character itself. Operates on names to avoid stale StructRNA refs."""
    def armature_ancestor(o):
        p = o.parent
        while p is not None:
            if p.type == "ARMATURE":
                return True
            p = p.parent
        return False

    names = [o.name for o in objects]
    skinned = [n for n in names
               if bpy.data.objects[n].type == "MESH" and armature_ancestor(bpy.data.objects[n])]
    if not skinned:
        return objects  # no rig -> nothing to judge helpers by, keep as-is
    removed = []
    for n in names:
        o = bpy.data.objects.get(n)
        if o is None:
            continue
        if o.type == "MESH" and not armature_ancestor(o):
            removed.append(n)
            bpy.data.objects.remove(o, do_unlink=True)
    print(f"[normalize] dropped helper meshes: {removed}")
    return [bpy.data.objects[n] for n in names if n in bpy.data.objects]


def clean_raw_gltf(src: Path) -> Path:
    """Strip helper meshes at the JSON level BEFORE importing.

    Quaternius packs ship one free-floating Icosphere per character. Removing
    the object inside Blender is not enough: the 5.x glTF exporter resurrects
    it into the exported file. Stripping the mesh reference from the source
    JSON (node shell kept, indices stay stable) kills it at the source."""
    if src.suffix.lower() != ".gltf":
        return src
    doc = json.loads(src.read_text(encoding="utf-8"))
    nodes = doc.get("nodes", [])
    joints = set()
    for skin in doc.get("skins", []):
        joints.update(skin.get("joints", []))
    parented = {c for n in nodes for c in n.get("children", [])}
    stripped = []
    for i, node in enumerate(nodes):
        if ("mesh" in node and "skin" not in node
                and i not in joints and i not in parented):
            node.pop("mesh", None)
            stripped.append(node.get("name", f"node#{i}"))
    if not stripped:
        return src
    out = src.with_name(src.stem + ".clean.gltf")
    out.write_text(json.dumps(doc), encoding="utf-8")
    print(f"[normalize] stripped helper mesh nodes from JSON: {stripped} -> {out.name}")
    return out


def read_glb_json(path: Path):
    """Parse the JSON chunk of a binary glTF container."""
    data = path.read_bytes()
    _, _, total = struct.unpack_from("<III", data, 0)
    off = 12
    while off < total:
        clen, ctype = struct.unpack_from("<II", data, off)
        payload = data[off + 8: off + 8 + clen]
        if ctype == 0x4E4F534A:  # 'JSON'
            return json.loads(payload.decode("utf-8"))
        off += 8 + clen
    raise ValueError(f"{path.name}: no JSON chunk found")


def strip_nodes_from_glb(path: Path, banned: set):
    """Post-export surgery: rewrite the glb's JSON chunk without banned nodes.
    Binary chunk is kept verbatim; orphaned accessors are harmless dead data."""
    data = path.read_bytes()
    magic, version, total = struct.unpack_from("<III", data, 0)
    json_chunk = b""
    bin_chunk = b""
    off = 12
    while off < total:
        clen, ctype = struct.unpack_from("<II", data, off)
        payload = data[off + 8: off + 8 + clen]
        if ctype == 0x4E4F534A:  # 'JSON'
            json_chunk = payload
        elif ctype == 0x004E4942:  # 'BIN'
            bin_chunk = payload
        off += 8 + clen
    doc = json.loads(json_chunk.decode("utf-8"))

    nodes = doc.get("nodes", [])
    banned_idx = {i for i, n in enumerate(nodes)
                  if n.get("name") in banned and "mesh" in n}
    if not banned_idx:
        return
    for i in banned_idx:
        nodes[i] = {}
    for scene in doc.get("scenes", []):
        scene["nodes"] = [i for i in scene.get("nodes", []) if i not in banned_idx]
    for n in nodes:
        if n.get("children"):
            n["children"] = [c for c in n["children"] if c not in banned_idx]

    out_json = json.dumps(doc).encode("utf-8")
    out_json += b" " * ((4 - len(out_json) % 4) % 4)
    bin_chunk += b"\x00" * ((4 - len(bin_chunk) % 4) % 4)
    total_len = 12 + 8 + len(out_json) + 8 + len(bin_chunk)
    out = struct.pack("<III", magic, version, total_len)
    out += struct.pack("<II", len(out_json), 0x4E4F534A) + out_json
    out += struct.pack("<II", len(bin_chunk), 0x004E4942) + bin_chunk
    path.write_bytes(out)
    print(f"[normalize] stripped helper nodes from exported glb: {sorted(banned)}")


def count_tris(objects):
    total = 0
    for o in objects:
        if o.type == "MESH":
            total += sum(len(p.vertices) - 2 for p in o.data.polygons)
    return total


def decimate(objects, max_tris):
    """Uniform collapse-decimate across all meshes to fit the triangle budget.
    Same ratio for every mesh keeps relative density; UVs survive collapse."""
    current = count_tris(objects)
    if current <= max_tris:
        return
    ratio = max_tris / current
    meshes = [o for o in objects if o.type == "MESH"]
    for o in meshes:
        bpy.ops.object.select_all(action="DESELECT")
        o.select_set(True)
        bpy.context.view_layer.objects.active = o
        mod = o.modifiers.new("wash_decimate", "DECIMATE")
        mod.ratio = ratio
        mod.use_collapse_triangulate = True
        bpy.ops.object.modifier_apply(modifier="wash_decimate")
    print(f"[normalize] decimated {current} -> {count_tris(meshes)} tris (ratio x{ratio:.3f})")


def downscale_textures(max_size):
    for img in bpy.data.images:
        w, h = img.size
        if w > max_size or h > max_size:
            k = max_size / max(w, h)
            nw, nh = max(1, int(w * k)), max(1, int(h * k))
            img.scale(nw, nh)
            print(f"[normalize] texture downscaled to {nw}x{nh}: {img.name}")


def main():
    args = parse_args()
    src = Path(args.input).resolve()
    dst = Path(args.out).resolve()
    dst.parent.mkdir(parents=True, exist_ok=True)

    reset_scene()
    src = clean_raw_gltf(src)
    objects = import_model(src)
    if not objects:
        print(f"[normalize] {src.name}: nothing importable", file=sys.stderr)
        sys.exit(2)
    imported_names = sorted(o.name for o in bpy.data.objects)
    objects = purge_helper_meshes(objects)

    # Hard sweep: nothing outside the kept set may survive to export, whatever
    # the remove() semantics say.
    keep_names = {o.name for o in objects}
    banned = [n for n in imported_names if n not in keep_names]
    for o in list(bpy.data.objects):
        if o.name not in keep_names:
            bpy.data.objects.remove(o, do_unlink=True)
    print("[normalize] scene before export:", sorted(o.name for o in bpy.data.objects))

    lo, hi = world_bbox(objects)
    raw_height = hi.z - lo.z
    scale = args.height / raw_height
    roots = [o for o in objects if o.parent is None]
    for r in roots:
        r.scale = r.scale * scale
    bpy.context.view_layer.update()

    lo, hi = world_bbox(objects)
    cx, cy = (lo.x + hi.x) / 2, (lo.y + hi.y) / 2
    for r in roots:
        r.location.x -= cx
        r.location.y -= cy
        r.location.z -= lo.z
    bpy.context.view_layer.update()

    renamed = rename_clips()

    # Budget passes: triangle budget first (decimate), then texture budget.
    if args.max_tris > 0:
        decimate(objects, args.max_tris)
    if args.tex_size > 0:
        downscale_textures(args.tex_size)

    # Bake transforms into object data. Skinned meshes are exported in armature
    # space, so parent-node scale/location would be dropped on export without this.
    bpy.ops.object.select_all(action="DESELECT")
    for o in objects:
        o.select_set(True)
    bpy.context.view_layer.objects.active = objects[0]
    bpy.ops.object.transform_apply(location=True, rotation=True, scale=True)

    bpy.ops.export_scene.gltf(filepath=str(dst), export_format="GLB", use_selection=True)
    if banned:
        strip_nodes_from_glb(dst, set(banned))

    # Self-check: inspect the exported glb's JSON chunk directly. Re-importing
    # into Blender is NOT a valid check: the 5.x glTF importer fabricates a
    # phantom Icosphere object for skinned meshes, which would false-fail.
    doc = read_glb_json(dst)
    node_names = [n.get("name", f"#{i}") for i, n in enumerate(doc.get("nodes", []))]
    scene_nodes = [node_names[i] for s in doc.get("scenes", []) for i in s.get("nodes", [])]
    leaked = sorted(set(node_names) & set(banned))
    print(f"[normalize] self-check nodes in {dst.name}: {sorted(node_names)}")
    print(f"[normalize] self-check scene roots: {sorted(scene_nodes)}; leaked={leaked}")
    if leaked:
        print(f"[normalize] ERROR: leaked helper objects {leaked}", file=sys.stderr)
        sys.exit(3)
    clips = sorted(a.name for a in bpy.data.actions)
    print(f"[normalize] {src.name}: {raw_height:.2f}m -> {args.height}m "
          f"(scale x{scale:.3f}), clips={clips}, renamed={renamed}, "
          f"out={dst} ({dst.stat().st_size // 1024} KB)")


main()
