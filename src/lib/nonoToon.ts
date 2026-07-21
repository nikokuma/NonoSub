import * as THREE from "three";

export type NonoShaderVariant = "toon" | "nontoon" | "portable";

export const NONO_MOODS = [
  "idle",
  "think",
  "neutral",
  "thumbs_up",
  "point_user",
  "point_self",
  "cheer",
  "heart_touch",
  "surprised",
] as const;
export type NonoMood = (typeof NONO_MOODS)[number];
export type NonoMaterialRole = "skin" | "face" | "hair" | "tail" | "cloth" | "eye" | "metal" | "accent" | "unknown";

const ROLE_COLORS: Record<Exclude<NonoMaterialRole, "unknown">, number> = {
  skin: 0xf1c7b2,
  face: 0xffffff,
  hair: 0x71ddea,
  tail: 0x122348,
  cloth: 0xffffff,
  eye: 0xffffff,
  metal: 0xbfc6d4,
  accent: 0xf18fbc,
};

const gradients = new Map<string, THREE.DataTexture>();

export function inferNonoMaterialRole(materialName: string, objectName = ""): NonoMaterialRole {
  const name = `${materialName} ${objectName}`.toLowerCase();
  if (/eye|iris|pupil|moon|shine/.test(name)) return "eye";
  if (/face/.test(name)) return "face";
  if (/body|skin/.test(name)) return "skin";
  if (/hair/.test(name)) return "hair";
  if (/tail(?!oring)|cable/.test(name)) return "tail";
  if (/metal|pin_rim|button|ring|zip/.test(name)) return "metal";
  if (/pink|bow|enamel|spark|accent/.test(name)) return "accent";
  if (/blazer|shirt|skirt|sock|shoe|cloth|jacket|seam|thread|short/.test(name)) return "cloth";
  return "unknown";
}

export function shaderVariantFromLocation(search: string, development = import.meta.env.DEV): NonoShaderVariant {
  if (!development) return "toon";
  const requested = new URLSearchParams(search).get("nonoShader");
  return requested === "nontoon" || requested === "portable" ? requested : "toon";
}

export function nonoAssetFromLocation(search: string, development = import.meta.env.DEV): string {
  if (!development) return "/assets/Nono.glb";
  return new URLSearchParams(search).get("nonoAsset") === "candidate"
    ? "/assets/NonoCandidate.glb"
    : "/assets/Nono.glb";
}

export function nonoMoodFromLocation(search: string, development = import.meta.env.DEV): NonoMood | undefined {
  if (!development) return undefined;
  const requested = new URLSearchParams(search).get("nonoMood");
  return (NONO_MOODS as readonly string[]).includes(requested ?? "") ? requested as NonoMood : undefined;
}

export function applyNonoMaterials(model: THREE.Object3D, variant: NonoShaderVariant): THREE.Material[] {
  if (variant === "portable") return [];
  const replaced: THREE.Material[] = [];
  model.traverse((object) => {
    if (!(object instanceof THREE.Mesh)) return;
    const originals = Array.isArray(object.material) ? object.material : [object.material];
    const next = originals.map((original) => {
      const role = inferNonoMaterialRole(original.name, object.name);
      if (role === "unknown") return original;
      const material = createNonoToonMaterial(original, role, variant);
      original.dispose();
      replaced.push(material);
      return material;
    });
    object.material = Array.isArray(object.material) ? next : next[0];
  });
  return replaced;
}

export function createNonoToonMaterial(
  source: THREE.Material,
  role: Exclude<NonoMaterialRole, "unknown">,
  variant: Exclude<NonoShaderVariant, "portable">,
): THREE.MeshToonMaterial {
  const standard = source as THREE.MeshStandardMaterial;
  const paletteColor = new THREE.Color(ROLE_COLORS[role]);
  const sourceColor = "color" in standard && standard.color instanceof THREE.Color ? standard.color : undefined;
  const useSourceColor = sourceColor && sourceColor.getHex() !== 0x000000 && sourceColor.getHex() !== 0xffffff;
  const color = role === "face" || role === "eye" ? new THREE.Color(0xffffff) : useSourceColor ? sourceColor!.clone() : paletteColor;
  const material = new THREE.MeshToonMaterial({
    name: `${source.name || role}__${variant}`,
    color,
    map: standard.map ?? null,
    alphaMap: standard.alphaMap ?? null,
    normalMap: standard.normalMap ?? null,
    transparent: source.transparent,
    opacity: source.opacity,
    alphaTest: source.alphaTest,
    side: source.side,
    depthWrite: source.depthWrite,
    depthTest: source.depthTest,
    vertexColors: "vertexColors" in standard ? standard.vertexColors : false,
    gradientMap: gradientFor(role, variant),
  });
  material.emissive.copy(role === "eye" ? new THREE.Color(0x314870) : role === "accent" ? new THREE.Color(0x2a1020) : new THREE.Color(0x000000));
  material.emissiveIntensity = role === "eye" ? 0.32 : role === "accent" ? 0.12 : 0;
  material.emissiveMap = standard.emissiveMap ?? null;
  installNonoShaderPatch(material, role, variant);
  material.customProgramCacheKey = () => `nono-${variant}-${role}-v1`;
  return material;
}

export function disposeNonoMaterials(materials: readonly THREE.Material[]): void {
  for (const material of materials) material.dispose();
}

function gradientFor(role: Exclude<NonoMaterialRole, "unknown">, variant: Exclude<NonoShaderVariant, "portable">): THREE.DataTexture {
  const profile = role === "skin" || role === "face"
    ? (variant === "nontoon" ? [112, 166, 236, 255] : [166, 166, 238, 255])
    : role === "hair" || role === "tail"
      ? (variant === "nontoon" ? [54, 104, 190, 255] : [82, 82, 176, 255])
      : variant === "nontoon" ? [72, 142, 226, 255] : [112, 112, 214, 255];
  const key = profile.join("-");
  const existing = gradients.get(key);
  if (existing) return existing;
  const texture = new THREE.DataTexture(new Uint8Array(profile), profile.length, 1, THREE.RedFormat);
  texture.minFilter = THREE.NearestFilter;
  texture.magFilter = THREE.NearestFilter;
  texture.generateMipmaps = false;
  texture.needsUpdate = true;
  gradients.set(key, texture);
  return texture;
}

/**
 * NonToon is a Unity/HLSL shader by lilxyzw. The experimental branch below
 * independently adapts its small ramp/rim/hair-specular concepts for Three.js;
 * it is deliberately not a full or source-compatible port.
 */
function installNonoShaderPatch(
  material: THREE.MeshToonMaterial,
  role: Exclude<NonoMaterialRole, "unknown">,
  variant: Exclude<NonoShaderVariant, "portable">,
): void {
  const rimStrength = role === "hair" || role === "tail" ? 0.22 : role === "metal" || role === "eye" ? 0.18 : 0.09;
  const hairStrength = variant === "nontoon" && role === "hair" ? 0.3 : 0;
  const specularStrength = variant === "nontoon" && (role === "metal" || role === "eye") ? 0.22 : 0;
  material.onBeforeCompile = (shader) => {
    shader.uniforms.nonoRimColor = { value: new THREE.Color(role === "hair" ? 0xbef7ff : 0xffdff0) };
    shader.uniforms.nonoRimStrength = { value: rimStrength };
    shader.uniforms.nonoHairStrength = { value: hairStrength };
    shader.uniforms.nonoSpecularStrength = { value: specularStrength };
    shader.vertexShader = shader.vertexShader
      .replace("#include <common>", `#include <common>\nvarying vec3 vNonoWorldNormal;\nvarying vec3 vNonoViewDirection;`)
      .replace("#include <worldpos_vertex>", `#include <worldpos_vertex>\nvec3 nonoWorldPosition = (modelMatrix * vec4(transformed, 1.0)).xyz;\nvNonoWorldNormal = normalize(mat3(modelMatrix) * transformedNormal);\nvNonoViewDirection = normalize(cameraPosition - nonoWorldPosition);`);
    shader.fragmentShader = shader.fragmentShader
      .replace("#include <common>", `#include <common>\nuniform vec3 nonoRimColor;\nuniform float nonoRimStrength;\nuniform float nonoHairStrength;\nuniform float nonoSpecularStrength;\nvarying vec3 vNonoWorldNormal;\nvarying vec3 vNonoViewDirection;`)
      .replace("#include <opaque_fragment>", `
        vec3 nonoNormal = normalize(vNonoWorldNormal);
        vec3 nonoView = normalize(vNonoViewDirection);
        float nonoFresnel = pow(1.0 - clamp(dot(nonoNormal, nonoView), 0.0, 1.0), 2.4);
        outgoingLight += nonoRimColor * nonoFresnel * nonoRimStrength;
        float nonoHairBand = smoothstep(0.70, 0.91, 1.0 - abs(dot(nonoNormal, normalize(nonoView + vec3(0.0, 0.65, 0.0)))));
        outgoingLight += mix(vec3(0.72, 0.94, 1.0), diffuseColor.rgb, 0.2) * nonoHairBand * nonoHairStrength;
        float nonoSpecular = pow(max(dot(reflect(-nonoView, nonoNormal), normalize(vec3(0.25, 0.8, 0.45))), 0.0), 28.0);
        outgoingLight += vec3(nonoSpecular * nonoSpecularStrength);
        #include <opaque_fragment>
      `);
  };
}
