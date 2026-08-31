# Blender headless animation preview strips (card AC-2).
#
# Renders every animation clip of a model as a horizontal sprite strip so the
# static catalog page can loop previews without WebGL / server (file:// safe).
#
# Usage:
#   blender -b -P anim_strip.py -- --model <glb> --meta-dir <dir-with-meta.json> \
#           [--frames 10] [--res 256]
#
# Output:
#   <meta-dir>/anim/<clip>.png        horizontal strip (frames * res, res)
#   <meta-dir>/anim_index.json        {source, clips:[{name, strip, frames, frame_w, frame_h}]}
#
# Everything runs inside Blender's bundled Python (numpy included).
import argparse
import json
import math
import re
import sys
from pathlib import Path

import bpy
from mathutils import Vector

AZIMUTH_DEG = 45.0   # same first view as turntable.py
ELEVATION_DEG = 20.0
LENS_MM = 50.0
MAX_CLIPS = 12       # safety cap per model
KEEP_TYPES = {"MESH", "ARMATURE"}


def parse_args():
    argv = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []
    p = argparse.ArgumentParser(description="Render per-clip sprite strips")
    p.add_argument("--model", required=True)
    p.add_argument("--meta-dir", required=True, help="dir containing meta.json; strips go to <dir>/anim/")
    p.add_argument("--frames", type=int, default=10)
    p.add_argument("--res", type=int, default=256)
    return p.parse_args(argv)


def pick_engine():
    engines = bpy.types.RenderSettings.bl_rna.properties["engine"].enum_items
    available = {e.identifier for e in engines}
    for candidate in ("BLENDER_EEVEE_NEXT", "BLENDER_EEVEE", "CYCLES"):
        if candidate in available:
            bpy.context.scene.render.engine = candidate
            if candidate == "CYCLES":
                bpy.context.scene.cycles.samples = 16
            return candidate
    raise RuntimeError("no usable render engine found")


def reset_scene():
    bpy.ops.wm.read_factory_settings(use_empty=True)


def import_glb(filepath):
    names_before = {o.name for o in bpy.data.objects}
    bpy.ops.import_scene.gltf(filepath=str(filepath))
    keep = []
    for name in [o.name for o in bpy.data.objects if o.name not in names_before]:
        o = bpy.data.objects.get(name)
        if o is None:
            continue
        if o.type in KEEP_TYPES:
            keep.append(name)
        else:
            bpy.data.objects.remove(o, do_unlink=True)
    return [bpy.data.objects[n] for n in keep if n in bpy.data.objects]


def world_bbox(objects):
    deps = bpy.context.evaluated_depsgraph_get()
    pts = []
    for o in objects:
        if o.type != "MESH":
            continue
        oe = o.evaluated_get(deps)
        pts.extend(oe.matrix_world @ Vector(c) for c in oe.bound_box)
    if not pts:
        pts = [o.matrix_world.translation for o in objects]
    lo = Vector((min(p.x for p in pts), min(p.y for p in pts), min(p.z for p in pts)))
    hi = Vector((max(p.x for p in pts), max(p.y for p in pts), max(p.z for p in pts)))
    return lo, hi


def setup_look(center, radius):
    scene = bpy.context.scene
    world = bpy.data.worlds.new("World")
    scene.world = world
    world.use_nodes = True
    bg = world.node_tree.nodes["Background"]
    bg.inputs[0].default_value = (0.85, 0.85, 0.85, 1.0)
    bg.inputs[1].default_value = 1.0

    # key sun + fill area, mirroring turntable.py's intent
    sun = bpy.data.lights.new("sun", type="SUN")
    sun.energy = 3.0
    so = bpy.data.objects.new("sun", sun)
    scene.collection.objects.link(so)
    so.rotation_euler = (math.radians(50), 0, math.radians(30))

    area = bpy.data.lights.new("fill", type="AREA")
    area.energy = 400.0 * max(radius, 0.5)
    area.size = max(radius * 2.5, 0.5)
    ao = bpy.data.objects.new("fill", area)
    scene.collection.objects.link(ao)
    ao.location = (center.x + radius * 1.5, center.y - radius * 1.5, center.z + radius * 1.8)
    # aim at center
    d = center - ao.location
    ao.rotation_euler = d.to_track_quat("-Z", "Y").to_euler()

    cam = bpy.data.cameras.new("cam")
    cam.lens = LENS_MM
    co = bpy.data.objects.new("cam", cam)
    scene.collection.objects.link(co)
    az, el = math.radians(AZIMUTH_DEG), math.radians(ELEVATION_DEG)
    dist = radius * 2.6 + 0.1
    co.location = (center.x + dist * math.cos(el) * math.sin(az),
                   center.y - dist * math.cos(el) * math.cos(az),
                   center.z + dist * math.sin(el))
    d = center - co.location
    co.rotation_euler = d.to_track_quat("-Z", "Y").to_euler()
    scene.camera = co
    return co


def set_action(objects, act):
    """Assign the action to the (skeletal) objects so frame_set() plays it.
    Handles both legacy and slotted (4.4+) actions."""
    for o in objects:
        if o.type not in {"ARMATURE", "MESH"}:
            continue
        ad = o.animation_data_create()
        ad.action = act
        if hasattr(ad, "action_slot"):
            slots = list(getattr(act, "slots", []) or [])
            if slots:
                try:
                    ad.action_slot = slots[0]
                except Exception:
                    pass  # best-effort; legacy single-action path needs no slot


def render_frames(objects, actions, out_dir, res, n_frames):
    scene = bpy.context.scene
    scene.render.resolution_x = res
    scene.render.resolution_y = res
    scene.render.resolution_percentage = 100
    scene.render.image_settings.file_format = "PNG"

    strips = []
    for act in actions[:MAX_CLIPS]:
        fs, fe = act.frame_range
        span = max(int(round(fe - fs)) - 1, 0)
        if span <= 0:
            continue
        n = max(2, min(n_frames, span + 1))
        frame_ids = [int(round(fs + (span) * i / (n - 1))) for i in range(n)]
        safe = re.sub(r"[^A-Za-z0-9_-]+", "_", act.name) or f"clip{len(strips)}"
        tmp_dir = out_dir / f"_tmp_{safe}"
        tmp_dir.mkdir(parents=True, exist_ok=True)

        set_action(objects, act)
        files = []
        for k, fr in enumerate(frame_ids):
            scene.frame_set(fr)
            scene.render.filepath = str(tmp_dir / f"f{k:03d}.png")
            bpy.ops.render.render(write_still=True)
            files.append(tmp_dir / f"f{k:03d}.png")

        # assemble horizontal strip via numpy (bundled with Blender)
        import numpy as np
        imgs = [bpy.data.images.load(str(f)) for f in files]
        w, h = imgs[0].size
        npx = w * h * 4
        buf = np.empty(len(imgs) * npx, dtype=np.float32)
        for i, im in enumerate(imgs):
            im.pixels.foreach_get(buf[i * npx:(i + 1) * npx])
        strip = bpy.data.images.new(f"strip_{safe}", width=w * len(imgs), height=h, alpha=True)
        strip.pixels.foreach_set(buf)
        strip.filepath_raw = str(out_dir / f"{safe}.png")
        strip.file_format = "PNG"
        strip.save()

        for im in imgs:
            bpy.data.images.remove(im)
        bpy.data.images.remove(strip)
        for f in files:
            f.unlink(missing_ok=True)
        tmp_dir.rmdir()

        strips.append({"name": act.name, "strip": f"anim/{safe}.png",
                       "frames": n, "frame_w": w, "frame_h": h})
        print(f"[anim_strip] {act.name}: {n} frames -> {safe}.png")
    return strips


def main():
    args = parse_args()
    model = Path(args.model)
    meta_dir = Path(args.meta_dir)
    out_dir = meta_dir / "anim"
    out_dir.mkdir(parents=True, exist_ok=True)

    reset_scene()
    objects = import_glb(str(model))
    if not objects:
        print("[anim_strip] ERROR: nothing imported", file=sys.stderr)
        sys.exit(3)
    lo, hi = world_bbox(objects)
    center = (lo + hi) / 2
    radius = max((hi - lo).length / 2, 0.1)
    setup_look(center, radius)
    engine = pick_engine()

    actions = sorted(bpy.data.actions, key=lambda a: a.name)
    if not actions:
        print("[anim_strip] no animation clips in this model")
    strips = render_frames(objects, actions, out_dir, args.res, args.frames)

    index = {
        "source": model.name,
        "engine": engine,
        "resolution": args.res,
        "clips": strips,
    }
    (out_dir / "anim_index.json").write_text(
        json.dumps(index, indent=2, ensure_ascii=False), encoding="utf-8")
    print(f"[anim_strip] done: {len(strips)} clip(s) -> {out_dir}")


main()
