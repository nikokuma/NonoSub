// Remove animation channels that target Nono's procedural bones.
//
// Blender's glTF exporter samples every bone into every clip; the runtime
// animates the tail chains, long-hair bones, and skirt secondaries itself, so
// shipped clips must not key them. Run between export_nono_final.py and
// audit_nono_glb.mjs:
//
//   node scripts/strip_nono_glb.mjs /path/to/NonoSubFinal.glb
//
// The file is rewritten in place. Orphaned samplers are dropped with their
// channels; accessor/buffer data is left untouched (unused ranges are inert).
import { readFileSync, writeFileSync } from "node:fs";
import { resolve } from "node:path";

const suppliedPath = process.argv[2];
if (!suppliedPath) throw new Error("Usage: node scripts/strip_nono_glb.mjs <file.glb>");
const path = resolve(suppliedPath);
const bytes = readFileSync(path);
const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
if (bytes.toString("ascii", 0, 4) !== "glTF" || view.getUint32(4, true) !== 2) {
  throw new Error(`${path} is not a glTF 2 binary.`);
}

const chunks = [];
let offset = 12;
while (offset + 8 <= bytes.byteLength) {
  const length = view.getUint32(offset, true);
  const type = view.getUint32(offset + 4, true);
  chunks.push({ type, body: bytes.subarray(offset + 8, offset + 8 + length) });
  offset += 8 + length;
}
const jsonChunk = chunks.find((chunk) => chunk.type === 0x4e4f534a);
if (!jsonChunk) throw new Error(`${path} has no JSON chunk.`);
const document = JSON.parse(jsonChunk.body.toString("utf8").replace(/\0+$/u, ""));

const nodes = document.nodes ?? [];
const namedNodeIndices = new Map(nodes.map((node, index) => [node.name, index]).filter(([name]) => Boolean(name)));
const childrenByNode = nodes.map((node) => node.children ?? []);

const tailBones = Array.from({ length: 24 }, (_, index) => `spine.${String(55 + index).padStart(3, "0")}`);
const semanticTails = ["L", "R"].flatMap((side) => Array.from({ length: 12 }, (_, index) => `tail.${side}.${String(index + 1).padStart(2, "0")}`));
const hairRoots = ["spine.021", "spine.031", "spine.039", "spine.085", "spine.093"];

const forbidden = new Set();
for (const name of [...tailBones, ...semanticTails]) {
  const index = namedNodeIndices.get(name);
  if (index !== undefined) forbidden.add(index);
}
for (const root of hairRoots) {
  const rootIndex = namedNodeIndices.get(root);
  if (rootIndex === undefined) continue;
  const pending = [rootIndex];
  while (pending.length > 0) {
    const current = pending.pop();
    if (forbidden.has(current)) continue;
    forbidden.add(current);
    pending.push(...(childrenByNode[current] ?? []));
  }
}
for (const [name, index] of namedNodeIndices) {
  if (/^skirt_root/u.test(name ?? "")) forbidden.add(index);
}

let removed = 0;
for (const animation of document.animations ?? []) {
  const kept = [];
  const keptSamplers = [];
  const samplerRemap = new Map();
  for (const channel of animation.channels ?? []) {
    if (forbidden.has(channel.target?.node)) {
      removed += 1;
      continue;
    }
    if (!samplerRemap.has(channel.sampler)) {
      samplerRemap.set(channel.sampler, keptSamplers.length);
      keptSamplers.push(animation.samplers[channel.sampler]);
    }
    kept.push({ ...channel, sampler: samplerRemap.get(channel.sampler) });
  }
  animation.channels = kept;
  animation.samplers = keptSamplers;
}

const jsonBytes = Buffer.from(JSON.stringify(document), "utf8");
const paddedJson = Buffer.concat([jsonBytes, Buffer.alloc((4 - (jsonBytes.length % 4)) % 4, 0x20)]);
const parts = [];
for (const chunk of chunks) {
  const body = chunk.type === 0x4e4f534a ? paddedJson : chunk.body;
  const header = Buffer.alloc(8);
  header.writeUInt32LE(body.length, 0);
  header.writeUInt32LE(chunk.type, 4);
  parts.push(header, body);
}
const payload = Buffer.concat(parts);
const fileHeader = Buffer.alloc(12);
fileHeader.write("glTF", 0, "ascii");
fileHeader.writeUInt32LE(2, 4);
fileHeader.writeUInt32LE(12 + payload.length, 8);
writeFileSync(path, Buffer.concat([fileHeader, payload]));
console.log(`Stripped ${removed} procedural-bone channels from ${path}.`);
