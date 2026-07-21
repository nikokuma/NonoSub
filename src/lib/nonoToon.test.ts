import { describe, expect, it } from "vitest";
import * as THREE from "three";
import {
  createNonoToonMaterial,
  inferNonoMaterialRole,
  nextNonoBlinkAt,
  nonoAssetFromLocation,
  nonoBlinkInfluence,
  nonoExpressionFromLocation,
  nonoMoodFromCue,
  nonoMoodFromLocation,
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
    expect(material.customProgramCacheKey()).toContain("nontoon-hair");
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
    expect(mouth.customProgramCacheKey()).toContain("glossy");
    expect(squint.color.getHex()).toBe(0x000000);

    eye.dispose();
    mouth.dispose();
    squint.dispose();
    eyeSource.dispose();
    mouthSource.dispose();
    squintSource.dispose();
  });
});
