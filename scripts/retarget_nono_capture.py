#!/usr/bin/env python3
"""Retarget a Rokoko capture FBX onto Nono_Rig and author release clips.

Run through Blender against the production file:

    Blender -b /Users/nico/Projects/Blendr/NonoSubProduction2.blend \
      --python scripts/retarget_nono_capture.py -- \
      --fbx /path/to/capture.fbx \
      --clip Idle:120:210:loop --clip Thumbs_Up:400:445:once \
      [--fps-scale 1.0] [--save]

Each --clip is Name:startFrame:endFrame:mode where mode is loop|once and the
frame range is measured on the retargeted take's own timeline. The take is
retargeted once through the Rokoko Studio Live add-on, then each clip is cut
from the bake, shifted to start at frame 1, cleaned of procedural-bone and
root-translation keys, optionally loop-blended, and stored with a fake user.
Requires the rokoko-studio-live add-on to be enabled.
"""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

import bpy

TAIL_BONES = {f"spine.{index:03d}" for index in range(55, 79)}
DYNAMIC_HAIR_ROOTS = ("spine.021", "spine.031", "spine.039", "spine.085", "spine.093")
BONE_PATH = re.compile(r'^pose\.bones\["([^"]+)"\]\.(.+)$')
LOOP_BLEND_FRAMES = 12


def parse_args() -> argparse.Namespace:
    argv = sys.argv[sys.argv.index("--") + 1 :] if "--" in sys.argv else []
    parser = argparse.ArgumentParser()
    parser.add_argument("--fbx", type=Path, required=True)
    parser.add_argument("--clip", action="append", required=True, help="Name:start:end:loop|once")
    parser.add_argument("--fps-scale", type=float, default=1.0, help="Multiply key times (0.5 remaps 60fps keys to 30fps)")
    parser.add_argument("--save", action="store_true", help="Save the production .blend after authoring")
    return parser.parse_args(argv)


def forbidden_bone_names(rig: bpy.types.Object) -> set[str]:
    names = set(TAIL_BONES)
    for root_name in DYNAMIC_HAIR_ROOTS:
        root = rig.data.bones.get(root_name)
        if root is None:
            continue
        pending = [root]
        while pending:
            bone = pending.pop()
            names.add(bone.name)
            pending.extend(bone.children)
    names.update(bone.name for bone in rig.data.bones if bone.name.lower().startswith("skirt_root"))
    return names


def import_capture(fbx: Path) -> bpy.types.Object:
    before = set(bpy.data.objects)
    bpy.ops.import_scene.fbx(filepath=str(fbx), automatic_bone_orientation=False)
    imported = [obj for obj in set(bpy.data.objects) - before if obj.type == "ARMATURE"]
    if len(imported) != 1:
        raise RuntimeError(f"Expected one armature in {fbx.name}, found {[obj.name for obj in imported]}")
    return imported[0]


def retarget(source: bpy.types.Object, target: bpy.types.Object) -> bpy.types.Action:
    scene = bpy.context.scene
    scene.rsl_retargeting_armature_source = source
    scene.rsl_retargeting_armature_target = target
    bpy.ops.rsl.build_bone_list()
    mapped = [(item.bone_name_source, item.bone_name_target) for item in scene.rsl_retargeting_bone_list]
    for source_bone, target_bone in mapped:
        print(f"NONO_RETARGET_MAP|{source_bone}|{target_bone or '(unmapped)'}")
    unmapped = [source_bone for source_bone, target_bone in mapped if not target_bone]
    if unmapped:
        print(f"NONO_RETARGET_UNMAPPED={','.join(unmapped)}")
    scene.rsl_retargeting_auto_scaling = True
    existing = set(bpy.data.actions)
    bpy.ops.rsl.retarget_animation()
    if not target.animation_data or not target.animation_data.action:
        created = [action for action in set(bpy.data.actions) - existing]
        if not created:
            raise RuntimeError("Retarget produced no action on Nono_Rig")
        return created[0]
    return target.animation_data.action


def scale_key_times(action: bpy.types.Action, factor: float) -> None:
    if abs(factor - 1.0) < 1e-9:
        return
    for curve in action.fcurves:
        for key in curve.keyframe_points:
            key.co.x *= factor
            key.handle_left.x *= factor
            key.handle_right.x *= factor
        curve.update()


def author_clip(bake: bpy.types.Action, rig: bpy.types.Object, name: str, start: float, end: float, loop: bool) -> bpy.types.Action:
    clip = bake.copy()
    clip.name = name
    forbidden = forbidden_bone_names(rig)
    for curve in list(clip.fcurves):
        drop = False
        if curve.data_path in {"location", "rotation_euler", "rotation_quaternion", "scale"}:
            drop = True  # object-level animation is never shipped
        match = BONE_PATH.match(curve.data_path)
        if match:
            bone_name, property_name = match.groups()
            if bone_name in forbidden:
                drop = True
            if bone_name == "spine" and property_name in {"location", "scale"}:
                drop = True
        if drop:
            clip.fcurves.remove(curve)
            continue
        keep = [key.co.x for key in curve.keyframe_points if start <= key.co.x <= end]
        if not keep:
            clip.fcurves.remove(curve)
            continue
        for key in reversed(list(curve.keyframe_points)):
            if key.co.x < start or key.co.x > end:
                curve.keyframe_points.remove(key)
        for key in curve.keyframe_points:
            key.co.x -= start - 1
            key.handle_left.x -= start - 1
            key.handle_right.x -= start - 1
        curve.update()
    if loop:
        blend_loop(clip)
    clip.use_fake_user = True
    return clip


def blend_loop(action: bpy.types.Action) -> None:
    """Ease each curve's tail back to its first value so the clip loops."""
    for curve in action.fcurves:
        points = curve.keyframe_points
        if len(points) < 3:
            continue
        first_value = points[0].co.y
        last_time = points[-1].co.x
        blend_start = last_time - LOOP_BLEND_FRAMES
        for key in points:
            if key.co.x <= blend_start:
                continue
            weight = (key.co.x - blend_start) / max(last_time - blend_start, 1e-6)
            smooth = weight * weight * (3 - 2 * weight)
            key.co.y = key.co.y * (1 - smooth) + first_value * smooth
        curve.update()


def main() -> None:
    args = parse_args()
    rig = bpy.data.objects.get("Nono_Rig")
    if not rig or rig.type != "ARMATURE":
        raise RuntimeError("Open the production file with Nono_Rig before retargeting")

    clips = []
    for spec in args.clip:
        parts = spec.split(":")
        if len(parts) != 4 or parts[3] not in {"loop", "once"}:
            raise RuntimeError(f"Bad --clip spec {spec!r}; expected Name:start:end:loop|once")
        clips.append((parts[0], float(parts[1]), float(parts[2]), parts[3] == "loop"))

    source = import_capture(args.fbx.expanduser().resolve())
    bake = retarget(source, rig)
    scale_key_times(bake, args.fps_scale)
    for name, start, end, loop in clips:
        stale = bpy.data.actions.get(name)
        if stale is not None:
            bpy.data.actions.remove(stale, do_unlink=True)
        clip = author_clip(bake, rig, name, start, end, loop)
        print(f"NONO_CLIP={clip.name} frames={clip.frame_range[0]:.0f}-{clip.frame_range[1]:.0f} fcurves={len(clip.fcurves)}")

    # Drop the raw bake and the imported capture skeleton.
    if rig.animation_data and rig.animation_data.action == bake:
        rig.animation_data.action = None
    bpy.data.actions.remove(bake, do_unlink=True)
    capture_action = source.animation_data.action if source.animation_data else None
    bpy.data.objects.remove(source, do_unlink=True)
    if capture_action is not None:
        bpy.data.actions.remove(capture_action, do_unlink=True)

    if args.save:
        bpy.ops.wm.save_mainfile()
        print(f"NONO_SAVED={bpy.data.filepath}")


if __name__ == "__main__":
    main()
