import { describe, expect, it } from "vitest";
import * as THREE from "three";
import {
  applyNonoMaterials,
  createNonoOutlines,
  createNonoToonMaterial,
  inferNonoMaterialRole,
  nextNonoBlinkAt,
  NONO_OUTLINE_CONFIG,
  nonoAssetFromLocation,
  nonoBlinkInfluence,
  nonoExpressionFromLocation,
  nonoMoodFromCue,
  nonoMoodFromLocation,
  nonoOutlineFromLocation,
  shaderVariantFromLocation,
} from "./nonoToon";

describe("Nono toon materials", () => {
  it("maps stable semantic and legacy names to material roles", () => {
    expect(inferNonoMaterialRole("Nono_Hair")).toBe("hair");
    expect(inferNonoMaterialRole("Material.001", "Nono_Hair_Long")).toBe("hair");
    expect(inferNonoMaterialRole("Nono_TailCable")).toBe("tail");
    expect(inferNonoMaterialRole("TeacherJacket_P5_Pin_Rim_PBR")).toBe("metal");
    expect(inferNonoMaterialRole("Nono_Shoes_AccentNo")).toBe("cloth");
    expect(inferNonoMaterialRole("Nono_Socks_PinkAccent")).toBe("cloth");
    expect(inferNonoMaterialRole("Nono_Mouth")).toBe("mouth");
    expect(inferNonoMaterialRole("Nono_Lips")).toBe("mouth");
    expect(inferNonoMaterialRole("Nono_Squint_Black")).toBe("squint");
    expect(inferNonoMaterialRole("mystery")).toBe("unknown");
  });

  it("keeps the release variant fixed while allowing a development comparison", () => {
    expect(shaderVariantFromLocation("?nonoShader=nontoon", true)).toBe("nontoon");
    expect(shaderVariantFromLocation("?nonoShader=portable", true)).toBe("portable");
    expect(shaderVariantFromLocation("?nonoShader=nontoon", false)).toBe("toon");
    expect(nonoAssetFromLocation("?nonoAsset=candidate", true)).toBe("/assets/NonoCandidate.glb");
    expect(nonoAssetFromLocation("?nonoAsset=candidate", false)).toBe("/assets/Nono.glb");
  });

  it("only honors the mood override for known moods in development", () => {
    expect(nonoMoodFromLocation("?nonoMood=thumbs_up", true)).toBe("thumbs_up");
    expect(nonoMoodFromLocation("?nonoMood=cheer", true)).toBe("cheer");
    expect(nonoMoodFromLocation("?nonoMood=moonwalk", true)).toBeUndefined();
    expect(nonoMoodFromLocation("", true)).toBeUndefined();
    expect(nonoMoodFromLocation("?nonoMood=thumbs_up", false)).toBeUndefined();
  });

  it("validates lesson gesture cues against the supported moods", () => {
    expect(nonoMoodFromCue("point_self")).toBe("point_self");
    expect(nonoMoodFromCue("heart_touch")).toBe("heart_touch");
    expect(nonoMoodFromCue("moonwalk")).toBeUndefined();
    expect(nonoMoodFromCue()).toBeUndefined();
  });

  it("only honors the squint expression override in development", () => {
    expect(nonoExpressionFromLocation("?nonoExpression=squint", true)).toBe("squint");
    expect(nonoExpressionFromLocation("?nonoExpression=blink", true)).toBeUndefined();
    expect(nonoExpressionFromLocation("?nonoExpression=squint", false)).toBeUndefined();
  });

  it("schedules blinks 8 to 12 seconds out and follows the close-hold-open profile", () => {
    expect(nextNonoBlinkAt(1_000, () => 0)).toBe(9_000);
    expect(nextNonoBlinkAt(1_000, () => 0.5)).toBe(11_000);
    expect(nextNonoBlinkAt(1_000, () => 1)).toBe(13_000);
    expect(nonoBlinkInfluence(0)).toBe(0);
    expect(nonoBlinkInfluence(35)).toBe(0.5);
    expect(nonoBlinkInfluence(70)).toBe(1);
    expect(nonoBlinkInfluence(85)).toBe(1);
    expect(nonoBlinkInfluence(145)).toBe(0.5);
    expect(nonoBlinkInfluence(190)).toBe(0);
  });

  it("creates a skinned-compatible toon material while preserving texture and alpha settings", () => {
    const map = new THREE.Texture();
    const source = new THREE.MeshStandardMaterial({ map, transparent: true, opacity: 0.75 });
    source.name = "Nono_Hair";
    const material = createNonoToonMaterial(source, "hair", "nontoon");
    expect(material).toBeInstanceOf(THREE.MeshToonMaterial);
    expect(material.map).toBe(map);
    expect(material.transparent).toBe(true);
    expect(material.opacity).toBe(0.75);
    expect(material.gradientMap?.magFilter).toBe(THREE.NearestFilter);
    expect(material.customProgramCacheKey()).toContain("nontoon-hair-matte-v3");
    material.dispose();
    source.dispose();
    map.dispose();
  });

  it("preserves authored eye and mouth colors while keeping squint marks black", () => {
    const eyeSource = new THREE.MeshStandardMaterial({ color: 0x2f6ea8, transparent: true, opacity: 0.6, alphaTest: 0.2 });
    eyeSource.name = "Nono_Eyes";
    const mouthSource = new THREE.MeshStandardMaterial({ color: 0x8f3455 });
    mouthSource.name = "Nono_Lips";
    const squintSource = new THREE.MeshStandardMaterial({ color: 0xff70b7 });
    squintSource.name = "Nono_Squint_Black";

    const eye = createNonoToonMaterial(eyeSource, "eye", "toon");
    const mouth = createNonoToonMaterial(mouthSource, "mouth", "toon");
    const squint = createNonoToonMaterial(squintSource, "squint", "toon");
    expect(eye.color.getHex()).toBe(0x2f6ea8);
    expect(eye.transparent).toBe(true);
    expect(eye.opacity).toBe(0.6);
    expect(eye.alphaTest).toBe(0.2);
    expect(mouth.color.getHex()).toBe(0x8f3455);
    expect(mouth.customProgramCacheKey()).toContain("toon-mouth-glossy-v3");
    expect(squint.color.getHex()).toBe(0x000000);

    eye.dispose();
    mouth.dispose();
    squint.dispose();
    eyeSource.dispose();
    mouthSource.dispose();
    squintSource.dispose();
  });

  it("renders lips with the textured face material treatment and no gloss", () => {
    const faceTexture = new THREE.Texture();
    const faceSource = new THREE.MeshStandardMaterial({ color: 0xffffff, map: faceTexture });
    faceSource.name = "Nono_Face_Base";
    const lipsSource = new THREE.MeshStandardMaterial({ color: 0xed9e99 });
    lipsSource.name = "Nono_Lips";
    const faceMesh = new THREE.Mesh(new THREE.BoxGeometry(), faceSource);
    faceMesh.name = "Nono_Head_Face";
    const lipsMesh = new THREE.Mesh(new THREE.BoxGeometry(), lipsSource);
    lipsMesh.name = "Nono_Head_Lips";
    const model = new THREE.Group();
    model.add(faceMesh, lipsMesh);

    applyNonoMaterials(model, "toon");
    const face = faceMesh.material as unknown as THREE.MeshToonMaterial;
    const lips = lipsMesh.material as unknown as THREE.MeshToonMaterial;
    expect(face.map).toBe(faceTexture);
    expect(lips.map).toBe(faceTexture);
    expect(lips.color.getHex()).toBe(0xffffff);
    expect(lips.emissive.getHex()).toBe(0x000000);
    expect(lips.emissiveIntensity).toBe(0);
    expect(lips.userData.nonoSpecularStrength).toBe(0);

    face.dispose();
    lips.dispose();
    faceMesh.geometry.dispose();
    lipsMesh.geometry.dispose();
    faceTexture.dispose();
  });

  it("creates a parented skinned outline that shares geometry and skeleton deformation", () => {
    const geometry = new THREE.BoxGeometry();
    const vertexCount = geometry.getAttribute("position").count;
    geometry.setAttribute("skinIndex", new THREE.Uint16BufferAttribute(new Uint16Array(vertexCount * 4), 4));
    const weights = new Float32Array(vertexCount * 4);
    for (let index = 0; index < vertexCount; index += 1) weights[index * 4] = 1;
    geometry.setAttribute("skinWeight", new THREE.Float32BufferAttribute(weights, 4));
    const material = new THREE.MeshBasicMaterial();
    material.name = "Nono_Skin";
    const source = new THREE.SkinnedMesh(geometry, material);
    source.name = "Nono_Body";
    const bone = new THREE.Bone();
    source.add(bone);
    source.bind(new THREE.Skeleton([bone]));
    const scene = new THREE.Scene();
    scene.add(source);

    const outlines = createNonoOutlines(scene);
    expect(outlines).toHaveLength(1);
    const outline = outlines[0] as THREE.SkinnedMesh;
    expect(outline).toBeInstanceOf(THREE.SkinnedMesh);
    expect(outline.material).toBeInstanceOf(THREE.MeshBasicMaterial);
    expect((outline.material as THREE.MeshBasicMaterial).side).toBe(THREE.BackSide);
    expect((outline.material as THREE.Material).userData.nonoOutlineDepthBias).toBe(0.0002);
    expect((outline.material as THREE.Material).customProgramCacheKey()).toContain("outline-skin-v2");
    expect(outline.parent).toBe(source);
    expect(outline.geometry).toBe(source.geometry);
    expect(outline.skeleton).toBe(source.skeleton);

    (outline.material as THREE.Material).dispose();
    material.dispose();
    geometry.dispose();
  });

  it("shares morph influence and dictionary references with an outline", () => {
    const geometry = new THREE.BoxGeometry();
    geometry.morphAttributes.position = [geometry.getAttribute("position").clone()];
    const material = new THREE.MeshBasicMaterial();
    material.name = "Nono_Cloth";
    const source = new THREE.Mesh(geometry, material);
    source.name = "Nono_Jacket";
    source.updateMorphTargets();
    const scene = new THREE.Scene();
    scene.add(source);

    const [outline] = createNonoOutlines(scene) as THREE.Mesh[];
    expect(outline.morphTargetInfluences).toBe(source.morphTargetInfluences);
    expect(outline.morphTargetDictionary).toBe(source.morphTargetDictionary);

    (outline.material as THREE.Material).dispose();
    material.dispose();
    geometry.dispose();
  });

  it("adds the thin maroon mouth outline while sharing facial morph influences", () => {
    const geometry = new THREE.BoxGeometry();
    geometry.morphAttributes.position = [geometry.getAttribute("position").clone()];
    const material = new THREE.MeshBasicMaterial({ color: 0x9e383d });
    material.name = "Nono_Mouth";
    const source = new THREE.Mesh(geometry, material);
    source.name = "Nono_Head_Mouth";
    source.updateMorphTargets();
    const scene = new THREE.Scene();
    scene.add(source);

    const [outline] = createNonoOutlines(scene) as THREE.Mesh[];
    expect(outline).toBeDefined();
    expect((outline.material as THREE.MeshBasicMaterial).color.getHex()).toBe(NONO_OUTLINE_CONFIG.mouth.color);
    expect(NONO_OUTLINE_CONFIG.mouth.width).toBe(0.002);
    expect(outline.morphTargetInfluences).toBe(source.morphTargetInfluences);

    (outline.material as THREE.Material).dispose();
    material.dispose();
    geometry.dispose();
  });

  it("keeps the face ramp nearly flat without flattening body skin", () => {
    const source = new THREE.MeshStandardMaterial();
    source.name = "Nono_Face_Base";
    const face = createNonoToonMaterial(source, "face", "toon");
    const skin = createNonoToonMaterial(source, "skin", "toon");
    const faceRamp = (face.gradientMap!.image as { data: Uint8Array }).data;
    const skinRamp = (skin.gradientMap!.image as { data: Uint8Array }).data;

    expect(faceRamp[0]).toBeGreaterThanOrEqual(240);
    expect(skinRamp[0]).toBeLessThan(240);

    face.dispose();
    skin.dispose();
    source.dispose();
  });

  it("gives each hairclip a dedicated outline darkened from its source color", () => {
    const scene = new THREE.Scene();
    const hairMaterial = new THREE.MeshBasicMaterial({ color: 0x71ddea });
    hairMaterial.name = "Nono_Hair";
    const hair = new THREE.Mesh(new THREE.BoxGeometry(), hairMaterial);
    hair.name = "Nono_Hair";
    const clipMaterial = new THREE.MeshBasicMaterial({ color: 0x1cf745 });
    clipMaterial.name = "Nono_Hair_Clip_L";
    const clip = new THREE.Mesh(new THREE.BoxGeometry(), clipMaterial);
    clip.name = "Nono_Hair_Clip_R";
    scene.add(hair, clip);

    const outlines = createNonoOutlines(scene) as THREE.Mesh[];
    const hairOutline = outlines.find((outline) => outline.parent === hair)!;
    const clipOutline = outlines.find((outline) => outline.parent === clip)!;
    const outlinedColor = (clipOutline.material as THREE.MeshBasicMaterial).color;
    expect(clipOutline.material).not.toBe(hairOutline.material);
    expect(outlinedColor.r).toBeCloseTo(clipMaterial.color.r * 0.16);
    expect(outlinedColor.g).toBeCloseTo(clipMaterial.color.g * 0.16);
    expect(outlinedColor.b).toBeCloseTo(clipMaterial.color.b * 0.16);
    expect((clipOutline.material as THREE.Material).userData.nonoOutlineWidth).toBe(0.010);
    expect((clipOutline.material as THREE.Material).userData.nonoOutlineDepthBias).toBe(0);
    expect((clipOutline.material as THREE.Material).customProgramCacheKey()).toContain("outline-clip-v2");

    (hairOutline.material as THREE.Material).dispose();
    (clipOutline.material as THREE.Material).dispose();
    hairMaterial.dispose();
    clipMaterial.dispose();
    hair.geometry.dispose();
    clip.geometry.dispose();
  });

  it("skips eyes, the squint mesh, transparent materials, and unknown roles", () => {
    const scene = new THREE.Scene();
    const addMesh = (name: string, materialName: string, transparent = false) => {
      const material = new THREE.MeshBasicMaterial({ transparent });
      material.name = materialName;
      const mesh = new THREE.Mesh(new THREE.BoxGeometry(), material);
      mesh.name = name;
      scene.add(mesh);
    };
    addMesh("Nono_Eyes", "Nono_Eye");
    addMesh("Nono_Squint", "Nono_Skin");
    addMesh("Nono_Hair", "Nono_Hair", true);
    addMesh("Nono_Mystery", "Mystery");

    expect(createNonoOutlines(scene)).toEqual([]);
    scene.traverse((object) => {
      if (!(object instanceof THREE.Mesh)) return;
      object.geometry.dispose();
      (object.material as THREE.Material).dispose();
    });
  });

  it("allows the outline dev kill-switch without changing release behavior", () => {
    expect(nonoOutlineFromLocation("?nonoOutline=0", true)).toBe(false);
    expect(nonoOutlineFromLocation("", true)).toBe(true);
    expect(nonoOutlineFromLocation("?nonoOutline=0", false)).toBe(true);
  });
});
