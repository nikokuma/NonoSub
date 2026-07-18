import { describe, expect, it } from "vitest";
import * as THREE from "three";
import { createNonoToonMaterial, inferNonoMaterialRole, nonoAssetFromLocation, shaderVariantFromLocation } from "./nonoToon";

describe("Nono toon materials", () => {
  it("maps stable semantic and legacy names to material roles", () => {
    expect(inferNonoMaterialRole("Nono_Hair")).toBe("hair");
    expect(inferNonoMaterialRole("Material.001", "Nono_Hair_Long")).toBe("hair");
    expect(inferNonoMaterialRole("Nono_TailCable")).toBe("tail");
    expect(inferNonoMaterialRole("TeacherJacket_P5_Pin_Rim_PBR")).toBe("metal");
    expect(inferNonoMaterialRole("mystery")).toBe("unknown");
  });

  it("keeps the release variant fixed while allowing a development comparison", () => {
    expect(shaderVariantFromLocation("?nonoShader=nontoon", true)).toBe("nontoon");
    expect(shaderVariantFromLocation("?nonoShader=portable", true)).toBe("portable");
    expect(shaderVariantFromLocation("?nonoShader=nontoon", false)).toBe("toon");
    expect(nonoAssetFromLocation("?nonoAsset=candidate", true)).toBe("/assets/NonoCandidate.glb");
    expect(nonoAssetFromLocation("?nonoAsset=candidate", false)).toBe("/assets/Nono.glb");
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
});
