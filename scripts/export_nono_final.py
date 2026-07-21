#!/usr/bin/env python3
"""Validate Nico's authored actions and export the final Nono GLB.

This script is intentionally strict: it exports only the required actions
(Idle, Think, Neutral, Thumbs_Up) plus any present optional gesture actions,
samples them at 30 fps, and rejects curves that would fight the procedural
tails, long hair, or skirt-secondary runtime.
"""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

import bpy


REQUIRED_ACTIONS = {"Idle": (2.0, 10.0), "Think": (2.0, 8.0), "Neutral": (2.0, 10.0), "Thumbs_Up": (1.0, 4.0)}
# Exported and validated only when present; the app falls back gracefully.
OPTIONAL_ACTIONS = {
    "Point_User": (1.0, 4.0),
    "Point_Self": (1.0, 4.0),
    "Cheer": (0.5, 4.0),
    "Heart_Touch": (0.5, 4.0),
    "Surprised": (0.5, 4.0),
}
TAIL_BONES = {f"spine.{index:03d}" for index in range(55, 79)}
DYNAMIC_HAIR_ROOTS = {"spine.021", "spine.031", "spine.039", "spine.085", "spine.093"}
BONE_PATH = re.compile(r'^pose\.bones\["([^"]+)"\]\.(.+)$')


def parse_args() -> argparse.Namespace:
    argv = sys.argv[sys.argv.index("--") + 1 :] if "--" in sys.argv else []
    parser = argparse.ArgumentParser()
    parser.add_argument("--output", type=Path, required=True)
    return parser.parse_args(argv)


def action_curves(action: bpy.types.Action):
    # Suit imports currently create legacy f-curves. Retain a guarded fallback
    # for Blender's layered Action API so the validator fails clearly instead
    # of silently skipping a future action representation.
    if hasattr(action, "fcurves"):
        try:
            yield from action.fcurves
            return
        except RuntimeError:
            pass
    for layer in getattr(action, "layers", []):
        for strip in getattr(layer, "strips", []):
            for channelbag in getattr(strip, "channelbags", []):
                yield from getattr(channelbag, "fcurves", [])


def descendants(rig: bpy.types.Object, root_name: str) -> set[str]:
    root = rig.data.bones.get(root_name)
    if root is None:
        return set()
    result = {root.name}
    pending = [root]
    while pending:
        bone = pending.pop()
        for child in bone.children:
            if child.name not in result:
                result.add(child.name)
                pending.append(child)
    return result


def validate_actions(rig: bpy.types.Object) -> dict[str, bpy.types.Action]:
    actions = {name: bpy.data.actions.get(name) for name in REQUIRED_ACTIONS}
    missing = [name for name, action in actions.items() if action is None]
    if missing:
        raise RuntimeError(f"Missing final actions: {', '.join(missing)}")
    limits = dict(REQUIRED_ACTIONS)
    for name, bounds in OPTIONAL_ACTIONS.items():
        action = bpy.data.actions.get(name)
        if action is not None:
            actions[name] = action
            limits[name] = bounds
        else:
            print(f"NONO_OPTIONAL_ACTION_MISSING={name}")

    forbidden_bones = set(TAIL_BONES)
    for root in DYNAMIC_HAIR_ROOTS:
        forbidden_bones.update(descendants(rig, root))
    forbidden_bones.update(bone.name for bone in rig.data.bones if bone.name.lower().startswith("skirt_root"))

    scene_fps = 30
    for name, action in actions.items():
        start, end = action.frame_range
        duration = max(0.0, (end - start) / scene_fps)
        minimum, maximum = limits[name]
        if duration < minimum - 1 / scene_fps or duration > maximum + 1 / scene_fps:
            raise RuntimeError(f"{name} is {duration:.2f}s; expected {minimum:.0f}–{maximum:.0f}s at 30 fps")
        for curve in action_curves(action):
            if curve.data_path in {"location", "scale"}:
                raise RuntimeError(f"{name} contains object-root {curve.data_path} animation")
            match = BONE_PATH.match(curve.data_path)
            if not match:
                continue
            bone_name, property_name = match.groups()
            if bone_name in forbidden_bones:
                raise RuntimeError(f"{name} keys procedural bone {bone_name}")
            if bone_name == "spine" and property_name == "scale":
                raise RuntimeError(f"{name} contains unintended root-bone scale")
            if bone_name == "spine" and property_name == "location":
                # Hip sway is allowed, but only re-based small-amplitude motion:
                # a drifting root means the capture wasn't normalized.
                extremes = [abs(k.co.y) for k in curve.keyframe_points]
                if extremes and max(extremes) > 0.35:
                    raise RuntimeError(f"{name} root-bone location amplitude {max(extremes):.2f} exceeds 0.35")
    return actions


def main() -> None:
    args = parse_args()
    output = args.output.expanduser().resolve()
    output.parent.mkdir(parents=True, exist_ok=True)
    rig = bpy.data.objects.get("Nono_Rig")
    export = bpy.data.collections.get("NONO_EXPORT")
    if not rig or rig.type != "ARMATURE" or not export:
        raise RuntimeError("Open NonoSubProduction.blend with Nono_Rig and NONO_EXPORT before final export")
    actions = validate_actions(rig)

    # Remove non-release actions only from this background export process. The
    # production .blend is never saved by this script.
    for action in list(bpy.data.actions):
        if action.name not in actions:
            bpy.data.actions.remove(action, do_unlink=True)
    bpy.context.scene.render.fps = 30
    for obj in export.all_objects:
        obj.hide_render = False
        obj.hide_set(False)

    bpy.ops.export_scene.gltf(
        filepath=str(output),
        export_format="GLB",
        use_selection=False,
        collection="NONO_EXPORT",
        export_apply=True,
        export_yup=True,
        export_texcoords=True,
        export_normals=True,
        export_tangents=True,
        export_materials="EXPORT",
        export_skins=True,
        export_all_influences=False,
        export_influence_nb=4,
        export_animations=True,
        export_animation_mode="ACTIONS",
        export_merge_animation="ACTION",
        # Sampling bakes every bone into each clip, including the procedural
        # tail/hair chains; scripts/strip_nono_glb.mjs removes those channels
        # after export (ACTIONS mode exports nothing without sampling).
        export_force_sampling=True,
        export_frame_step=1,
        export_lights=False,
        export_cameras=False,
    )
    print(f"NONO_FINAL_GLB={output}")


if __name__ == "__main__":
    main()
