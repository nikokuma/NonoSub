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
export type NonoExpression = "squint";
export type NonoMaterialRole = "skin" | "face" | "hair" | "tail" | "cloth" | "eye" | "mouth" | "squint" | "metal" | "accent" | "unknown";

type NonoOutlineRole = Extract<NonoMaterialRole, "skin" | "face" | "hair" | "tail" | "cloth" | "mouth" | "metal" | "accent">;

export const NONO_OUTLINE_CONFIG: Record<NonoOutlineRole, { color: number; width: number }> = {
  skin: { color: 0x3a2028, width: 0.006 },
  face: { color: 0x3a2028, width: 0.006 },
  hair: { color: 0x0e2f38, width: 0.008 },
  tail: { color: 0x0a1226, width: 0.006 },
  cloth: { color: 0x14141c, width: 0.007 },
  mouth: { color: 0x4a1420, width: 0.002 },
  metal: { color: 0x23262e, width: 0.004 },
  accent: { color: 0x4a1e33, width: 0.004 },
};

const ROLE_COLORS: Record<Exclude<NonoMaterialRole, "unknown">, number> = {
  skin: 0xf1c7b2,
  face: 0xffffff,
  hair: 0x71ddea,
  tail: 0x122348,
  cloth: 0xffffff,
  eye: 0xffffff,
  mouth: 0x6b263f,
  squint: 0x000000,
  metal: 0xbfc6d4,
  accent: 0xf18fbc,
};

const gradients = new Map<string, THREE.DataTexture>();
const outlineMaterials = new Map<NonoOutlineRole, THREE.MeshBasicMaterial>();
// Clip-space push ≈ world-units * 2*far*near / (d^2 * (far-near)) at view distance d;
// with the 0.1/100 frustum and Nono ~4.8 units away, 0.0002 ≈ a 2.3cm push — enough
// for real geometry (ahoge, sweeps, clips) to beat a hull, small enough to keep
// interior outlines (chin, arms against skirt).
const NONO_OUTLINE_DEPTH_BIAS = 0.0002;

const SHADE_TINTS: Record<Exclude<NonoMaterialRole, "unknown">, number> = {
  skin: 0xeedbe0,
  face: 0xf7edf0,
  hair: 0x8fa8d9,
  tail: 0x9aa4cc,
  cloth: 0xa8a4c4,
  eye: 0xc4a8bc,
  mouth: 0xc4a8bc,
  squint: 0xffffff,
  metal: 0xaab0c0,
  accent: 0xc4a8bc,
};

export function inferNonoMaterialRole(materialName: string, objectName = ""): NonoMaterialRole {
  const name = `${materialName} ${objectName}`.toLowerCase();
  if (/squint/.test(name)) return "squint";
  if (/mouth|lips?/.test(name)) return "mouth";
  if (/eye|iris|pupil|moon|shine/.test(name)) return "eye";
  if (/face/.test(name)) return "face";
  if (/body|skin/.test(name)) return "skin";
  if (/hair/.test(name)) return "hair";
  if (/tail(?!oring)|cable/.test(name)) return "tail";
  if (/metal|pin_rim|button|ring|zip/.test(name)) return "metal";
  if (/blazer|shirt|skirt|sock|shoe|cloth|jacket|seam|thread|short/.test(name)) return "cloth";
  if (/pink|bow|enamel|spark|accent/.test(name)) return "accent";
  return "unknown";
}

export function nonoMoodFromCue(gesture?: string): NonoMood | undefined {
  return (NONO_MOODS as readonly string[]).includes(gesture ?? "") ? gesture as NonoMood : undefined;
}

export function nextNonoBlinkAt(nowMs: number, random = Math.random): number {
  return nowMs + 8_000 + THREE.MathUtils.clamp(random(), 0, 1) * 4_000;
}

export function nonoBlinkInfluence(elapsedMs: number): number {
  if (elapsedMs <= 0) return 0;
  if (elapsedMs < 70) return elapsedMs / 70;
  if (elapsedMs < 100) return 1;
  if (elapsedMs < 190) return 1 - (elapsedMs - 100) / 90;
  return 0;
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

export function nonoExpressionFromLocation(search: string, development = import.meta.env.DEV): NonoExpression | undefined {
  if (!development) return undefined;
  return new URLSearchParams(search).get("nonoExpression") === "squint" ? "squint" : undefined;
}

export function nonoOutlineFromLocation(search: string, development = import.meta.env.DEV): boolean {
  if (!development) return true;
  return new URLSearchParams(search).get("nonoOutline") !== "0";
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
  const faceMap = replaced.find((material) => (
    material instanceof THREE.MeshToonMaterial
    && /face_base/i.test(material.name)
    && material.map
  )) as THREE.MeshToonMaterial | undefined;
  for (const material of replaced) {
    if (!(material instanceof THREE.MeshToonMaterial) || !/lips/i.test(material.name)) continue;
    material.map = faceMap?.map ?? null;
    material.color.set(faceMap?.map ? 0xffffff : 0xf6ddd3);
    material.emissive.set(0x000000);
    material.emissiveIntensity = 0;
    material.emissiveMap = null;
    material.userData.nonoSpecularStrength = 0;
    material.needsUpdate = true;
  }
  return replaced;
}

export function createNonoOutlines(model: THREE.Object3D): THREE.Object3D[] {
  const sources: Array<{ mesh: THREE.Mesh; role: NonoOutlineRole }> = [];
  model.traverse((object) => {
    if (!(object instanceof THREE.Mesh) || object.name === "Nono_Squint" || object.userData.nonoOutline) return;
    const materials = Array.isArray(object.material) ? object.material : [object.material];
    if (materials.length === 0 || materials.some((material) => material.transparent)) return;
    const roles = materials.map((material) => inferNonoMaterialRole(material.name, object.name));
    const role = roles[0];
    if (!isNonoOutlineRole(role) || roles.some((candidate) => candidate !== role)) return;
    sources.push({ mesh: object, role });
  });

  return sources.map(({ mesh: source, role }) => {
    const material = isHairClip(source)
      ? clipOutlineMaterialFor(source)
      : outlineMaterialFor(role);
    const outline = source instanceof THREE.SkinnedMesh
      ? new THREE.SkinnedMesh(source.geometry, material)
      : new THREE.Mesh(source.geometry, material);
    outline.name = `${source.name}__outline`;
    outline.frustumCulled = false;
    outline.userData.nonoOutline = true;
    if (source.morphTargetInfluences) outline.morphTargetInfluences = source.morphTargetInfluences;
    if (source.morphTargetDictionary) outline.morphTargetDictionary = source.morphTargetDictionary;
    source.add(outline);
    if (outline instanceof THREE.SkinnedMesh && source instanceof THREE.SkinnedMesh) {
      outline.bindMode = source.bindMode;
      outline.bind(source.skeleton, source.bindMatrix);
    }
    return outline;
  });
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
  const meaningfulEyeColor = sourceColor && sourceColor.getHex() !== 0xffffff ? sourceColor : undefined;
  const color = role === "squint"
    ? new THREE.Color(0x000000)
    : role === "mouth"
      ? sourceColor?.clone() ?? paletteColor
      : role === "face"
        ? new THREE.Color(0xffffff)
        : role === "eye"
          ? meaningfulEyeColor?.clone() ?? (standard.map ? new THREE.Color(0xffffff) : sourceColor?.clone() ?? paletteColor)
          : role === "skin" && standard.map
            ? new THREE.Color(0xffffff)
            : useSourceColor ? sourceColor!.clone() : paletteColor;
  const glossyMouth = role === "mouth" && /lips?/.test(source.name.toLowerCase());
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
  installNonoShaderPatch(material, role, variant, glossyMouth);
  material.customProgramCacheKey = () => `nono-${variant}-${role}-${glossyMouth ? "glossy" : "matte"}-v3`;
  return material;
}

export function disposeNonoMaterials(materials: readonly THREE.Material[]): void {
  for (const material of materials) material.dispose();
}

function gradientFor(role: Exclude<NonoMaterialRole, "unknown">, variant: Exclude<NonoShaderVariant, "portable">): THREE.DataTexture {
  const profile = role === "face"
    ? [240, 244, 248, 255]
    : role === "skin"
      ? (variant === "nontoon" ? [112, 166, 236, 255] : [212, 212, 244, 255])
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

function isNonoOutlineRole(role: NonoMaterialRole): role is NonoOutlineRole {
  return role in NONO_OUTLINE_CONFIG;
}

function outlineMaterialFor(role: NonoOutlineRole): THREE.MeshBasicMaterial {
  const existing = outlineMaterials.get(role);
  if (existing) return existing;
  const config = NONO_OUTLINE_CONFIG[role];
  const material = createOutlineMaterial(`Nono_${role}__outline`, new THREE.Color(config.color), config.width, `nono-outline-${role}-v2`);
  outlineMaterials.set(role, material);
  return material;
}

function isHairClip(source: THREE.Mesh): boolean {
  const materials = Array.isArray(source.material) ? source.material : [source.material];
  return /clip/i.test(`${source.name} ${materials.map((material) => material.name).join(" ")}`);
}

function clipOutlineMaterialFor(source: THREE.Mesh): THREE.MeshBasicMaterial {
  const materials = Array.isArray(source.material) ? source.material : [source.material];
  const sourceColor = materials.find((material) => "color" in material && material.color instanceof THREE.Color);
  const color = sourceColor && "color" in sourceColor && sourceColor.color instanceof THREE.Color
    ? sourceColor.color.clone().multiplyScalar(0.16)
    : new THREE.Color(NONO_OUTLINE_CONFIG.hair.color);
  // Clips are thin decals lying on the hair surface: any depth push sends their
  // hull behind the hair, erasing the rim. Bias 0 — the biased hair hull loses
  // to the clip outline instead.
  return createOutlineMaterial(`${source.name || "Nono_Hair_Clip"}__outline`, color, 0.010, "nono-outline-clip-v2", 0);
}

function createOutlineMaterial(
  name: string,
  color: THREE.Color,
  width: number,
  cacheKey: string,
  depthBias = NONO_OUTLINE_DEPTH_BIAS,
): THREE.MeshBasicMaterial {
  const material = new THREE.MeshBasicMaterial({ name, color, side: THREE.BackSide });
  material.userData.nonoOutlineWidth = width;
  material.userData.nonoOutlineDepthBias = depthBias;
  material.onBeforeCompile = (shader) => {
    shader.uniforms.uNonoOutlineWidth = { value: width };
    shader.uniforms.uNonoOutlineDepthBias = { value: depthBias };
    shader.vertexShader = shader.vertexShader
      .replace("#include <common>", "#include <common>\nuniform float uNonoOutlineWidth;\nuniform float uNonoOutlineDepthBias;")
      .replace("#include <begin_vertex>", "vec3 transformed = vec3( position ) + normal * uNonoOutlineWidth;")
      .replace("#include <project_vertex>", "#include <project_vertex>\ngl_Position.z += uNonoOutlineDepthBias * gl_Position.w;");
  };
  material.customProgramCacheKey = () => cacheKey;
  return material;
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
  glossyMouth: boolean,
): void {
  const rimStrength = role === "squint" ? 0 : role === "hair" || role === "tail" ? 0.28 : role === "metal" || role === "eye" ? 0.18 : 0.09;
  const hairStrength = role === "hair" ? variant === "nontoon" ? 0.3 : 0.18 : 0;
  const specularStrength = glossyMouth ? 0.12 : variant === "nontoon" && (role === "metal" || role === "eye") ? 0.22 : 0;
  material.userData.nonoSpecularStrength = specularStrength;
  material.onBeforeCompile = (shader) => {
    shader.uniforms.nonoRimColor = { value: new THREE.Color(role === "hair" ? 0xbef7ff : 0xffdff0) };
    shader.uniforms.nonoRimStrength = { value: rimStrength };
    shader.uniforms.nonoHairStrength = { value: hairStrength };
    shader.uniforms.nonoSpecularStrength = { value: material.userData.nonoSpecularStrength };
    shader.uniforms.nonoShadeTint = { value: new THREE.Color(SHADE_TINTS[role]) };
    shader.vertexShader = shader.vertexShader
      .replace("#include <common>", `#include <common>\nvarying vec3 vNonoWorldNormal;\nvarying vec3 vNonoViewDirection;`)
      .replace("#include <worldpos_vertex>", `#include <worldpos_vertex>\nvec3 nonoWorldPosition = (modelMatrix * vec4(transformed, 1.0)).xyz;\nvNonoWorldNormal = normalize(mat3(modelMatrix) * transformedNormal);\nvNonoViewDirection = normalize(cameraPosition - nonoWorldPosition);`);
    shader.fragmentShader = shader.fragmentShader
      .replace("#include <common>", `#include <common>\nuniform vec3 nonoRimColor;\nuniform float nonoRimStrength;\nuniform float nonoHairStrength;\nuniform float nonoSpecularStrength;\nuniform vec3 nonoShadeTint;\nvarying vec3 vNonoWorldNormal;\nvarying vec3 vNonoViewDirection;`)
      .replace("#include <gradientmap_pars_fragment>", `
        #ifdef USE_GRADIENTMAP
          uniform sampler2D gradientMap;
        #endif

        vec3 getGradientIrradiance( vec3 normal, vec3 lightDirection ) {
          float dotNL = dot( normal, lightDirection );
          vec2 coord = vec2( dotNL * 0.5 + 0.5, 0.0 );
          #ifdef USE_GRADIENTMAP
            float nonoGradientStep = texture2D( gradientMap, coord ).r;
          #else
            vec2 fw = fwidth( coord ) * 0.5;
            float nonoGradientStep = mix( 0.7, 1.0, smoothstep( 0.7 - fw.x, 0.7 + fw.x, coord.x ) );
          #endif
          return mix( nonoShadeTint, vec3( 1.0 ), nonoGradientStep );
        }
      `)
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
