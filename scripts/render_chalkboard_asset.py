"""Render NonoSub's chalkboard as a transparent, softly textured anime prop.

Run this from the open chalkboard Blender scene. The scene is restored after the
render so the comparison setup remains intact.
"""

from __future__ import annotations

import os
from pathlib import Path

import bpy


OUTPUT = Path(
    os.environ.get(
        "NONOSUB_CHALKBOARD_OUTPUT",
        "/Users/nico/Projects/NonoSub/static/assets/nono-chalkboard-anime.png",
    )
)


def render() -> None:
    scene = bpy.context.scene
    material = bpy.data.materials["MAT_Board_DeepGreen_ChalkDust"]
    noise = material.node_tree.nodes["Noise Texture"]
    ramp = material.node_tree.nodes["Color Ramp"].color_ramp

    original = {
        "camera": scene.camera,
        "filepath": scene.render.filepath,
        "resolution_x": scene.render.resolution_x,
        "resolution_y": scene.render.resolution_y,
        "resolution_percentage": scene.render.resolution_percentage,
        "film_transparent": scene.render.film_transparent,
        "file_format": scene.render.image_settings.file_format,
        "color_mode": scene.render.image_settings.color_mode,
        "noise_scale": noise.inputs["Scale"].default_value,
        "noise_detail": noise.inputs["Detail"].default_value,
        "ramp_colors": [tuple(element.color) for element in ramp.elements],
        "hidden": {obj.name: obj.hide_render for obj in scene.objects},
    }

    try:
        # Keep the handmade surface, but make it quiet enough for lesson text.
        noise.inputs["Scale"].default_value = 7.5
        noise.inputs["Detail"].default_value = 2.0
        ramp.elements[0].color = (0.0045, 0.066, 0.017, 1.0)
        ramp.elements[1].color = (0.013, 0.094, 0.038, 1.0)

        # Render only the textured board and remove the heavy black backplate.
        for obj in scene.objects:
            obj.hide_render = "PureGreen" in obj.name
        bpy.data.objects["Outline_Backplate"].hide_render = True

        scene.camera = bpy.data.objects["Camera_Front_16x9"]
        scene.render.resolution_x = 1920
        scene.render.resolution_y = 1080
        scene.render.resolution_percentage = 100
        scene.render.film_transparent = True
        scene.render.image_settings.file_format = "PNG"
        scene.render.image_settings.color_mode = "RGBA"
        scene.render.filepath = str(OUTPUT)

        OUTPUT.parent.mkdir(parents=True, exist_ok=True)
        bpy.ops.render.render(write_still=True)
        print(f"Rendered transparent chalkboard to {OUTPUT}")
    finally:
        scene.camera = original["camera"]
        scene.render.filepath = original["filepath"]
        scene.render.resolution_x = original["resolution_x"]
        scene.render.resolution_y = original["resolution_y"]
        scene.render.resolution_percentage = original["resolution_percentage"]
        scene.render.film_transparent = original["film_transparent"]
        scene.render.image_settings.file_format = original["file_format"]
        scene.render.image_settings.color_mode = original["color_mode"]
        noise.inputs["Scale"].default_value = original["noise_scale"]
        noise.inputs["Detail"].default_value = original["noise_detail"]
        for element, color in zip(ramp.elements, original["ramp_colors"], strict=True):
            element.color = color
        for name, hidden in original["hidden"].items():
            bpy.data.objects[name].hide_render = hidden


render()
