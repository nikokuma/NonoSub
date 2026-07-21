#!/usr/bin/env python3
"""Prepare NonoSubCheckpointFinal.blend for release export without fidelity loss.

Unlike prepare_nono_production.py (which masks the body, replaces every
material with flat palette stand-ins, and fabricates a hair highlight), this
script keeps the checkpoint's original geometry, materials, textures, and
modifier stacks. It only:

  * renames Nono_Rig2 -> Nono_Rig and zeroes its object transform,
  * moves the teacher-form objects into NONO_EXPORT (hoodie form stays out),
  * joins the loose chest-detail planes into one rig-bound mesh,
  * normalizes skin weights to the 4-influence GLB budget (deform bones only),
  * unlinks Principled alpha on materials that don't need blending so the
    glTF exporter emits OPAQUE instead of sort-order-fragile BLEND,
  * downsizes textures above --max-texture and packs them into the file,
  * imports the final mean-centered mocap actions from NonoSubProduction2.blend.

Run:

    Blender -b NonoSubCheckpointFinal.blend \
      --python scripts/prepare_nono_release.py -- \
      --output ~/Projects/Blendr/NonoSubRelease.blend
"""

from __future__ import annotations

import argparse
import sys
import traceback
from pathlib import Path

import bpy
from mathutils import Vector


CANONICAL_RIG_SOURCE = "Nono_Rig2"
EXPORT_COLLECTION = "NONO_EXPORT"
PLANE_DETAILS = ("Plane", "Plane.001", "Plane.003", "Plane.004")
EXCLUDED_OBJECTS = {"Plane.002", "Nono_TeacherSkirt"}
PROTECTED_SOURCES = {
    "NonoSubCheckpoint.blend",
    "NonoSubCheckpointCodex.blend",
    "NonoSubCheckpointFable.blend",
    "NonoSubCheckpointFinal.blend",
    "NonoSubCheckpointKimi.blend",
}
TAIL_BONES = tuple(f"spine.{index:03d}" for index in range(55, 79))
DYNAMIC_HAIR_ROOTS = ("spine.021", "spine.031", "spine.039", "spine.085", "spine.093")
FINAL_ACTIONS = (
    "Idle",
    "Neutral",
    "Think",
    "Thumbs_Up",
    "Point_User",
    "Point_Self",
    "Cheer",
    "Heart_Touch",
    "Surprised",
)
SHAPE_KEY_MESHES = ("Nono_Head.001", "Nono_BrowsNLashes.001")

# Canonical object names expected by the GLB audit and the app runtime.
SEMANTIC_NAMES = {
    "Nono_Body.001": "Nono_Body",
    "Nono_Head.001": "Nono_Head",
    "Nono_BrowsNLashes.001": "Nono_BrowsNLashes",
    "Nono_Eye_Iris.001": "Nono_Eye_Iris",
    "Nono_Eye_Pupil.001": "Nono_Eye_Pupil",
    "Nono_Eye_ShineStripe.001": "Nono_Eye_ShineStripe",
    "Nono_Eyes_Moon.001": "Nono_Eyes_Moon",
    "Nono_Eyes_PinkSmall.001": "Nono_Eyes_PinkSmall",
    "Nono_Eyes_TopShine.001": "Nono_Eyes_TopShine",
    "Nono_Hair_Bangs.001": "Nono_Hair_Bangs",
    "Nono_Hair_Fwip.001": "Nono_Hair_Fwip",
    "Nono_Hair_Long.001": "Nono_Hair_Long",
    "Nono_Hair_Sweep.001": "Nono_Hair_Sweep",
    "Nono_Hair_Bow.001": "Nono_Hair_Bow_Pink",
    "Nono_Hair_Bow.002": "Nono_Hair_Bow_Navy",
    "Nono_NoHairClip.L.001": "Nono_Hair_Clip_L",
    "Nono_NoHairClip.R.001": "Nono_Hair_Clip_R",
    "Nono_Tails.001": "Nono_Tails",
    "Nono_Tail_Plugs.001": "Nono_Tail_Plugs",
    "Nono_Blazer": "Nono_Outfit_Blazer",
    "Nono_Shirt": "Nono_Outfit_Shirt",
    "Nono_Shirt.001": "Nono_Outfit_Shirt_Detail_L",
    "Nono_Shirt.002": "Nono_Outfit_Shirt_Detail_R",
    "Nono_NeckBow": "Nono_Outfit_NeckBow",
    "Nono_Skirt.001": "Nono_Outfit_Skirt",
    "Nono_Shorts.001": "Nono_Outfit_Shorts",
    "Nono_Socks.001": "Nono_Outfit_Socks",
    "Nono_Shoes.001": "Nono_Outfit_Shoes",
    "Nono_Shoe_Tongue.001": "Nono_Outfit_Shoe_Tongue",
    "Nono_Shoes_Laces.001": "Nono_Outfit_Shoe_Laces",
    "Nono_Shoes_No.001": "Nono_Outfit_Shoe_Accent",
}

# Rename-only material mapping: the audit requires Nono_<Role> prefixes and
# the app infers toon roles from these names. Material contents are untouched.
SEMANTIC_MATERIALS = {
    "Body": "Nono_Skin_Body",
    "Face": "Nono_Face_Base",
    "Material.001": "Nono_Hair_Main",
    "HairClipNoL": "Nono_Hair_Clip_L",
    "HairClipNoR": "Nono_Hair_Clip_R",
    "PinkBow": "Nono_Accent_BowPink",
    "DarkBlueBow": "Nono_Accent_BowNavy",
    "Material.012": "Nono_TailCable_Main",
    "Material.013": "Nono_Metal_TailPlug",
    "Blazer_Camel": "Nono_Blazer_Camel",
    "TeacherJacket_P5_Seam_PBR": "Nono_Blazer_Seam",
    "TeacherJacket_P5_PocketShadow_PBR": "Nono_Blazer_PocketShadow",
    "TeacherJacket_P5_Button_Brown_PBR": "Nono_Metal_ButtonBrown",
    "TeacherJacket_P5_ButtonThread_PBR": "Nono_Metal_ButtonThread",
    "Teacher_Collared_Shirt_Button": "Nono_Metal_ShirtButton",
    "Teacher_Collared_Shirt_Button_Loops": "Nono_Metal_ShirtButtonLoops",
    "Material.006": "Nono_Metal_ButtonAccent",
    "TeacherJacket_P5_Pin_Enamel_PBR": "Nono_Accent_PinEnamel",
    "TeacherJacket_P5_Pin_Rim_PBR": "Nono_Metal_PinRim",
    "TeacherJacket_P5_Pin_Mark_PBR": "Nono_Accent_PinSpark",
    "Shirt_White": "Nono_Shirt_White",
    "Skirt_Navy": "Nono_Skirt_Navy",
    "Material.005": "Nono_Skirt_Shorts",
    "Socks": "Nono_Socks_Main",
    "Shoes": "Nono_Shoes_Tongue",
    "Shoes_Converse": "Nono_Shoes_Converse",
    "ShoeLaces": "Nono_Shoes_Laces",
    "ShoeNo": "Nono_Shoes_AccentNo",
    "ShoeRings": "Nono_Metal_ShoeRings",
    "Eye": "Nono_Eye_Iris",
    "EyeShine": "Nono_Eye_Shine",
    "EyeSparklePink": "Nono_Eye_SparklePink",
    "Material.002": "Nono_Eye_Moon",
}


def parse_args() -> argparse.Namespace:
    argv = sys.argv[sys.argv.index("--") + 1 :] if "--" in sys.argv else []
    parser = argparse.ArgumentParser()
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument(
        "--actions-from",
        type=Path,
        default=None,
        help="Optional blend file to import the final actions from; by default the checkpoint's own actions are used",
    )
    parser.add_argument("--max-texture", type=int, default=1024)
    return parser.parse_args(argv)


def ensure_safe_output(output: Path) -> Path:
    output = output.expanduser().resolve()
    if output.name in PROTECTED_SOURCES:
        raise RuntimeError(f"Refusing to overwrite protected source: {output}")
    output.parent.mkdir(parents=True, exist_ok=True)
    return output


def collection(name: str) -> bpy.types.Collection:
    result = bpy.data.collections.get(name)
    if result is None:
        result = bpy.data.collections.new(name)
        bpy.context.scene.collection.children.link(result)
    return result


def move_to_collection(obj: bpy.types.Object, target: bpy.types.Collection) -> None:
    if obj.name not in target.objects:
        target.objects.link(obj)
    for existing in list(obj.users_collection):
        if existing != target:
            existing.objects.unlink(obj)


def armature_target(obj: bpy.types.Object) -> bpy.types.Object | None:
    if obj.type != "MESH":
        return None
    for modifier in obj.modifiers:
        if modifier.type == "ARMATURE" and modifier.object is not None:
            return modifier.object
    return obj.parent if obj.parent and obj.parent.type == "ARMATURE" else None


def classify_export_objects(rig: bpy.types.Object) -> set[bpy.types.Object]:
    teacher = bpy.data.collections.get("Collection")
    if teacher is None:
        raise RuntimeError("Expected the teacher-form scene collection named 'Collection'")
    result: set[bpy.types.Object] = {rig}
    for obj in teacher.objects:
        if obj.name in EXCLUDED_OBJECTS:
            continue
        if obj.name in PLANE_DETAILS:
            result.add(obj)
            continue
        if obj.type == "MESH" and armature_target(obj) == rig:
            result.add(obj)
    return result


def join_plane_details(rig: bpy.types.Object, export: bpy.types.Collection) -> bpy.types.Object:
    planes = [bpy.data.objects.get(name) for name in PLANE_DETAILS]
    planes = [obj for obj in planes if obj and obj.type == "MESH"]
    if len(planes) != len(PLANE_DETAILS):
        missing = sorted(set(PLANE_DETAILS) - {obj.name for obj in planes})
        raise RuntimeError(f"Missing visible outfit plane details: {missing}")

    bpy.ops.object.select_all(action="DESELECT")
    for obj in planes:
        obj.hide_set(False)
        obj.select_set(True)
    bpy.context.view_layer.objects.active = planes[0]
    bpy.ops.object.transform_apply(location=False, rotation=True, scale=True)
    bpy.ops.object.join()
    detail = bpy.context.object
    detail.name = "Nono_Outfit_ChestDetails"
    detail.data.name = "Nono_Outfit_ChestDetails_Mesh"
    world = detail.matrix_world.copy()
    detail.parent = rig
    detail.matrix_world = world
    modifier = next((item for item in detail.modifiers if item.type == "ARMATURE"), None)
    modifier = modifier or detail.modifiers.new("NonoArmature", "ARMATURE")
    modifier.object = rig
    detail.vertex_groups.clear()
    group = detail.vertex_groups.new(name="spine.001")
    group.add(range(len(detail.data.vertices)), 1.0, "REPLACE")
    move_to_collection(detail, export)
    return detail


def normalize_deform_weights(obj: bpy.types.Object) -> None:
    if obj.type != "MESH" or not obj.vertex_groups:
        return
    bpy.ops.object.select_all(action="DESELECT")
    obj.hide_set(False)
    obj.select_set(True)
    bpy.context.view_layer.objects.active = obj
    bpy.ops.object.mode_set(mode="WEIGHT_PAINT")
    bpy.ops.object.vertex_group_clean(group_select_mode="BONE_DEFORM", limit=0.001, keep_single=True)
    bpy.ops.object.vertex_group_limit_total(group_select_mode="BONE_DEFORM", limit=4)
    bpy.ops.object.vertex_group_normalize_all(group_select_mode="BONE_DEFORM", lock_active=False)
    bpy.ops.object.mode_set(mode="OBJECT")


def bind_unweighted_vertices(obj: bpy.types.Object, rig: bpy.types.Object) -> int:
    if obj.type != "MESH":
        return 0
    unweighted = []
    for vertex in obj.data.vertices:
        total = sum(assignment.weight for assignment in vertex.groups if assignment.weight > 0.0001)
        if total <= 0.0001:
            unweighted.append(vertex)
    if not unweighted:
        return 0
    deform_bones = [bone for bone in rig.data.bones if bone.use_deform]
    if not deform_bones:
        raise RuntimeError("Canonical rig has no deformation bones")
    bone_positions = {bone.name: rig.matrix_world @ bone.head_local for bone in deform_bones}
    groups: dict[str, bpy.types.VertexGroup] = {}
    for vertex in unweighted:
        world = obj.matrix_world @ vertex.co
        closest = min(deform_bones, key=lambda bone: (bone_positions[bone.name] - world).length_squared)
        groups[closest.name] = groups.get(closest.name) or obj.vertex_groups.get(closest.name) or obj.vertex_groups.new(name=closest.name)
        groups[closest.name].add([vertex.index], 1.0, "REPLACE")
    print(f"Bound {len(unweighted)} previously unweighted vertices on {obj.name} to nearest deformation bones")
    return len(unweighted)


def apply_modifiers_preserving_shape_keys(obj: bpy.types.Object) -> None:
    """Bake non-armature modifiers into every shape while keeping one topology."""
    shape_keys = obj.data.shape_keys
    if shape_keys is None:
        raise RuntimeError(f"{obj.name} is missing its required shape keys")

    shape_keys.animation_data_clear()
    for key in shape_keys.key_blocks:
        key.value = 0.0
    obj.show_only_shape_key = False

    key_names = [key.name for key in shape_keys.key_blocks]
    captures: list[bpy.types.Mesh] = []
    duplicates: list[tuple[bpy.types.Object, bpy.types.Mesh]] = []
    depsgraph = bpy.context.evaluated_depsgraph_get()
    try:
        for index, key_name in enumerate(key_names):
            duplicate = obj.copy()
            duplicate.data = obj.data.copy()
            duplicate.name = f"{obj.name}__{key_name}_modifier_capture"
            bpy.context.scene.collection.objects.link(duplicate)
            duplicates.append((duplicate, duplicate.data))
            for modifier in list(duplicate.modifiers):
                if modifier.type == "ARMATURE":
                    duplicate.modifiers.remove(modifier)
            duplicate.show_only_shape_key = True
            duplicate.active_shape_key_index = index
            bpy.context.view_layer.update()
            capture = bpy.data.meshes.new_from_object(
                duplicate.evaluated_get(depsgraph),
                preserve_all_data_layers=True,
                depsgraph=depsgraph,
            )
            capture.name = f"{obj.data.name}__{key_name}_applied"
            captures.append(capture)

        topology = {(len(mesh.vertices), len(mesh.polygons)) for mesh in captures}
        if len(topology) != 1:
            details = ", ".join(
                f"{name}=({len(mesh.vertices)} vertices, {len(mesh.polygons)} polygons)"
                for name, mesh in zip(key_names, captures)
            )
            raise RuntimeError(f"Shape-key modifier topology diverged for {obj.name}: {details}")

        old_mesh = obj.data
        obj.data = captures[0]
        obj.data.name = old_mesh.name
        for modifier in list(obj.modifiers):
            if modifier.type != "ARMATURE":
                obj.modifiers.remove(modifier)

        obj.shape_key_add(name=key_names[0], from_mix=False)
        for key_name, capture in zip(key_names[1:], captures[1:]):
            key = obj.shape_key_add(name=key_name, from_mix=False)
            coordinates = [component for vertex in capture.vertices for component in vertex.co]
            key.data.foreach_set("co", coordinates)
            key.value = 0.0
        obj.data.shape_keys.use_relative = True
        obj.show_only_shape_key = False
        obj.active_shape_key_index = 0
        for key in obj.data.shape_keys.key_blocks:
            key.value = 0.0
        print(
            f"Applied non-armature modifiers across {obj.name} shape keys: "
            f"{key_names} ({len(obj.data.vertices)} vertices, {len(obj.data.polygons)} polygons)"
        )

        if old_mesh.users == 0:
            bpy.data.meshes.remove(old_mesh)
    finally:
        for duplicate, duplicate_mesh in duplicates:
            bpy.data.objects.remove(duplicate, do_unlink=True)
            if duplicate_mesh.users == 0:
                bpy.data.meshes.remove(duplicate_mesh)
        for capture in captures:
            if capture.users == 0:
                bpy.data.meshes.remove(capture)


def _socket_default(socket):
    value = getattr(socket, "default_value", None)
    if value is None:
        return None
    try:
        return list(value)
    except TypeError:
        return float(value)


def _as_color(value):
    if isinstance(value, list) and len(value) >= 3:
        return value[:4] if len(value) >= 4 else [*value[:3], 1.0]
    return None


def _resolve_color_source(socket, seen: set[tuple[int, str]] | None = None):
    """Resolve a flat color or image source through RGB/ramp/Mix nodes."""
    if not socket.is_linked:
        color = _as_color(_socket_default(socket))
        return ("COLOR", color) if color is not None else None
    seen = seen or set()
    link = socket.links[0]
    node = link.from_node
    marker = (node.as_pointer(), link.from_socket.identifier)
    if marker in seen:
        return None
    seen.add(marker)

    if node.type == "TEX_IMAGE":
        color_output = node.outputs.get("Color")
        return ("TEXTURE", color_output) if node.image is not None and color_output is not None else None
    if node.type == "RGB":
        color = _as_color(_socket_default(link.from_socket))
        return ("COLOR", color) if color is not None else None
    if node.type == "VALTORGB":
        elements = node.color_ramp.elements
        color = list(elements[-1].color) if elements else None
        return ("COLOR", color) if color is not None else None
    if node.type not in {"MIX_RGB", "MIX"}:
        return None

    # The authored lit/image value is the second branch. MIX nodes can expose
    # duplicate A/B sockets for different data types, so consider every socket
    # with the preferred name until a color-capable source resolves.
    preferred_names = ("Color2", "B", "Color1", "A")
    candidates = []
    for name in preferred_names:
        candidates.extend(candidate for candidate in node.inputs if candidate.name == name)
    for candidate in candidates:
        source = _resolve_color_source(candidate, set(seen))
        if source is not None:
            return source
    return None


def bake_toon_flat_colors(materials: set[bpy.types.Material]) -> None:
    """Expose each toon chain as a glTF-readable flat color or image.

    The glTF exporter cannot evaluate the authored toon ramps and Mix nodes.
    Constant chains are baked into Base Color; image sources hidden behind a
    Mix are rewired directly so the authored texture remains intact.
    """
    for material in materials:
        if material is None or material.node_tree is None:
            continue
        tree = material.node_tree
        principled = next((node for node in tree.nodes if node.type == "BSDF_PRINCIPLED"), None)
        if principled is None or "Base Color" not in principled.inputs:
            continue
        base = principled.inputs["Base Color"]
        if not base.is_linked:
            continue
        if base.links[0].from_node.type == "TEX_IMAGE":
            continue

        source = _resolve_color_source(base)
        if source is None:
            mix = tree.nodes.get("A TOON MIX - CONNECTED")
            if mix is not None:
                for input_name in ("Color2", "Color1"):
                    mix_input = mix.inputs.get(input_name)
                    if mix_input is not None:
                        source = _resolve_color_source(mix_input)
                    if source is not None:
                        break
        if source is None:
            shadow = tree.nodes.get("A HSV SHADOW")
            if shadow is not None and "Color" in shadow.inputs:
                source = _resolve_color_source(shadow.inputs["Color"])
        if source is None:
            print(f"Could not resolve a flat color for {material.name}; it will export white")
            continue

        for link in list(base.links):
            tree.links.remove(link)
        source_type, source_value = source
        if source_type == "TEXTURE":
            tree.links.new(source_value, base)
            print(f"Rewired image {source_value.node.image.name} into Base Color for {material.name}")
        else:
            base.default_value = source_value
            print(f"Baked flat color {tuple(round(c, 3) for c in source_value)} into {material.name}")


def force_opaque_export(materials: set[bpy.types.Material]) -> None:
    for material in materials:
        if material is None:
            continue
        if not material.use_nodes or material.node_tree is None:
            continue
        for node in material.node_tree.nodes:
            if node.type != "BSDF_PRINCIPLED" or "Alpha" not in node.inputs:
                continue
            alpha = node.inputs["Alpha"]
            for link in list(alpha.links):
                material.node_tree.links.remove(link)
            alpha.default_value = 1.0


def shrink_and_pack_images(max_texture: int) -> None:
    for image in bpy.data.images:
        if image.size[0] == 0 or image.size[1] == 0:
            continue
        if max(image.size) > max_texture:
            ratio = max_texture / max(image.size)
            image.scale(max(1, round(image.size[0] * ratio)), max(1, round(image.size[1] * ratio)))
            print(f"Downscaled {image.name} to {image.size[0]}x{image.size[1]}")
        try:
            image.pack()
        except RuntimeError as error:
            print(f"Could not pack {image.name}: {error}")


def import_final_actions(source: Path) -> None:
    source = source.expanduser().resolve()
    if not source.exists():
        raise RuntimeError(f"Actions source not found: {source}")
    for name in FINAL_ACTIONS:
        existing = bpy.data.actions.get(name)
        if existing is not None:
            bpy.data.actions.remove(existing, do_unlink=True)
    with bpy.data.libraries.load(str(source), link=False) as (data_from, data_to):
        missing = sorted(set(FINAL_ACTIONS) - set(data_from.actions))
        if missing:
            raise RuntimeError(f"Actions missing from {source.name}: {missing}")
        data_to.actions = list(FINAL_ACTIONS)
    for name in FINAL_ACTIONS:
        action = bpy.data.actions.get(name)
        if action is None:
            raise RuntimeError(f"Failed to import action {name}")
        action.use_fake_user = True


def apply_semantic_names(export: bpy.types.Collection) -> None:
    for old_name, new_name in SEMANTIC_NAMES.items():
        obj = bpy.data.objects.get(old_name)
        if obj and obj.name in export.all_objects:
            obj.name = new_name
            if obj.type == "MESH":
                obj.data.name = f"{new_name}_Mesh"
    for obj in list(export.all_objects):
        if obj.name.startswith("Nono_TeacherJacket_"):
            name = obj.name.removesuffix(".001").replace("Nono_TeacherJacket_", "Nono_Outfit_Blazer_")
            obj.name = name
            if obj.type == "MESH":
                obj.data.name = f"{name}_Mesh"
    for old_name, new_name in SEMANTIC_MATERIALS.items():
        material = bpy.data.materials.get(old_name)
        if material is not None:
            material.name = new_name


def mean_center_hip_curves() -> None:
    """Remove the capture's room-position DC offset from each clip's hip keys.

    The raw retarget keys bone "spine" locations in absolute room space; the
    export validator (and the app) expect small re-based sway around zero.
    Subtracting the per-curve mean keeps the sway, drops the offset. Idempotent.
    """
    for name in FINAL_ACTIONS:
        action = bpy.data.actions.get(name)
        if action is None:
            continue
        for curve in action.fcurves:
            if curve.data_path != 'pose.bones["spine"].location':
                continue
            points = curve.keyframe_points
            if not points:
                continue
            mean = sum(point.co.y for point in points) / len(points)
            if abs(mean) < 1e-6:
                continue
            for point in points:
                point.co.y -= mean
                point.handle_left.y -= mean
                point.handle_right.y -= mean
            curve.update()
            print(f"Centered {name} hip axis {curve.array_index} (offset {mean:.3f})")


def center_canonical_rig(rig: bpy.types.Object) -> None:
    rig.location = Vector((0.0, 0.0, 0.0))
    rig.rotation_mode = "XYZ"
    rig.rotation_euler = Vector((0.0, 0.0, 0.0))
    rig.scale = Vector((1.0, 1.0, 1.0))


def validate_prepared_scene(export: bpy.types.Collection, rig: bpy.types.Object) -> None:
    export_objects = list(export.all_objects)
    armatures = [obj for obj in export_objects if obj.type == "ARMATURE"]
    if armatures != [rig]:
        raise RuntimeError(f"NONO_EXPORT must contain one canonical armature, found {[obj.name for obj in armatures]}")
    names = {obj.name for obj in export_objects}
    for required in (
        "Nono_Rig",
        "Nono_Body",
        "Nono_Tails",
        "Nono_Tail_Plugs",
        "Nono_Outfit_ChestDetails",
        "Nono_Squint",
    ):
        if required not in names:
            raise RuntimeError(f"Prepared scene is missing {required}")
    expected_shapes = {
        "Nono_Head": ["Basis", "Blink", "SmallSmile", "BigSmile"],
        "Nono_BrowsNLashes": ["Basis", "Blink"],
    }
    for object_name, expected_keys in expected_shapes.items():
        obj = bpy.data.objects.get(object_name)
        if obj is None or obj.name not in names or obj.type != "MESH":
            raise RuntimeError(f"Prepared scene is missing shape-key mesh {object_name}")
        shape_keys = obj.data.shape_keys
        actual_keys = [key.name for key in shape_keys.key_blocks] if shape_keys else []
        if actual_keys != expected_keys:
            raise RuntimeError(f"{object_name} shape keys are {actual_keys}; expected {expected_keys}")
        nonzero = [(key.name, key.value) for key in shape_keys.key_blocks if abs(key.value) > 1e-6]
        if nonzero:
            raise RuntimeError(f"{object_name} has nonzero shape-key values: {nonzero}")
        if shape_keys.animation_data is not None:
            raise RuntimeError(f"{object_name} shape keys still have animation data")
        modifier_types = [modifier.type for modifier in obj.modifiers]
        if modifier_types != ["ARMATURE"]:
            raise RuntimeError(f"{object_name} must retain only ARMATURE, found {modifier_types}")
    for bone in (*TAIL_BONES, *DYNAMIC_HAIR_ROOTS):
        if bone not in rig.data.bones:
            raise RuntimeError(f"Canonical rig is missing required bone {bone}")
    for name in FINAL_ACTIONS:
        if bpy.data.actions.get(name) is None:
            raise RuntimeError(f"Final action {name} is missing")


def main() -> None:
    args = parse_args()
    output = ensure_safe_output(args.output)
    if Path(bpy.data.filepath).name != "NonoSubCheckpointFinal.blend":
        raise RuntimeError("Run this script on NonoSubCheckpointFinal.blend")
    # Save-as immediately so every mutation below only ever touches the copy.
    bpy.ops.wm.save_as_mainfile(filepath=str(output), check_existing=False)

    rig = bpy.data.objects.get(CANONICAL_RIG_SOURCE)
    if not rig or rig.type != "ARMATURE":
        raise RuntimeError("Expected a Nono_Rig2 armature in the source file")
    # A checkpoint saved while displaying Rest Position would export every clip
    # as the bind pose (armature evaluation disabled). Force pose evaluation.
    rig.data.pose_position = "POSE"

    export_objects = classify_export_objects(rig)
    export = collection(EXPORT_COLLECTION)
    for obj in export_objects:
        obj.hide_set(False)
        move_to_collection(obj, export)

    detail = join_plane_details(rig, export)
    export_objects.difference_update({bpy.data.objects.get(name) for name in PLANE_DETAILS})
    export_objects.add(detail)

    rig.name = "Nono_Rig"
    rig.data.name = "Nono_Rig_Armature"
    center_canonical_rig(rig)

    materials: set[bpy.types.Material] = set()
    for obj in list(export.all_objects):
        if obj.type != "MESH":
            continue
        if obj.name in SHAPE_KEY_MESHES:
            apply_modifiers_preserving_shape_keys(obj)
        bind_unweighted_vertices(obj, rig)
        normalize_deform_weights(obj)
        materials.update(slot.material for slot in obj.material_slots if slot.material)
    bake_toon_flat_colors(materials)
    force_opaque_export(materials)
    apply_semantic_names(export)
    shrink_and_pack_images(args.max_texture)
    if args.actions_from is not None:
        import_final_actions(args.actions_from)
    mean_center_hip_curves()
    validate_prepared_scene(export, rig)

    bpy.ops.wm.save_as_mainfile(filepath=str(output), check_existing=False)
    print(f"NONO_RELEASE_BLEND={output}")


if __name__ == "__main__":
    try:
        main()
    except Exception:
        traceback.print_exc()
        sys.exit(1)
