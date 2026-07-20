#!/usr/bin/env python3
"""Prepare Nono's Blender checkpoint for a safe, single-rig glTF export.

Run through Blender, for example:

    Blender -b NonoSubProductionSource.blend \
      --python scripts/prepare_nono_production.py -- \
      --output NonoSubProduction.blend \
      --candidate-glb NonoSubProductionCandidate.glb

The script deliberately refuses to overwrite NonoSubCheckpoint.blend. It keeps
all construction/source objects in SOURCE_ONLY and selects only NONO_EXPORT for
the candidate export. The candidate may omit Idle/Think/Present while Nico is
still authoring those actions; the final asset audit remains strict.
"""

from __future__ import annotations

import argparse
import math
import sys
import traceback
from pathlib import Path
from typing import Iterable

import bmesh
import bpy
from mathutils import Vector


CHECKPOINT_NAME = "NonoSubCheckpoint.blend"
CANONICAL_RIG_SOURCE = "Nono_Rig2"
DUPLICATE_RIG_SOURCE = "Nono_Rig2.001"
SOURCE_COLLECTION = "SOURCE_ONLY"
EXPORT_COLLECTION = "NONO_EXPORT"
PLANE_DETAILS = ("Plane", "Plane.001", "Plane.003", "Plane.004")
EXCLUDED_OBJECTS = {"Plane.002", "Nono_TeacherSkirt", "Nono_Blazer_Button"}
TAIL_BONES = tuple(f"spine.{index:03d}" for index in range(55, 79))
DYNAMIC_HAIR_ROOTS = ("spine.021", "spine.031", "spine.039", "spine.085", "spine.093")


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


PALETTES = {
    "Nono_Skin": (0.88, 0.58, 0.46, 1.0),
    "Nono_Face": (1.0, 1.0, 1.0, 1.0),
    "Nono_Hair": (0.19, 0.78, 0.86, 1.0),
    "Nono_TailCable": (0.018, 0.035, 0.11, 1.0),
    "Nono_Blazer": (0.48, 0.27, 0.12, 1.0),
    "Nono_Shirt": (0.92, 0.9, 0.84, 1.0),
    "Nono_Skirt": (0.016, 0.035, 0.12, 1.0),
    "Nono_Eye": (1.0, 1.0, 1.0, 1.0),
    "Nono_Metal": (0.54, 0.55, 0.6, 1.0),
    "Nono_Accent": (0.9, 0.27, 0.58, 1.0),
    "Nono_Shoes": (0.08, 0.08, 0.11, 1.0),
    "Nono_Socks": (0.88, 0.9, 0.94, 1.0),
    "Nono_Hair_Highlight": (0.75, 0.98, 1.0, 0.72),
}


def parse_args() -> argparse.Namespace:
    argv = sys.argv[sys.argv.index("--") + 1 :] if "--" in sys.argv else []
    parser = argparse.ArgumentParser()
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--candidate-glb", type=Path)
    parser.add_argument("--max-texture", type=int, default=1024)
    return parser.parse_args(argv)


def ensure_safe_output(output: Path) -> Path:
    output = output.expanduser().resolve()
    if output.name == CHECKPOINT_NAME:
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


def retarget_mesh(obj: bpy.types.Object, old_rig: bpy.types.Object, rig: bpy.types.Object) -> None:
    world = obj.matrix_world.copy()
    for modifier in obj.modifiers:
        if modifier.type == "ARMATURE" and modifier.object == old_rig:
            modifier.object = rig
    if obj.parent == old_rig:
        obj.parent = rig
        obj.matrix_world = world


def source_name(name: str) -> str:
    return name if name.startswith("SOURCE_") else f"SOURCE_{name}"


def classify_export_objects(rig: bpy.types.Object, duplicate: bpy.types.Object) -> set[bpy.types.Object]:
    result: set[bpy.types.Object] = {rig}
    for obj in bpy.data.objects:
        if obj.name in EXCLUDED_OBJECTS:
            continue
        if obj.name in PLANE_DETAILS:
            result.add(obj)
            continue
        target = armature_target(obj)
        if obj.type == "MESH" and target in {rig, duplicate}:
            result.add(obj)
    return result


def isolate_collections(export_objects: set[bpy.types.Object]) -> tuple[bpy.types.Collection, bpy.types.Collection]:
    source = collection(SOURCE_COLLECTION)
    export = collection(EXPORT_COLLECTION)
    source.hide_render = True
    source.hide_viewport = True
    for obj in list(bpy.data.objects):
        move_to_collection(obj, export if obj in export_objects else source)
        if obj not in export_objects and not obj.name.startswith("SOURCE_"):
            obj.name = source_name(obj.name)
    return source, export


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


def duplicate_full_body(export_body: bpy.types.Object, source: bpy.types.Collection) -> bpy.types.Object:
    copy = export_body.copy()
    copy.data = export_body.data.copy()
    copy.name = "SOURCE_Nono_Body_Full"
    source.objects.link(copy)
    copy.hide_render = True
    copy.hide_set(True)
    return copy


def vertex_kept_as_exposed_skin(obj: bpy.types.Object, vertex: bpy.types.MeshVertex) -> bool:
    # The head is a separate object. Keep the upper neck boundary and all hand /
    # finger vertices; clothing and socks cover the remaining body surface.
    if vertex.co.z >= 0.76:
        return True
    for assignment in vertex.groups:
        if assignment.weight < 0.08:
            continue
        name = obj.vertex_groups[assignment.group].name.lower()
        if any(token in name for token in ("hand", "palm", "thumb", "f_index", "f_middle", "f_ring", "f_pinky")):
            return True
    return False


def mask_covered_body(obj: bpy.types.Object) -> None:
    keep = {vertex.index for vertex in obj.data.vertices if vertex_kept_as_exposed_skin(obj, vertex)}
    mesh = bmesh.new()
    mesh.from_mesh(obj.data)
    mesh.verts.ensure_lookup_table()
    remove_faces = [face for face in mesh.faces if not all(vertex.index in keep for vertex in face.verts)]
    bmesh.ops.delete(mesh, geom=remove_faces, context="FACES")
    loose = [vertex for vertex in mesh.verts if not vertex.link_faces and not vertex.link_edges]
    if loose:
        bmesh.ops.delete(mesh, geom=loose, context="VERTS")
    mesh.to_mesh(obj.data)
    mesh.free()
    obj.data.update()


def normalize_weights(obj: bpy.types.Object) -> None:
    if obj.type != "MESH" or not obj.vertex_groups:
        return
    bpy.ops.object.select_all(action="DESELECT")
    obj.hide_set(False)
    obj.select_set(True)
    bpy.context.view_layer.objects.active = obj
    bpy.ops.object.mode_set(mode="WEIGHT_PAINT")
    bpy.ops.object.vertex_group_clean(group_select_mode="ALL", limit=0.001, keep_single=True)
    bpy.ops.object.vertex_group_limit_total(group_select_mode="ALL", limit=4)
    bpy.ops.object.vertex_group_normalize_all(group_select_mode="ALL", lock_active=False)
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


def first_color_image(material: bpy.types.Material | None) -> bpy.types.Image | None:
    if material is None or not material.use_nodes or material.node_tree is None:
        return None
    images = [node.image for node in material.node_tree.nodes if node.type == "TEX_IMAGE" and node.image]
    if not images:
        return None
    # Prefer human-readable color maps over mask/normal helpers.
    images.sort(key=lambda image: (any(token in image.name.lower() for token in ("mask", "normal", "rough")), -image.size[0]))
    return images[0]


def material_role(obj: bpy.types.Object, original: bpy.types.Material | None) -> str:
    name = f"{obj.name} {original.name if original else ''}".lower()
    if "hair_highlight" in name:
        return "Nono_Hair_Highlight"
    if any(token in name for token in ("eye", "iris", "pupil", "moon", "shine")):
        return "Nono_Eye"
    if "head" in name or "face" in name:
        return "Nono_Face"
    if "body" in name:
        return "Nono_Skin"
    if "hair" in name:
        return "Nono_Accent" if "bow" in name or "clip" in name else "Nono_Hair"
    if "tail" in name:
        return "Nono_Metal" if "plug" in name else "Nono_TailCable"
    if "blazer" in name or "jacket" in name or "chestdetail" in name:
        if any(token in name for token in ("button", "pin_rim", "thread")):
            return "Nono_Metal"
        if any(token in name for token in ("pin_enamel", "pin_spark")):
            return "Nono_Accent"
        return "Nono_Blazer"
    if "shirt" in name:
        return "Nono_Shirt"
    if "skirt" in name or "neckbow" in name or "short" in name:
        return "Nono_Skirt"
    if "sock" in name:
        return "Nono_Socks"
    if "shoe" in name:
        return "Nono_Shoes"
    return "Nono_Accent"


def socket(node: bpy.types.Node, *names: str):
    for name in names:
        if name in node.inputs:
            return node.inputs[name]
    return None


def create_portable_material(
    role: str,
    image: bpy.types.Image | None,
    cache: dict[tuple[str, str | None], bpy.types.Material],
) -> bpy.types.Material:
    key = (role, image.name if image else None)
    if key in cache:
        return cache[key]
    suffix = f"_{image.name.replace(' ', '_')}" if image and role in {"Nono_Eye", "Nono_Face", "Nono_Skin", "Nono_Metal", "Nono_Shoes"} else ""
    material = bpy.data.materials.new(f"{role}{suffix}")
    material.use_nodes = True
    nodes = material.node_tree.nodes
    links = material.node_tree.links
    nodes.clear()
    output = nodes.new("ShaderNodeOutputMaterial")
    shader = nodes.new("ShaderNodeBsdfPrincipled")
    links.new(shader.outputs["BSDF"], output.inputs["Surface"])
    base = socket(shader, "Base Color")
    base.default_value = PALETTES[role]
    roughness = socket(shader, "Roughness")
    if roughness:
        roughness.default_value = 0.34 if role in {"Nono_Eye", "Nono_Metal"} else 0.72
    metallic = socket(shader, "Metallic")
    if metallic:
        metallic.default_value = 0.24 if role == "Nono_Metal" else 0.0
    if image:
        texture = nodes.new("ShaderNodeTexImage")
        texture.image = image
        links.new(texture.outputs["Color"], base)
        alpha = socket(shader, "Alpha")
        if alpha and "Alpha" in texture.outputs:
            links.new(texture.outputs["Alpha"], alpha)
    if role in {"Nono_Eye", "Nono_Hair_Highlight"}:
        emission = socket(shader, "Emission Color", "Emission")
        if emission:
            emission.default_value = PALETTES[role]
        strength = socket(shader, "Emission Strength")
        if strength:
            strength.default_value = 0.26 if role == "Nono_Eye" else 0.8
    if role == "Nono_Hair_Highlight":
        alpha = socket(shader, "Alpha")
        if alpha:
            alpha.default_value = PALETTES[role][3]
        material.surface_render_method = "DITHERED"
        material.use_transparency_overlap = False
    cache[key] = material
    return material


def replace_export_materials(export_meshes: Iterable[bpy.types.Object], max_texture: int) -> None:
    cache: dict[tuple[str, str | None], bpy.types.Material] = {}
    scaled: set[bpy.types.Image] = set()
    for obj in export_meshes:
        if obj.type != "MESH":
            continue
        old_materials = list(obj.data.materials)
        obj.data.materials.clear()
        for original in old_materials or [None]:
            role = material_role(obj, original)
            image = first_color_image(original) if role in {"Nono_Eye", "Nono_Face", "Nono_Skin", "Nono_Metal", "Nono_Shoes"} else None
            if image and image not in scaled and max(image.size) > max_texture:
                ratio = max_texture / max(image.size)
                image.scale(max(1, round(image.size[0] * ratio)), max(1, round(image.size[1] * ratio)))
                scaled.add(image)
            obj.data.materials.append(create_portable_material(role, image, cache))


def create_hair_highlight(rig: bpy.types.Object, export: bpy.types.Collection) -> bpy.types.Object:
    # Four short anime-style shine strokes are projected onto the actual bangs.
    # They then follow the head bone, while the runtime shader supplies the
    # softer view-dependent hair band underneath.
    hair = bpy.data.objects.get("Nono_Hair_Bangs")
    if not hair or hair.type != "MESH":
        raise RuntimeError("Nono_Hair_Bangs is required to derive the authored hair highlight")
    center_x = rig.location.x
    y = -0.085
    strokes = (
        ((-0.115, 1.300), (-0.078, 1.274)),
        ((-0.050, 1.320), (-0.014, 1.288)),
        ((0.018, 1.315), (0.052, 1.284)),
        ((0.080, 1.300), (0.112, 1.276)),
    )
    vertices: list[tuple[float, float, float]] = []
    faces: list[tuple[int, int, int, int]] = []
    half_width = 0.006
    for start, end in strokes:
        direction = Vector((end[0] - start[0], end[1] - start[1]))
        normal = Vector((-direction.y, direction.x)).normalized() * half_width
        base = len(vertices)
        vertices.extend((
            (center_x + start[0] + normal.x, y, start[1] + normal.y),
            (center_x + start[0] - normal.x, y, start[1] - normal.y),
            (center_x + end[0] - normal.x, y, end[1] - normal.y),
            (center_x + end[0] + normal.x, y, end[1] + normal.y),
        ))
        faces.append((base, base + 1, base + 2, base + 3))
    mesh = bpy.data.meshes.new("Nono_Hair_Highlight_Mesh")
    mesh.from_pydata(vertices, [], faces)
    mesh.update()
    obj = bpy.data.objects.new("Nono_Hair_Highlight", mesh)
    export.objects.link(obj)
    shrinkwrap = obj.modifiers.new("ConformToHair", "SHRINKWRAP")
    shrinkwrap.target = hair
    shrinkwrap.wrap_method = "NEAREST_SURFACEPOINT"
    shrinkwrap.wrap_mode = "ABOVE_SURFACE"
    shrinkwrap.offset = 0.0015
    bpy.ops.object.select_all(action="DESELECT")
    obj.select_set(True)
    bpy.context.view_layer.objects.active = obj
    bpy.ops.object.modifier_apply(modifier=shrinkwrap.name)
    world = obj.matrix_world.copy()
    obj.parent = rig
    obj.matrix_world = world
    armature = obj.modifiers.new("NonoArmature", "ARMATURE")
    armature.object = rig
    group = obj.vertex_groups.new(name="spine.006")
    group.add(range(len(mesh.vertices)), 1.0, "REPLACE")
    return obj


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
    for required in ("Nono_Rig", "Nono_Body", "Nono_Tails", "Nono_Tail_Plugs", "Nono_Hair_Highlight"):
        if required not in names:
            raise RuntimeError(f"Prepared scene is missing {required}")
    for bone in (*TAIL_BONES, *DYNAMIC_HAIR_ROOTS):
        if bone not in rig.data.bones:
            raise RuntimeError(f"Canonical rig is missing required bone {bone}")
    if any(name.startswith("SOURCE_") for name in names):
        raise RuntimeError("SOURCE_ONLY object leaked into NONO_EXPORT")


def export_candidate(path: Path, export: bpy.types.Collection) -> None:
    path = path.expanduser().resolve()
    path.parent.mkdir(parents=True, exist_ok=True)
    for obj in export.all_objects:
        obj.hide_set(False)
        obj.hide_render = False
    bpy.ops.export_scene.gltf(
        filepath=str(path),
        export_format="GLB",
        use_selection=False,
        collection=EXPORT_COLLECTION,
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
        export_animation_mode="ACTIVE_ACTIONS",
        export_force_sampling=True,
        export_lights=False,
        export_cameras=False,
    )


def main() -> None:
    args = parse_args()
    output = ensure_safe_output(args.output)
    rig = bpy.data.objects.get(CANONICAL_RIG_SOURCE)
    if not rig or rig.type != "ARMATURE":
        raise RuntimeError("Expected a Nono_Rig2 armature in the source file")
    # Checkpoint files carry a single rig; the duplicate only exists in
    # NonoSubProductionSource.blend. Treat it as optional.
    duplicate = bpy.data.objects.get(DUPLICATE_RIG_SOURCE)
    if duplicate is not None and duplicate.type != "ARMATURE":
        raise RuntimeError("Nono_Rig2.001 exists but is not an armature")
    if duplicate is None:
        duplicate = rig

    export_objects = classify_export_objects(rig, duplicate)
    for obj in tuple(export_objects):
        if obj.type == "MESH" and armature_target(obj) == duplicate:
            retarget_mesh(obj, duplicate, rig)

    source, export = isolate_collections(export_objects)
    plane_objects = {obj for obj in export_objects if obj.name in PLANE_DETAILS}
    export_objects.difference_update(plane_objects)
    detail = join_plane_details(rig, export)
    export_objects.add(detail)

    # Source names were reserved before the production objects receive their
    # canonical names, preventing Blender from silently adding .001 suffixes.
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
    rig.name = "Nono_Rig"
    rig.data.name = "Nono_Rig_Armature"

    body = bpy.data.objects.get("Nono_Body")
    if not body:
        raise RuntimeError("Could not identify the production Nono body")
    duplicate_full_body(body, source)
    mask_covered_body(body)

    highlight = create_hair_highlight(rig, export)
    export_objects.add(highlight)
    center_canonical_rig(rig)

    for obj in list(export.all_objects):
        if obj.type == "MESH":
            bind_unweighted_vertices(obj, rig)
            normalize_weights(obj)
    replace_export_materials((obj for obj in export.all_objects if obj.type == "MESH"), args.max_texture)
    validate_prepared_scene(export, rig)

    bpy.ops.wm.save_as_mainfile(filepath=str(output), check_existing=False)
    if args.candidate_glb:
        export_candidate(args.candidate_glb, export)
    print(f"NONO_PREPARED_BLEND={output}")
    if args.candidate_glb:
        print(f"NONO_CANDIDATE_GLB={args.candidate_glb.expanduser().resolve()}")


if __name__ == "__main__":
    try:
        main()
    except Exception:
        traceback.print_exc()
        raise
