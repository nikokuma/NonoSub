#!/usr/bin/env python3
"""Render four quick visual-audit views from NonoSubProductionPrepared.blend."""

from __future__ import annotations

import argparse
import math
import sys
from pathlib import Path

import bpy
from mathutils import Vector


def parse_args() -> argparse.Namespace:
    argv = sys.argv[sys.argv.index("--") + 1 :] if "--" in sys.argv else []
    parser = argparse.ArgumentParser()
    parser.add_argument("--output-dir", type=Path, required=True)
    return parser.parse_args(argv)


def look_at(obj: bpy.types.Object, target: Vector) -> None:
    obj.rotation_euler = (target - obj.location).to_track_quat("-Z", "Y").to_euler()


def export_bounds(collection: bpy.types.Collection) -> tuple[Vector, Vector]:
    depsgraph = bpy.context.evaluated_depsgraph_get()
    low = Vector((math.inf, math.inf, math.inf))
    high = Vector((-math.inf, -math.inf, -math.inf))
    for obj in collection.all_objects:
        if obj.type != "MESH":
            continue
        evaluated = obj.evaluated_get(depsgraph)
        for corner in evaluated.bound_box:
            world = evaluated.matrix_world @ Vector(corner)
            low = Vector((min(low[i], world[i]) for i in range(3)))
            high = Vector((max(high[i], world[i]) for i in range(3)))
    return low, high


def add_area(name: str, location: tuple[float, float, float], energy: float, color: tuple[float, float, float], size: float, target: Vector) -> None:
    data = bpy.data.lights.new(name, "AREA")
    data.energy = energy
    data.color = color
    data.shape = "DISK"
    data.size = size
    obj = bpy.data.objects.new(name, data)
    bpy.context.scene.collection.objects.link(obj)
    obj.location = location
    look_at(obj, target)


def main() -> None:
    args = parse_args()
    output = args.output_dir.expanduser().resolve()
    output.mkdir(parents=True, exist_ok=True)
    export = bpy.data.collections.get("NONO_EXPORT")
    source = bpy.data.collections.get("SOURCE_ONLY")
    if not export:
        raise RuntimeError("NONO_EXPORT collection is missing")
    if source:
        source.hide_render = True
        source.hide_viewport = True
    export.hide_render = False
    export.hide_viewport = False
    for obj in export.all_objects:
        obj.hide_render = False
        obj.hide_set(False)

    low, high = export_bounds(export)
    center = (low + high) * 0.5
    height = max(high.z - low.z, 0.1)
    radius = height * 2.15

    scene = bpy.context.scene
    scene.render.engine = "BLENDER_EEVEE"
    scene.render.resolution_x = 700
    scene.render.resolution_y = 900
    scene.render.resolution_percentage = 100
    scene.render.image_settings.file_format = "PNG"
    scene.render.film_transparent = False
    scene.render.image_settings.color_mode = "RGBA"
    scene.world.color = (0.018, 0.023, 0.035)
    scene.view_settings.look = "AgX - Medium High Contrast"
    scene.view_settings.exposure = -1.0

    camera_data = bpy.data.cameras.new("NonoAuditCamera")
    camera_data.lens = 62
    camera = bpy.data.objects.new("NonoAuditCamera", camera_data)
    scene.collection.objects.link(camera)
    scene.camera = camera

    add_area("NonoAuditKey", (center.x - 2.2, center.y - 2.8, high.z + 1.4), 140, (1.0, 0.78, 0.9), height * 1.8, center)
    add_area("NonoAuditFill", (center.x + 2.8, center.y - 1.2, center.z + 0.4), 85, (0.55, 0.82, 1.0), height * 1.5, center)
    add_area("NonoAuditRim", (center.x - 1.3, center.y + 2.5, high.z), 180, (0.42, 0.32, 1.0), height, center)

    views = {
        "front": Vector((center.x, center.y - radius, center.z + height * 0.03)),
        "three-quarter": Vector((center.x + radius * 0.68, center.y - radius * 0.68, center.z + height * 0.03)),
        "side": Vector((center.x + radius, center.y, center.z + height * 0.03)),
        "back": Vector((center.x, center.y + radius, center.z + height * 0.03)),
    }
    for name, position in views.items():
        camera.location = position
        look_at(camera, center + Vector((0, 0, height * 0.02)))
        scene.render.filepath = str(output / f"nono-{name}.png")
        bpy.ops.render.render(write_still=True)
        print(f"NONO_RENDER={scene.render.filepath}")


if __name__ == "__main__":
    main()
