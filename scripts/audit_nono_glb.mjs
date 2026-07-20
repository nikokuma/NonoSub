import { readFileSync } from "node:fs";
import { resolve } from "node:path";

const args = process.argv.slice(2);
const allowMissingAnimations = args.includes("--allow-missing-animations");
const suppliedPath = args.find((arg) => !arg.startsWith("--"));
const path = resolve(suppliedPath ?? "static/assets/Nono.glb");
const bytes = readFileSync(path);
const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);

if (bytes.toString("ascii", 0, 4) !== "glTF" || view.getUint32(4, true) !== 2) {
  throw new Error(`${path} is not a glTF 2 binary.`);
}

let offset = 12;
let document;
let binary;
while (offset + 8 <= bytes.byteLength) {
  const length = view.getUint32(offset, true);
  const type = view.getUint32(offset + 4, true);
  offset += 8;
  if (type === 0x4e4f534a) {
    const json = bytes.toString("utf8", offset, offset + length).replace(/\0+$/u, "");
    document = JSON.parse(json);
  } else if (type === 0x004e4942) {
    binary = bytes.subarray(offset, offset + length);
  }
  offset += length;
}

if (!document) throw new Error(`${path} has no JSON chunk.`);
if (!binary) throw new Error(`${path} has no binary chunk.`);

const errors = [];
const warnings = [];
const nodes = document.nodes ?? [];
const meshes = document.meshes ?? [];
const skins = document.skins ?? [];
const animations = document.animations ?? [];
const materials = document.materials ?? [];
const accessors = document.accessors ?? [];
const bufferViews = document.bufferViews ?? [];
const nodeNames = new Set(nodes.map((node) => node.name).filter(Boolean));
const namedNodeIndices = new Map(nodes.map((node, index) => [node.name, index]).filter(([name]) => Boolean(name)));

const currentLeft = Array.from({ length: 12 }, (_, index) => `spine.${String(55 + index).padStart(3, "0")}`);
const currentRight = Array.from({ length: 12 }, (_, index) => `spine.${String(67 + index).padStart(3, "0")}`);
const semanticLeft = Array.from({ length: 12 }, (_, index) => `tail.L.${String(index + 1).padStart(2, "0")}`);
const semanticRight = Array.from({ length: 12 }, (_, index) => `tail.R.${String(index + 1).padStart(2, "0")}`);
const dynamicHairRoots = ["spine.021", "spine.031", "spine.039", "spine.085", "spine.093"];
const hasChain = (names) => names.every((name) => nodeNames.has(name));

if (skins.length !== 1) errors.push(`Expected one canonical skin/armature, found ${skins.length}.`);
if (!nodeNames.has("Nono_Rig")) errors.push("Missing canonical Nono_Rig node.");
if (nodes.some((node) => /^SOURCE_/u.test(node.name ?? ""))) errors.push("SOURCE_ONLY object leaked into the GLB.");
if (!hasChain(currentLeft) && !hasChain(semanticLeft)) errors.push("Missing complete left 12-bone tail chain.");
if (!hasChain(currentRight) && !hasChain(semanticRight)) errors.push("Missing complete right 12-bone tail chain.");
for (const root of dynamicHairRoots) {
  if (!nodeNames.has(root)) errors.push(`Missing dynamic hair root ${root}.`);
}

const tailNode = nodes.find((node) => node.name === "Nono_Tails");
if (!tailNode) errors.push("Missing Nono_Tails node.");
else {
  if (tailNode.skin === undefined) errors.push("Nono_Tails is not connected to a glTF skin.");
  const mesh = tailNode.mesh === undefined ? undefined : meshes[tailNode.mesh];
  if (!mesh) errors.push("Nono_Tails has no mesh.");
  else if (!(mesh.primitives ?? []).every((primitive) => primitive.attributes?.JOINTS_0 !== undefined && primitive.attributes?.WEIGHTS_0 !== undefined)) {
    errors.push("Nono_Tails primitives are missing JOINTS_0 or WEIGHTS_0.");
  }
}

const REQUIRED_CLIPS = ["idle", "think", "neutral", "thumbs_up"];
const OPTIONAL_CLIPS = ["point_user", "point_self", "cheer", "hand_over_mouth", "surprised"];
const animationNames = new Set(animations.map((animation) => animation.name?.toLowerCase()).filter(Boolean));
for (const clip of REQUIRED_CLIPS) {
  if (!animationNames.has(clip) && !allowMissingAnimations) {
    errors.push(`Missing ${clip} animation clip.`);
  }
}
for (const clip of OPTIONAL_CLIPS) {
  if (!animationNames.has(clip)) warnings.push(`Optional gesture clip ${clip} not shipped; app falls back.`);
}
for (const name of animationNames) {
  if (!REQUIRED_CLIPS.includes(name) && !OPTIONAL_CLIPS.includes(name)) {
    warnings.push(`Animation clip "${name}" is outside the known mood set and will never play.`);
  }
}
if (allowMissingAnimations && animations.length === 0) {
  warnings.push("Idle/Think/Neutral/Thumbs_Up are intentionally pending Nico's suit-animation capture.");
}

const childrenByNode = nodes.map((node) => node.children ?? []);
function descendants(rootIndex) {
  const result = new Set([rootIndex]);
  const pending = [rootIndex];
  while (pending.length > 0) {
    const current = pending.pop();
    for (const child of childrenByNode[current] ?? []) {
      if (!result.has(child)) {
        result.add(child);
        pending.push(child);
      }
    }
  }
  return result;
}

const proceduralNodes = new Set();
for (const name of [...currentLeft, ...currentRight, ...semanticLeft, ...semanticRight]) {
  const index = namedNodeIndices.get(name);
  if (index !== undefined) proceduralNodes.add(index);
}
for (const root of dynamicHairRoots) {
  const index = namedNodeIndices.get(root);
  if (index !== undefined) for (const descendant of descendants(index)) proceduralNodes.add(descendant);
}
for (const animation of animations) {
  const forbidden = (animation.channels ?? []).map((channel) => channel.target?.node).filter((index) => proceduralNodes.has(index));
  if (forbidden.length > 0) {
    const names = [...new Set(forbidden.map((index) => nodes[index]?.name ?? `node ${index}`))];
    errors.push(`${animation.name} keys procedural tail/hair bones: ${names.join(", ")}.`);
  }
}

const componentReaders = {
  5120: { bytes: 1, read: (data, at) => data.getInt8(at), normalize: (value) => Math.max(value / 127, -1) },
  5121: { bytes: 1, read: (data, at) => data.getUint8(at), normalize: (value) => value / 255 },
  5122: { bytes: 2, read: (data, at) => data.getInt16(at, true), normalize: (value) => Math.max(value / 32767, -1) },
  5123: { bytes: 2, read: (data, at) => data.getUint16(at, true), normalize: (value) => value / 65535 },
  5125: { bytes: 4, read: (data, at) => data.getUint32(at, true), normalize: (value) => value / 4294967295 },
  5126: { bytes: 4, read: (data, at) => data.getFloat32(at, true), normalize: (value) => value },
};
const componentsPerType = { SCALAR: 1, VEC2: 2, VEC3: 3, VEC4: 4, MAT2: 4, MAT3: 9, MAT4: 16 };
const binaryView = new DataView(binary.buffer, binary.byteOffset, binary.byteLength);

function readAccessor(index) {
  const accessor = accessors[index];
  if (!accessor) throw new Error(`Missing accessor ${index}.`);
  if (accessor.sparse) throw new Error(`Sparse accessor ${index} is unsupported by the asset audit.`);
  const bufferView = bufferViews[accessor.bufferView];
  if (!bufferView) throw new Error(`Accessor ${index} has no buffer view.`);
  const reader = componentReaders[accessor.componentType];
  const components = componentsPerType[accessor.type];
  if (!reader || !components) throw new Error(`Accessor ${index} uses unsupported format.`);
  const packedStride = reader.bytes * components;
  const stride = bufferView.byteStride ?? packedStride;
  const start = (bufferView.byteOffset ?? 0) + (accessor.byteOffset ?? 0);
  const values = [];
  for (let row = 0; row < accessor.count; row += 1) {
    const tuple = [];
    for (let column = 0; column < components; column += 1) {
      const raw = reader.read(binaryView, start + row * stride + column * reader.bytes);
      tuple.push(accessor.normalized ? reader.normalize(raw) : raw);
    }
    values.push(tuple);
  }
  return values;
}

let drawCalls = 0;
let triangles = 0;
const skinnedNodeCount = nodes.filter((node) => node.skin !== undefined && node.mesh !== undefined).length;
for (const [nodeIndex, node] of nodes.entries()) {
  if (node.mesh === undefined) continue;
  const mesh = meshes[node.mesh];
  if (!mesh) {
    errors.push(`${node.name ?? `node ${nodeIndex}`} references a missing mesh.`);
    continue;
  }
  if ((node.name ?? "").startsWith("Nono_") && node.skin === undefined) {
    errors.push(`${node.name} is an unskinned exported character mesh.`);
  }
  const skin = node.skin === undefined ? undefined : skins[node.skin];
  for (const primitive of mesh.primitives ?? []) {
    drawCalls += 1;
    const mode = primitive.mode ?? 4;
    if (mode !== 4) errors.push(`${node.name ?? mesh.name} uses non-triangle primitive mode ${mode}.`);
    const triangleAccessor = primitive.indices ?? primitive.attributes?.POSITION;
    const elementCount = accessors[triangleAccessor]?.count ?? 0;
    triangles += Math.floor(elementCount / 3);

    const attributes = primitive.attributes ?? {};
    if (node.skin !== undefined) {
      if (attributes.JOINTS_0 === undefined || attributes.WEIGHTS_0 === undefined) {
        errors.push(`${node.name ?? mesh.name} has a skin but lacks JOINTS_0/WEIGHTS_0.`);
        continue;
      }
      if (attributes.JOINTS_1 !== undefined || attributes.WEIGHTS_1 !== undefined) {
        errors.push(`${node.name ?? mesh.name} exports more than four vertex influences.`);
      }
      const joints = readAccessor(attributes.JOINTS_0);
      const weights = readAccessor(attributes.WEIGHTS_0);
      if (joints.length !== weights.length) errors.push(`${node.name ?? mesh.name} joint/weight counts differ.`);
      let invalidWeights = 0;
      let invalidJoints = 0;
      for (let index = 0; index < weights.length; index += 1) {
        const sum = weights[index].reduce((total, value) => total + value, 0);
        if (!Number.isFinite(sum) || Math.abs(sum - 1) > 0.02) invalidWeights += 1;
        if (skin && joints[index]?.some((joint) => joint < 0 || joint >= skin.joints.length)) invalidJoints += 1;
      }
      if (invalidWeights > 0) errors.push(`${node.name ?? mesh.name} has ${invalidWeights} non-normalized/unweighted vertices.`);
      if (invalidJoints > 0) errors.push(`${node.name ?? mesh.name} has ${invalidJoints} out-of-range skin indices.`);
    }
  }
}

const materialNames = materials.map((material) => material.name ?? "");
for (const required of [
  "Nono_Skin",
  "Nono_Face",
  "Nono_Hair",
  "Nono_TailCable",
  "Nono_Blazer",
  "Nono_Shirt",
  "Nono_Skirt",
  "Nono_Eye",
  "Nono_Metal",
]) {
  if (!materialNames.some((name) => name.startsWith(required))) errors.push(`Missing required material role ${required}.`);
}

for (const node of nodes.filter((candidate) => /^Nono_Hair_(Bangs|Fwip|Long|Sweep)$/u.test(candidate.name ?? ""))) {
  const mesh = meshes[node.mesh];
  for (const primitive of mesh?.primitives ?? []) {
    if (primitive.attributes?.TEXCOORD_0 === undefined) errors.push(`${node.name} is missing hair UVs.`);
    if (primitive.attributes?.TANGENT === undefined) warnings.push(`${node.name} has no exported tangent; runtime hair specular will use its fallback.`);
  }
}

const MAX_TRIANGLES = 120_000;
const MAX_DRAW_CALLS = 60;
const MAX_BYTES = 15 * 1024 * 1024;
if (triangles > MAX_TRIANGLES) errors.push(`Triangle budget exceeded: ${triangles.toLocaleString()} > ${MAX_TRIANGLES.toLocaleString()}.`);
if (drawCalls > MAX_DRAW_CALLS) errors.push(`Draw-call budget exceeded: ${drawCalls} > ${MAX_DRAW_CALLS}.`);
if (bytes.byteLength > MAX_BYTES) errors.push(`File-size budget exceeded: ${(bytes.byteLength / 1024 / 1024).toFixed(1)} MB > 15 MB.`);

if (errors.length > 0) {
  console.error(`Nono GLB audit failed (${path}):`);
  for (const error of errors) console.error(`- ${error}`);
  for (const warning of warnings) console.warn(`- Warning: ${warning}`);
  process.exitCode = 1;
} else {
  console.log(`Nono GLB audit passed: ${triangles.toLocaleString()} triangles, ${drawCalls} draws, ${skinnedNodeCount} skinned meshes, ${(bytes.byteLength / 1024 / 1024).toFixed(1)} MB.`);
  for (const warning of warnings) console.warn(`- Warning: ${warning}`);
}
