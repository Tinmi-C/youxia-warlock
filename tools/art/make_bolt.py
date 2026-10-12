# ART10 ballista bolt authoring (AI-surgeon discipline, five rules): a
# minimal four-sided bolt the string launches — shaft + pyramidal tip along
# ±X (the ballista's pinned long axis), origin at shaft center so it can be
# parented straight onto the string node's centered pivot. No textures, one
# flat material; geometry only, ~0.30 long.
#
# Usage:
#   blender -b -P make_bolt.py -- --out bolt.glb
#   blender -b -P make_bolt.py -- --out bolt.glb --dry-run
#
# Exit codes: 0 ok; 3 bad args; 4 self-check failure.

import argparse
import sys

import bpy
from mathutils import Vector

SHAFT = 0.26          # square shaft length along X
TIP = 0.04            # pyramidal tip (points -X, the muzzle direction)
DIAM = 0.014          # square cross-section side
TIP_COLOR = (0.16, 0.15, 0.14, 1.0)  # dark iron


def fail(code, msg):
    print(f"FAIL[{code}]: {msg}", file=sys.stderr)
    sys.exit(code)


def parse_args():
    argv = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []
    p = argparse.ArgumentParser(description="author the ballista bolt GLB")
    p.add_argument("--out", required=True)
    p.add_argument("--dry-run", action="store_true")
    return p.parse_args(argv)


def main():
    args = parse_args()
    total = SHAFT + TIP
    print("plan:")
    print(f"  bolt  : shaft {SHAFT} + tip {TIP} = {total} long along ±X")
    print(f"  cross : square {DIAM}x{DIAM}, origin at shaft center")
    print(f"  nodes : single 'bolt' mesh node, one flat material")

    if args.dry_run:
        print("dry-run: nothing written")
        return

    bpy.ops.wm.read_factory_settings(use_empty=True)

    import bmesh
    mesh = bpy.data.meshes.new("bolt")
    bm = bmesh.new()
    hx, hy = SHAFT / 2.0, DIAM / 2.0
    # Tip at -X (the muzzle direction); nock (flat butt) at +X where the
    # string pushes. Built on [-hx-TIP, +hx], then centered on the span.
    x_butt = hx
    x_tipbase = -hx
    x_point = -(hx + TIP)
    shift = TIP / 2.0

    def vert(x, s, t):
        return bm.verts.new((x + shift, s * hy, t * hy))

    # rings: butt (x_butt), tipbase (x_tipbase); corners ordered as a ring
    corners = [(+1, +1), (+1, -1), (-1, -1), (-1, +1)]
    butt = [vert(x_butt, s, t) for s, t in corners]
    base = [vert(x_tipbase, s, t) for s, t in corners]
    point = bm.verts.new((x_point + shift, 0.0, 0.0))

    # shaft sides (outward winding)
    for i in range(4):
        j = (i + 1) % 4
        bm.faces.new((butt[i], butt[j], base[j], base[i]))
    # butt cap
    bm.faces.new((butt[3], butt[2], butt[1], butt[0]))
    # tip frustum sides + point
    for i in range(4):
        j = (i + 1) % 4
        bm.faces.new((base[i], base[j], point))
    bm.normal_update()
    bm.to_mesh(mesh)
    bm.free()

    mat = bpy.data.materials.new("iron")
    mat.use_nodes = True
    bsdf = next(n for n in mat.node_tree.nodes if n.type == "BSDF_PRINCIPLED")
    bsdf.inputs["Base Color"].default_value = TIP_COLOR
    bsdf.inputs["Metallic"].default_value = 0.7
    bsdf.inputs["Roughness"].default_value = 0.45
    mesh.materials.append(mat)

    obj = bpy.data.objects.new("bolt", mesh)
    bpy.context.scene.collection.objects.link(obj)

    # Self-check before export: dims + centered origin.
    xs = [v.co.x for v in mesh.vertices]
    if abs((max(xs) - min(xs)) - total) > 1e-4:
        fail(4, f"length {max(xs)-min(xs)} != {total}")
    if abs(max(xs) + min(xs)) > 1e-4:
        fail(4, "origin not centered on the shaft")

    bpy.ops.export_scene.gltf(filepath=args.out, export_format="GLB",
                              export_apply=False, export_animations=False)
    print(f"OK: {args.out} bolt {total} long")


main()
