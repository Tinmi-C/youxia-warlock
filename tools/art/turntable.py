# Blender headless turntable renderer for glTF assets (card 17, tool layer).
#
# Usage:
#   blender -b -P turntable.py -- --in <file-or-dir> --out <dir> [--resolution 512]
#
# For each .glb found in the input:
#   - renders 4 azimuth views with standardized lighting into <out>/<name>/
#   - writes <out>/<name>/meta.json with the numbers downstream consumers need:
#     bbox/height (-> hit radius candidate for enemies.ron), triangle count,
#     material list (flat-color check), animation clip list (Mixamo gap check).
#
# Everything runs inside Blender's bundled Python; no system Python required.
import argparse
import json
import math
import sys
from pathlib import Path

import bpy
from mathutils import Vector

AZIMUTHS = (45.0, 135.0, 225.0, 315.0)  # degrees around Z (Blender up-axis)
ELEVATION_DEG = 20.0
LENS_MM = 50.0
KEEP_TYPES = {"MESH", "ARMATURE"}  # drop cameras/lights/empties shipped inside glTF


def parse_args():
    argv = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []
    parser = argparse.ArgumentParser(description="Turntable gallery renderer for glTF")
    parser.add_argument("--in", dest="input", required=True, help="glb file or directory")
    parser.add_argument("--out", dest="out", required=True, help="output directory")
    parser.add_argument("--resolution", type=int, default=512)
    return parser.parse_args(argv)


def pick_engine(scene):
    # EEVEE was renamed across Blender versions; fall back gracefully.
    engines = bpy.types.RenderSettings.bl_rna.properties["engine"].enum_items
    available = {e.identifier for e in engines}
    for candidate in ("BLENDER_EEVEE_NEXT", "BLENDER_EEVEE", "CYCLES"):
        if candidate in available:
            scene.render.engine = candidate
            if candidate == "CYCLES":
                scene.cycles.samples = 32
            return candidate
    raise RuntimeError("no usable render engine found")


def reset_scene():
    bpy.ops.wm.read_factory_settings(use_empty=True)


def import_glb(filepath):
    # Track by NAME, not by object reference: scene resets invalidate old
    # StructRNA wrappers and touching them raises ReferenceError.
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


def count_triangles(objects):
    deps = bpy.context.evaluated_depsgraph_get()
    total = 0
    for o in objects:
        if o.type != "MESH":
            continue
        mesh = o.evaluated_get(deps).to_mesh()
        total += sum(max(len(p.vertices) - 2, 0) for p in mesh.polygons)
        o.evaluated_get(deps).to_mesh_clear()
    return total


def setup_world():
    world = bpy.data.worlds.new("TurntableWorld")
    bpy.context.scene.world = world
    world.use_nodes = True
    bg = world.node_tree.nodes["Background"]
    bg.inputs[0].default_value = (0.85, 0.85, 0.85, 1.0)  # neutral gray backdrop
    bg.inputs[1].default_value = 1.0


def add_lights(center, size):
    sun = bpy.data.objects.new("KeySun", bpy.data.lights.new("KeySun", type="SUN"))
    sun.data.energy = 3.0
    sun.rotation_euler = (math.radians(50), 0.0, math.radians(35))
    bpy.context.collection.objects.link(sun)

    fill = bpy.data.objects.new("FillArea", bpy.data.lights.new("FillArea", type="AREA"))
    fill.data.energy = 300.0
    fill.data.size = max(size, 1.0)
    fill.location = center + Vector((-2.0, -2.5, 2.0))
    bpy.context.collection.objects.link(fill)


def render_views(out_dir, name, center, radius, resolution):
    scene = bpy.context.scene
    cam = bpy.data.objects.new("TurntableCam", bpy.data.cameras.new("TurntableCam"))
    cam.data.lens = LENS_MM
    bpy.context.collection.objects.link(cam)
    scene.camera = cam

    dist = radius / math.tan(cam.data.angle / 2) * 1.15
    paths = []
    for i, az in enumerate(AZIMUTHS):
        az_r, el_r = math.radians(az), math.radians(ELEVATION_DEG)
        offset = Vector((
            dist * math.cos(el_r) * math.cos(az_r),
            dist * math.cos(el_r) * math.sin(az_r),
            dist * math.sin(el_r),
        ))
        cam.location = center + offset
        cam.rotation_euler = (center - cam.location).to_track_quat("-Z", "Y").to_euler()
        target = out_dir / f"{name}_{i}_{int(az)}deg.png"
        scene.render.filepath = str(target)
        bpy.ops.render.render(write_still=True)
        paths.append(target.name)
    return paths


def purge_helper_meshes(objects):
    """Same rule as normalize.py: with a rigged character present, free-floating
    helper meshes (Quaternius ships a 2m Icosphere) would poison the bbox and
    the renders. Drop them, by name, to avoid stale StructRNA references."""
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
        return objects
    removed = []
    for n in names:
        o = bpy.data.objects.get(n)
        if o is None:
            continue
        if o.type == "MESH" and not armature_ancestor(o):
            removed.append(n)
            bpy.data.objects.remove(o, do_unlink=True)
    if removed:
        print(f"[turntable] dropped helper meshes: {removed}")
    return [bpy.data.objects[n] for n in names if n in bpy.data.objects]


def collect_meta(filepath, objects, images):
    deps = bpy.context.evaluated_depsgraph_get()
    lo, hi = world_bbox(objects)
    dims = hi - lo
    footprint = max(dims.x, dims.y) / 2  # horizontal play area (XZ in glTF = XY here)
    materials = sorted({m.name for o in objects if o.type == "MESH"
                        for m in o.data.materials if m})
    clips = [{"name": a.name,
              "frame_start": float(a.frame_range[0]),
              "frame_end": float(a.frame_range[1])}
             for a in bpy.data.actions]
    return {
        "source": filepath.name,
        "bbox_min_m": [round(v, 4) for v in lo],
        "bbox_max_m": [round(v, 4) for v in hi],
        # Blender is Z-up; the glTF importer converts Y-up -> Z-up, so height = Z extent.
        "height_m": round(dims.z, 4),
        "size_m": {"x": round(dims.x, 4), "y": round(dims.y, 4), "z": round(dims.z, 4)},
        "footprint_radius_m": round(footprint, 4),  # -> hit_radius candidate for enemies.ron
        "triangles": count_triangles(objects),
        "material_count": len(materials),
        "material_names": materials,
        "has_armature": any(o.type == "ARMATURE" for o in objects),
        "animation_clips": clips,
        "images": images,
    }


def main():
    args = parse_args()
    src = Path(args.input).resolve()
    files = sorted([*src.glob("*.glb"), *src.glob("*.gltf")]) if src.is_dir() else [src]
    if not files:
        print(f"[turntable] no .glb found under {src}", file=sys.stderr)
        sys.exit(2)

    out_root = Path(args.out).resolve()
    out_root.mkdir(parents=True, exist_ok=True)
    report = []

    for f in files:
        reset_scene()
        scene = bpy.context.scene
        scene.render.resolution_x = args.resolution
        scene.render.resolution_y = args.resolution
        engine = pick_engine(scene)
        setup_world()

        objects = import_glb(f)
        objects = purge_helper_meshes(objects)
        if not objects:
            print(f"[turntable] {f.name}: nothing importable, skipped", file=sys.stderr)
            continue

        lo, hi = world_bbox(objects)
        center = (lo + hi) / 2
        radius = max((hi - lo).length / 2, 0.1)
        add_lights(center, radius)

        target_dir = out_root / f.stem
        target_dir.mkdir(parents=True, exist_ok=True)
        images = render_views(target_dir, f.stem, center, radius, args.resolution)

        meta = collect_meta(f, objects, images)
        meta["render_engine"] = engine
        (target_dir / "meta.json").write_text(
            json.dumps(meta, indent=2, ensure_ascii=False), encoding="utf-8")
        report.append(meta)
        print(f"[turntable] {f.name}: height={meta['height_m']}m "
              f"tris={meta['triangles']} clips={len(meta['animation_clips'])} "
              f"materials={meta['material_count']} -> {target_dir}")

    (out_root / "gallery_summary.json").write_text(
        json.dumps(report, indent=2, ensure_ascii=False), encoding="utf-8")
    print(f"[turntable] done: {len(report)} model(s) -> {out_root}")


main()
