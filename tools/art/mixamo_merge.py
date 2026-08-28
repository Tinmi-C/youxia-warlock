# Blender headless: merge Mixamo animation FBX files onto a rigged character.
# Inputs: rigged character FBX (with skin) + N animation FBXs (without skin),
# each animation file NAMED after its target clip (idle/walk/attack/hit/death).
# Output: single GLB with the rigged mesh + one clip per animation file.
#
# Usage:
#   blender -b -P mixamo_merge.py -- --rigged player_rigged.fbx --anims <dir> --out merged.glb
#
# The merged GLB is intentionally NOT game-ready yet: run normalize.py on it
# afterwards (Mixamo units are cm, so expect a ~100x oversize that --height fixes).
import argparse
import sys
from pathlib import Path

import bpy


def parse_args():
    argv = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []
    parser = argparse.ArgumentParser(description="Merge Mixamo anim FBXs into one rigged GLB")
    parser.add_argument("--rigged", required=True, help="rigged character FBX (with skin)")
    parser.add_argument("--anims", required=True, help="directory of animation FBXs named by clip")
    parser.add_argument("--out", required=True, help="output merged GLB path")
    parser.add_argument("--no-in-place", action="store_true",
                        help="keep Hips horizontal travel (root motion). Default "
                             "strips it: the game drives position, clips must be "
                             "in-place or the character slides off its collider.")
    return parser.parse_args(argv)


def import_fbx_names(path):
    """Import an FBX and return the names of newly created objects (name-keyed:
    stale StructRNA wrappers die on removals, names do not)."""
    before = {o.name for o in bpy.data.objects}
    bpy.ops.import_scene.fbx(filepath=str(path))
    return [n for n in bpy.data.objects.keys() if n not in before]


def drop_objects(names):
    for n in names:
        o = bpy.data.objects.get(n)
        if o is not None:
            bpy.data.objects.remove(o, do_unlink=True)


def fcurve_collections(act):
    """Fcurve collections of an action across Blender API generations: legacy
    Action.fcurves and the slotted Actions (4.4+: layers -> strips -> bags)."""
    if hasattr(act, "fcurves"):
        return [act.fcurves]
    colls = []
    for layer in act.layers:
        for strip in layer.strips:
            if strip.type != "KEYFRAME":
                continue
            for slot in act.slots:
                bag = strip.channelbag(slot)
                if bag is not None:
                    colls.append(bag.fcurves)
    return colls


def remove_fcurves(act, predicate):
    """Remove matching fcurves; returns the count (API-generation safe)."""
    removed = 0
    for coll in fcurve_collections(act):
        for fc in list(coll):
            if predicate(fc):
                coll.remove(fc)
                removed += 1
    return removed


def main():
    args = parse_args()
    rigged = Path(args.rigged).resolve()
    anim_dir = Path(args.anims).resolve()
    dst = Path(args.out).resolve()
    dst.parent.mkdir(parents=True, exist_ok=True)

    bpy.ops.wm.read_factory_settings(use_empty=True)

    # 1) Rigged character: keep armature + skinned mesh.
    names = import_fbx_names(rigged)
    arm_names = [n for n in names
                 if n in bpy.data.objects and bpy.data.objects[n].type == "ARMATURE"]
    if not arm_names:
        print(f"[mixamo_merge] ERROR: no armature found in {rigged.name}", file=sys.stderr)
        sys.exit(2)
    target = bpy.data.objects[arm_names[0]]

    # Drop the rig's own default action (a 1-frame T-pose stub) so it does not
    # pollute the exported clip list.
    if target.animation_data and target.animation_data.action:
        stub = target.animation_data.action
        target.animation_data.action = None
        if stub.users == 0:
            bpy.data.actions.remove(stub)

    # 2) Each animation FBX: steal its action (rename to file stem = clip name),
    #    then discard the temporary rig/mesh it came with.
    clips = []
    for f in sorted(anim_dir.glob("*.fbx")):
        if f.resolve() == rigged:
            continue
        anames = import_fbx_names(f)
        src_arm = next((bpy.data.objects[n] for n in anames
                        if n in bpy.data.objects and bpy.data.objects[n].type == "ARMATURE"), None)
        act = src_arm.animation_data.action if (src_arm and src_arm.animation_data) else None
        if act is None:
            print(f"[mixamo_merge] WARN: no action in {f.name}, skipped")
            drop_objects(anames)
            continue
        act.name = f.stem
        act.use_fake_user = True  # survive with no active user after rig cleanup
        clips.append(act.name)
        drop_objects(anames)
        print(f"[mixamo_merge] clip {act.name} frames="
              f"{act.frame_range[0]:.0f}..{act.frame_range[1]:.0f}")

    if not clips:
        print("[mixamo_merge] ERROR: no animations collected", file=sys.stderr)
        sys.exit(3)

    # Hard purge: nothing outside the collected clip set may survive to export
    # (Mixamo rigs ship a 1-frame T-pose stub action that would pollute the
    # clip list and, being first, hijack the active pose on re-import).
    keep = set(clips)
    for a in list(bpy.data.actions):
        if a.name not in keep:
            a.use_fake_user = False
            bpy.data.actions.remove(a)

    # In-place pass: Mixamo pack clips carry the actor's horizontal travel in
    # the Hips location channels (walk drifts ~0.8m/loop, attack lunges ~0.9m).
    # Mixamo bones are FBX Y-up: the Hips BONE points up, so bone-local Y
    # (array_index 1) is WORLD up (the bob) and X/Z (0, 2) are the horizontal
    # plane (the travel). Strip X/Z, keep the bob; leg animation is untouched.
    if not args.no_in_place:
        stripped = 0
        for name in clips:
            act = bpy.data.actions[name]
            stripped += remove_fcurves(
                act,
                lambda fc: ('pose.bones["mixamorig:Hips"].location' in fc.data_path
                            and fc.array_index in (0, 2)))
        print(f"[mixamo_merge] in-place: stripped {stripped} Hips X/Z channels")

    # 3) Glue actions onto the rigged armature (active + NLA stash) so the
    #    exporter sees them as per-action clips.
    if target.animation_data is None:
        target.animation_data_create()
    for name in clips:
        act = bpy.data.actions[name]
        track = target.animation_data.nla_tracks.new()
        track.name = name  # one track per clip: NLA_TRACKS mode maps 1:1 if needed
        strip = track.strips.new(name, int(act.frame_range[0]), act)
        strip.action_frame_start = act.frame_range[0]
        strip.action_frame_end = act.frame_range[1]
    target.animation_data.action = bpy.data.actions[clips[0]]

    # 4) Strip stray import baggage, export one GLB (one clip per action).
    for o in list(bpy.data.objects):
        if o.type in {"LIGHT", "CAMERA"}:
            bpy.data.objects.remove(o, do_unlink=True)

    bpy.ops.export_scene.gltf(filepath=str(dst), export_format="GLB",
                              export_animation_mode="ACTIONS",
                              export_force_sampling=True)
    print(f"[mixamo_merge] out={dst} ({dst.stat().st_size // 1024} KB) clips={sorted(clips)}")


main()
