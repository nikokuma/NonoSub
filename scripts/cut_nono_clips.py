#!/usr/bin/env python3
"""Cut the 9 release clips out of the recorded Rokoko take on Nono's rig.

Run headless against the checkpoint (which is NEVER saved by this script):

    Blender -b NonoSubCheckpointFinal.blend --python scripts/cut_nono_clips.py -- \
      --output ~/Projects/Blendr/NonoClipLibrary.blend

The raw take is `Anim Arm Nono_Rig2` (60 fps, XYZ-euler keys, hip location in
absolute room space). Amplification parameters captured during the live mocap
session are stored on the rig object:
  - "nono_amp_neutral": per-bone neutral rotations ({mode:"E", order, value})
  - "nono_amp_gains":   per-bone rotation gains (shoulder 1.5 / upper_arm 1.6 / forearm 1.3)
  - "nono_amp_hip_neutral" / "nono_amp_hip_gain": hip re-basing

Per clip this script samples every 2nd raw frame (60 -> 30 fps), amplifies the
six arm bones as `neutral * (neutral^-1 * raw)^gain`, re-bases the hip, drops
procedural/tail/hair/skirt and object-level channels, loop-blends the looping
clips, and rebuilds LINEAR fcurves starting at frame 1.

CRITICAL LESSON (2026-07-21): the delta quaternion MUST be canonicalized to the
positive hemisphere (`if delta.w < 0: delta.negate()`) before exponentiation.
The raw take's wrapped euler channels (e.g. upper_arm.L X at -347..-360 deg)
otherwise convert to negative-hemisphere quaternions, and the gain amplifies
the 360-theta long-way rotation - which contorted Nono's left arm to ~179 deg
from neutral in every gesture clip of the first release. An acceptance check
below now fails the cut if any amplified bone strays implausibly far.

The cut actions are written to a small library .blend; import them with:
    scripts/prepare_nono_release.py -- --actions-from <library.blend>
"""

from __future__ import annotations

import argparse
import json
import math
import re
import sys
import traceback
from pathlib import Path

import bpy
from mathutils import Euler, Quaternion

RAW_ACTION = "Anim Arm Nono_Rig2"
RAW_STEP = 2  # 60fps take -> 30fps clips

# Raw-take frame ranges chosen by Nico (60 fps).
CLIP_RANGES = {
    "Idle": (4188, 4620),
    "Neutral": (4188, 4620),
    "Think": (15200, 15469),
    "Thumbs_Up": (22452, 22580),
    "Point_Self": (26959, 27140),
    "Point_User": (31952, 32128),
    "Cheer": (36235, 36312),
    "Surprised": (40165, 40241),
    "Heart_Touch": (44712, 44822),
}
LOOP_CLIPS = {"Idle", "Neutral", "Think"}
LOOP_BLEND_FRAMES = 10

TAIL_BONES = {f"spine.{index:03d}" for index in range(55, 79)}
DYNAMIC_HAIR_ROOTS = ("spine.021", "spine.031", "spine.039", "spine.085", "spine.093")
BONE_PATH = re.compile(r'^pose\.bones\["([^"]+)"\]\.(.+)$')

# Amplification taper: full gain below TAPER_START, fading to gain 1.0 (raw
# as-performed) at TAPER_END. Nico's extreme poses (cheer elbows ~138 deg) have
# no headroom - amplifying them folds the limb flat against itself.
TAPER_START_DEG = 60.0
TAPER_END_DEG = 130.0
# Sanity ceiling: honest output never exceeds the raw extreme (~145 deg); the
# 2026-07-21 hemisphere bug produced ~179 deg on every gesture clip.
MAX_AMPLIFIED_DELTA_DEG = 165.0


def parse_args() -> argparse.Namespace:
    argv = sys.argv[sys.argv.index("--") + 1 :] if "--" in sys.argv else []
    parser = argparse.ArgumentParser()
    parser.add_argument("--output", type=Path, required=True, help="Library .blend to write the cut actions into")
    return parser.parse_args(argv)


def forbidden_bones(rig: bpy.types.Object) -> set[str]:
    result = set(TAIL_BONES)
    for root_name in DYNAMIC_HAIR_ROOTS:
        root = rig.data.bones.get(root_name)
        if root is None:
            continue
        pending = [root]
        while pending:
            bone = pending.pop()
            result.add(bone.name)
            pending.extend(bone.children)
    result.update(bone.name for bone in rig.data.bones if bone.name.lower().startswith("skirt_root"))
    return result


def quat_pow(q: Quaternion, gain: float) -> Quaternion:
    # q is canonical (w >= 0), so .angle is the shortest rotation in [0, pi].
    if q.w > 0.9999999:
        return Quaternion()
    angle_deg = math.degrees(q.angle)
    taper = 1.0 - smoothstep((angle_deg - TAPER_START_DEG) / (TAPER_END_DEG - TAPER_START_DEG))
    effective_gain = 1.0 + (gain - 1.0) * taper
    return Quaternion(q.axis, q.angle * effective_gain)


def smoothstep(t: float) -> float:
    t = max(0.0, min(1.0, t))
    return t * t * (3.0 - 2.0 * t)


FINGER_BONE = re.compile(r"^(f_index|f_middle|f_ring|f_pinky|thumb|hand)\.")
# Rokoko finger/wrist solve occasionally flips a joint ~90-180 deg for a few
# frames and back (e.g. the whole right hand at Surprised f12-13). Detect the
# spike as an implausible per-frame rotation step and slerp across it.
QUAT_SPIKE_RAD = math.radians(55.0)
QUAT_SPIKE_MAX_SPAN = 8  # frames at 30fps (~0.27s)


def rotation_step(a: Quaternion, b: Quaternion) -> float:
    dot = min(1.0, abs(a.dot(b)))
    return 2.0 * math.acos(dot)


def quat_despike(series: list[Quaternion]) -> None:
    i = 1
    while i < len(series):
        if rotation_step(series[i - 1], series[i]) > QUAT_SPIKE_RAD:
            landing = None
            for j in range(i + 1, min(i + 1 + QUAT_SPIKE_MAX_SPAN, len(series))):
                if rotation_step(series[i - 1], series[j]) <= QUAT_SPIKE_RAD:
                    landing = j
                    break
            if landing is not None:
                for k in range(i, landing):
                    t = (k - i + 1) / (landing - i + 1)
                    series[k] = series[i - 1].slerp(series[landing], t)
                i = landing
            # else: sustained reorientation - keep as performed
        i += 1


def main() -> None:
    args = parse_args()
    output = args.output.expanduser().resolve()
    output.parent.mkdir(parents=True, exist_ok=True)
    if output == Path(bpy.data.filepath):
        raise RuntimeError("Refusing to overwrite the open source file")

    rig = bpy.data.objects.get("Nono_Rig2") or bpy.data.objects.get("Nono_Rig")
    raw = bpy.data.actions.get(RAW_ACTION)
    if rig is None or raw is None:
        raise RuntimeError(f"Need rig and raw take {RAW_ACTION!r} (open NonoSubCheckpointFinal.blend)")

    def as_dict(value):
        if isinstance(value, str):
            return json.loads(value)
        return value.to_dict() if hasattr(value, "to_dict") else dict(value)

    neutral_prop = as_dict(rig.get("nono_amp_neutral"))
    gains = as_dict(rig.get("nono_amp_gains"))
    hip_neutral_raw = rig.get("nono_amp_hip_neutral")
    hip_neutral = [float(v) for v in (json.loads(hip_neutral_raw) if isinstance(hip_neutral_raw, str) else list(hip_neutral_raw))]
    hip_gain = float(rig.get("nono_amp_hip_gain"))
    neutral_quats: dict[str, Quaternion] = {}
    for bone_name, entry in neutral_prop.items():
        entry = as_dict(entry)
        if entry.get("mode") != "E":
            raise RuntimeError(f"Unexpected neutral mode for {bone_name}: {entry.get('mode')}")
        neutral_quats[bone_name] = Euler(list(entry["value"]), entry.get("order", "XYZ")).to_quaternion()

    banned = forbidden_bones(rig)

    # Collect raw curves per (bone, property) -> {array_index: fcurve}
    channels: dict[tuple[str, str], dict[int, bpy.types.FCurve]] = {}
    for curve in raw.fcurves:
        match = BONE_PATH.match(curve.data_path)
        if not match:
            continue  # drop object-level transforms
        bone_name, prop = match.groups()
        if bone_name in banned or bone_name not in rig.pose.bones:
            continue
        channels.setdefault((bone_name, prop), {})[curve.array_index] = curve

    new_actions: list[bpy.types.Action] = []
    for clip_name, (start, end) in CLIP_RANGES.items():
        frame_count = (end - start) // RAW_STEP + 1
        # sampled[data_path][array_index] = [values per output frame]
        sampled: dict[str, dict[int, list[float]]] = {}
        max_delta_deg: dict[str, float] = {}

        # Pass 1: rotations through quaternion space. The raw Rokoko retarget
        # alternates between equivalent euler decompositions (which quaternions
        # normalize away) and occasionally flips fingers/wrists outright (which
        # quat_despike repairs below).
        quat_series: dict[str, list[Quaternion]] = {}
        for i in range(frame_count):
            frame = start + i * RAW_STEP
            for (bone_name, prop), curves in channels.items():
                if prop != "rotation_euler":
                    continue
                raw_euler = Euler(
                    (curves.get(0).evaluate(frame) if 0 in curves else 0.0,
                     curves.get(1).evaluate(frame) if 1 in curves else 0.0,
                     curves.get(2).evaluate(frame) if 2 in curves else 0.0),
                    "XYZ",
                )
                rotation = raw_euler.to_quaternion()
                if bone_name in gains:
                    neutral = neutral_quats[bone_name]
                    delta = neutral.inverted() @ rotation
                    if delta.w < 0.0:  # hemisphere canonicalization - THE fix
                        delta.negate()
                    rotation = neutral @ quat_pow(delta, float(gains[bone_name]))
                    check = neutral.inverted() @ rotation
                    if check.w < 0.0:
                        check.negate()
                    max_delta_deg[bone_name] = max(max_delta_deg.get(bone_name, 0.0), math.degrees(check.angle))
                series = quat_series.setdefault(bone_name, [])
                if series and series[-1].dot(rotation) < 0.0:
                    rotation.negate()
                series.append(rotation)

        for bone_name, worst in sorted(max_delta_deg.items()):
            if worst > MAX_AMPLIFIED_DELTA_DEG:
                raise RuntimeError(
                    f"{clip_name}: amplified {bone_name} reaches {worst:.1f} deg from neutral "
                    f"(> {MAX_AMPLIFIED_DELTA_DEG}); hemisphere bug or bad neutral - refusing to cut"
                )

        for bone_name, series in quat_series.items():
            if FINGER_BONE.match(bone_name):
                quat_despike(series)
            path = f'pose.bones["{bone_name}"].rotation_euler'
            values = sampled.setdefault(path, {})
            prev = None
            for rotation in series:
                out_euler = rotation.to_euler("XYZ", prev) if prev else rotation.to_euler("XYZ")
                prev = out_euler
                for axis in range(3):
                    values.setdefault(axis, []).append(out_euler[axis])

        # Pass 2: location/scale channels.
        for i in range(frame_count):
            frame = start + i * RAW_STEP
            for (bone_name, prop), curves in channels.items():
                if prop == "rotation_euler":
                    continue
                path = f'pose.bones["{bone_name}"].{prop}'
                values = sampled.setdefault(path, {})
                for axis, curve in curves.items():
                    value = curve.evaluate(frame)
                    if bone_name == "spine" and prop == "location":
                        value = (value - hip_neutral[axis]) * hip_gain
                    values.setdefault(axis, []).append(value)

        if clip_name in LOOP_CLIPS:
            blend = min(LOOP_BLEND_FRAMES, frame_count - 1)
            for values in sampled.values():
                for series in values.values():
                    first = series[0]
                    for k in range(blend):
                        index = frame_count - blend + k
                        t = smoothstep((k + 1) / blend)
                        series[index] = series[index] * (1.0 - t) + first * t

        stale = bpy.data.actions.get(clip_name)
        if stale is not None:
            bpy.data.actions.remove(stale, do_unlink=True)
        action = bpy.data.actions.new(clip_name)
        action.use_fake_user = True
        for path, values in sampled.items():
            for axis, series in values.items():
                curve = action.fcurves.new(data_path=path, index=axis)
                curve.keyframe_points.add(frame_count)
                for i, value in enumerate(series):
                    point = curve.keyframe_points[i]
                    point.co = (1 + i, value)
                    point.interpolation = "LINEAR"
                curve.update()
        new_actions.append(action)
        worst_summary = ", ".join(f"{b}:{d:.0f}" for b, d in sorted(max_delta_deg.items()))
        print(f"CUT {clip_name}: frames 1-{frame_count}, arm max deltas deg [{worst_summary}]")

    bpy.data.libraries.write(str(output), set(new_actions), fake_user=True, compress=True)
    print(f"NONO_CLIP_LIBRARY={output}")


if __name__ == "__main__":
    try:
        main()
    except Exception:
        traceback.print_exc()
        sys.exit(1)
