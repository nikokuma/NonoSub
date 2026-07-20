import * as THREE from "three";

export const TAIL_TUNING = {
  baseHandle: 0.40,
  tipHandle: 0.30,
  maxHandle: 0.5,
  sagGain: 0.10,
  bulgeSplit: [0.35, 0.65],
  bulgeMax: 0.6,
  slackTolerance: 0.01,
  slackIterations: 3,
  lutSamples: 64,
  protectedJoints: 3,
  rampEnd: 6,
  stretchMax: 1.3,
  stretchStartSegment: 3,
  followThrough: 0.02,
  anticipation: {
    window: 0.3,
    magnitude: 0.06,
  },
  droop: {
    magnitude: 0.08,
  },
  wave: {
    frequency: 0.45,
    waveNumber: 0.55,
    amplitude: 0.015,
    workingSuppression: 0.7,
  },
} as const;

export interface CableCurve {
  p0: THREE.Vector3;
  p1: THREE.Vector3;
  p2: THREE.Vector3;
  p3: THREE.Vector3;
  degenerate: boolean;
  totalLength: number;
  cumulativeLengths: Float64Array;
  points: Float64Array;
}

export interface CableCurveInput {
  root: THREE.Vector3;
  baseTangent: THREE.Vector3;
  target: THREE.Vector3;
  tipTangent: THREE.Vector3;
  chainLength: number;
  bulgeDirection: THREE.Vector3;
  sagDirection: THREE.Vector3;
}

export function createCableCurve(): CableCurve {
  return {
    p0: new THREE.Vector3(),
    p1: new THREE.Vector3(),
    p2: new THREE.Vector3(),
    p3: new THREE.Vector3(),
    degenerate: true,
    totalLength: 0,
    cumulativeLengths: new Float64Array(TAIL_TUNING.lutSamples + 1),
    points: new Float64Array((TAIL_TUNING.lutSamples + 1) * 3),
  };
}

export function buildCableCurve(input: CableCurveInput, out = createCableCurve()): CableCurve {
  const chordLength = input.root.distanceTo(input.target);
  out.p0.copy(input.root);
  out.p3.copy(input.target);
  out.degenerate = chordLength < 1e-5 || !Number.isFinite(chordLength);
  if (out.degenerate) {
    out.p1.copy(input.root);
    out.p2.copy(input.root);
    rebuildLut(out);
    return out;
  }

  const baseHandle = Math.min(TAIL_TUNING.baseHandle * chordLength, TAIL_TUNING.maxHandle * chordLength);
  const tipHandle = Math.min(TAIL_TUNING.tipHandle * chordLength, TAIL_TUNING.maxHandle * chordLength);
  out.p1.copy(input.baseTangent).multiplyScalar(baseHandle).add(input.root);
  out.p2.copy(input.tipTangent).multiplyScalar(-tipHandle).add(input.target);
  const sag = TAIL_TUNING.sagGain * chordLength;
  out.p1.addScaledVector(input.sagDirection, sag);
  out.p2.addScaledVector(input.sagDirection, sag);

  rebuildLut(out);
  const safeChainLength = Math.max(0, Number.isFinite(input.chainLength) ? input.chainLength : 0);
  const slack = safeChainLength - out.totalLength;
  if (slack <= TAIL_TUNING.slackTolerance * safeChainLength || safeChainLength <= 0) return out;

  _baseP1.copy(out.p1);
  _baseP2.copy(out.p2);
  const maximumBulge = TAIL_TUNING.bulgeMax * safeChainLength;
  let lowerMagnitude = 0;
  let lowerError = out.totalLength - safeChainLength;
  let upperMagnitude = maximumBulge;
  applyBulge(out, input.bulgeDirection, upperMagnitude);
  rebuildLut(out);
  let upperError = out.totalLength - safeChainLength;

  for (let iteration = 0; iteration < TAIL_TUNING.slackIterations && Math.abs(upperError) > TAIL_TUNING.slackTolerance * safeChainLength; iteration += 1) {
    const denominator = upperError - lowerError;
    if (Math.abs(denominator) < 1e-8) break;
    const nextMagnitude = THREE.MathUtils.clamp(
      upperMagnitude - upperError * (upperMagnitude - lowerMagnitude) / denominator,
      0,
      maximumBulge,
    );
    if (Math.abs(nextMagnitude - upperMagnitude) < 1e-8) break;
    lowerMagnitude = upperMagnitude;
    lowerError = upperError;
    upperMagnitude = nextMagnitude;
    applyBulge(out, input.bulgeDirection, upperMagnitude);
    rebuildLut(out);
    upperError = out.totalLength - safeChainLength;
  }
  return out;
}

export function curvePointByArc(curve: CableCurve, s: number, out = new THREE.Vector3()): THREE.Vector3 {
  if (!Number.isFinite(s) || s <= 0) return out.copy(curve.p0);
  if (s > curve.totalLength) {
    let high = curve.cumulativeLengths.length - 1;
    while (high > 0 && curve.cumulativeLengths[high] - curve.cumulativeLengths[high - 1] <= 1e-8) high -= 1;
    if (high === 0) return out.copy(curve.p3);
    const end = high * 3;
    const start = end - 3;
    _endTangent.set(
      curve.points[end] - curve.points[start],
      curve.points[end + 1] - curve.points[start + 1],
      curve.points[end + 2] - curve.points[start + 2],
    ).normalize();
    return out.copy(curve.p3).addScaledVector(_endTangent, s - curve.totalLength);
  }
  if (curve.totalLength <= 1e-8) return out.copy(curve.p0);
  const distance = s;
  let low = 0;
  let high = curve.cumulativeLengths.length - 1;
  while (low + 1 < high) {
    const middle = (low + high) >> 1;
    if (curve.cumulativeLengths[middle] < distance) low = middle;
    else high = middle;
  }
  const startLength = curve.cumulativeLengths[low];
  const intervalLength = curve.cumulativeLengths[high] - startLength;
  const alpha = intervalLength > 1e-8 ? (distance - startLength) / intervalLength : 0;
  const start = low * 3;
  const end = high * 3;
  return out.set(
    THREE.MathUtils.lerp(curve.points[start], curve.points[end], alpha),
    THREE.MathUtils.lerp(curve.points[start + 1], curve.points[end + 1], alpha),
    THREE.MathUtils.lerp(curve.points[start + 2], curve.points[end + 2], alpha),
  );
}

export function curveLength(curve: CableCurve): number {
  return curve.totalLength;
}

export function presentationTargetOffset(
  phase: "idle" | "point" | "hold" | "underline" | "retract" | "sustain",
  progress: number,
  isPointTail: boolean,
  out: { pullback: number; droop: number } = { pullback: 0, droop: 0 },
): { pullback: number; droop: number } {
  const clampedProgress = THREE.MathUtils.clamp(progress, 0, 1);
  out.pullback = phase === "point" && isPointTail && clampedProgress < TAIL_TUNING.anticipation.window
    ? Math.sin(Math.PI * clampedProgress / TAIL_TUNING.anticipation.window) * TAIL_TUNING.anticipation.magnitude
    : 0;
  out.droop = phase === "retract" && clampedProgress > 0 && clampedProgress < 1
    ? Math.sin(Math.PI * clampedProgress) * TAIL_TUNING.droop.magnitude
    : 0;
  return out;
}

export function distributeStretch(restLengths: readonly number[], stretch: number, out?: number[]): number[] {
  const result = out ?? new Array<number>(restLengths.length);
  result.length = restLengths.length;
  const cappedStretch = THREE.MathUtils.clamp(Number.isFinite(stretch) ? stretch : 1, 1, TAIL_TUNING.stretchMax);
  let total = 0;
  for (let index = 0; index < restLengths.length; index += 1) {
    const length = Math.max(0, Number.isFinite(restLengths[index]) ? restLengths[index] : 0);
    result[index] = length;
    total += length;
  }
  if (cappedStretch <= 1 || total <= 0) return result;

  const denominator = Math.max(1, restLengths.length - 1 - TAIL_TUNING.stretchStartSegment);
  let weightedLength = 0;
  for (let index = TAIL_TUNING.stretchStartSegment; index < result.length; index += 1) {
    weightedLength += result[index] * smoothstep((index - TAIL_TUNING.stretchStartSegment) / denominator);
  }
  if (weightedLength <= 1e-12) return result;
  const gain = total / weightedLength;
  let stretchedTotal = 0;
  let lastStretchable = -1;
  for (let index = 0; index < result.length; index += 1) {
    if (index >= TAIL_TUNING.stretchStartSegment) {
      const weight = smoothstep((index - TAIL_TUNING.stretchStartSegment) / denominator);
      result[index] *= 1 + (cappedStretch - 1) * weight * gain;
      if (weight > 0) lastStretchable = index;
    }
    stretchedTotal += result[index];
  }
  if (lastStretchable >= 0) result[lastStretchable] += cappedStretch * total - stretchedTotal;
  return result;
}

export function blendGuidePolyline(
  restJoints: readonly THREE.Vector3[],
  curveJoints: readonly THREE.Vector3[],
  strength: number,
  out: THREE.Vector3[],
): void {
  const count = Math.min(restJoints.length, curveJoints.length, out.length);
  const clampedStrength = THREE.MathUtils.clamp(Number.isFinite(strength) ? strength : 0, 0, 1);
  const rampSpan = Math.max(1, TAIL_TUNING.rampEnd - (TAIL_TUNING.protectedJoints - 1));
  for (let index = 0; index < count; index += 1) {
    const ramp = index < TAIL_TUNING.protectedJoints
      ? 0
      : index >= TAIL_TUNING.rampEnd
        ? 1
        : smoothstep((index - (TAIL_TUNING.protectedJoints - 1)) / rampSpan);
    out[index].copy(restJoints[index]).lerp(curveJoints[index], clampedStrength * ramp);
  }
}

export function applyTravelingWave(
  joints: THREE.Vector3[],
  segmentLateral: (index: number) => THREE.Vector3,
  timeSeconds: number,
  side: "left" | "right",
  strength: number,
  suppressTipCount: number,
  chainLength: number,
): void {
  const denominator = Math.max(1, joints.length - 1);
  const phase = side === "left" ? 0 : Math.PI * 0.6;
  const suppression = 1 - TAIL_TUNING.wave.workingSuppression * THREE.MathUtils.clamp(strength, 0, 1);
  const amplitude = TAIL_TUNING.wave.amplitude * Math.max(0, chainLength) * suppression;
  const activeCount = Math.max(0, joints.length - Math.max(0, suppressTipCount));
  for (let index = 0; index < activeCount; index += 1) {
    const envelope = smoothstep(index / denominator);
    const wave = Math.sin(2 * Math.PI * TAIL_TUNING.wave.frequency * timeSeconds - TAIL_TUNING.wave.waveNumber * index + phase);
    joints[index].addScaledVector(segmentLateral(index), amplitude * envelope * wave);
  }
}

export function fitChainToPolyline(
  chain: THREE.Bone[],
  restLocalPositions: readonly THREE.Vector3[],
  guide: readonly THREE.Vector3[],
  segmentLengths: readonly number[],
): void {
  const count = Math.min(chain.length, restLocalPositions.length, guide.length, segmentLengths.length + 1);
  if (count < 2) return;
  chain[0].updateWorldMatrix(true, true);
  for (let index = 0; index < count - 1; index += 1) {
    const bone = chain[index];
    const child = chain[index + 1];
    child.position.copy(restLocalPositions[index + 1]);
    child.updateWorldMatrix(true, false);
    bone.getWorldPosition(_boneWorldPosition);
    child.getWorldPosition(_childWorldPosition);
    const currentLength = _childWorldPosition.distanceTo(_boneWorldPosition);
    const requestedLength = Math.max(0, Number.isFinite(segmentLengths[index]) ? segmentLengths[index] : 0);
    if (currentLength > 1e-10) child.position.multiplyScalar(requestedLength / currentLength);
    child.updateWorldMatrix(false, false);
    child.getWorldPosition(_childWorldPosition);
    _currentDirection.subVectors(_childWorldPosition, _boneWorldPosition);
    _desiredDirection.subVectors(guide[index + 1], _boneWorldPosition);
    if (_currentDirection.lengthSq() <= 1e-12 || _desiredDirection.lengthSq() <= 1e-12) continue;
    _currentDirection.normalize();
    _desiredDirection.normalize();
    const dot = THREE.MathUtils.clamp(_currentDirection.dot(_desiredDirection), -1, 1);
    bone.getWorldQuaternion(_boneWorldQuaternion);
    if (dot < -0.999) {
      stableLocalLateral(restLocalPositions[index + 1], _stableAxis).applyQuaternion(_boneWorldQuaternion).normalize();
      _delta.setFromAxisAngle(_stableAxis, Math.PI);
    } else {
      _delta.setFromUnitVectors(_currentDirection, _desiredDirection);
    }
    _desiredWorldQuaternion.copy(_delta).multiply(_boneWorldQuaternion);
    if (bone.parent) bone.parent.getWorldQuaternion(_parentWorldQuaternion).invert();
    else _parentWorldQuaternion.identity();
    bone.quaternion.copy(_parentWorldQuaternion).multiply(_desiredWorldQuaternion).normalize();
    bone.updateWorldMatrix(false, false);
    child.updateWorldMatrix(false, false);
  }
}

export function aimChainTip(chain: THREE.Bone[], worldDirection: THREE.Vector3, blend: number): void {
  if (chain.length < 2) return;
  const clampedBlend = THREE.MathUtils.clamp(Number.isFinite(blend) ? blend : 0, 0, 1);
  if (clampedBlend <= 1e-4 || worldDirection.lengthSq() <= 1e-12) return;
  const bone = chain[chain.length - 2];
  const tip = chain[chain.length - 1];
  bone.updateWorldMatrix(true, true);
  bone.getWorldPosition(_boneWorldPosition);
  tip.getWorldPosition(_childWorldPosition);
  _currentDirection.subVectors(_childWorldPosition, _boneWorldPosition);
  if (_currentDirection.lengthSq() <= 1e-12) return;
  _currentDirection.normalize();
  _desiredDirection.copy(worldDirection).normalize();
  const dot = THREE.MathUtils.clamp(_currentDirection.dot(_desiredDirection), -1, 1);
  bone.getWorldQuaternion(_boneWorldQuaternion);
  if (dot < -0.999) {
    stableLocalLateral(tip.position, _stableAxis).applyQuaternion(_boneWorldQuaternion).normalize();
    _delta.setFromAxisAngle(_stableAxis, Math.PI);
  } else {
    _delta.setFromUnitVectors(_currentDirection, _desiredDirection);
  }
  _desiredWorldQuaternion.copy(_delta).multiply(_boneWorldQuaternion);
  _boneWorldQuaternion.slerp(_desiredWorldQuaternion, clampedBlend);
  if (bone.parent) bone.parent.getWorldQuaternion(_parentWorldQuaternion).invert();
  else _parentWorldQuaternion.identity();
  bone.quaternion.copy(_parentWorldQuaternion).multiply(_boneWorldQuaternion).normalize();
  bone.updateWorldMatrix(false, false);
  tip.updateWorldMatrix(false, false);
}

function applyBulge(curve: CableCurve, direction: THREE.Vector3, magnitude: number): void {
  curve.p1.copy(_baseP1).addScaledVector(direction, magnitude * TAIL_TUNING.bulgeSplit[0] * 2);
  curve.p2.copy(_baseP2).addScaledVector(direction, magnitude * TAIL_TUNING.bulgeSplit[1] * 2);
}

function rebuildLut(curve: CableCurve): void {
  let previousX = 0;
  let previousY = 0;
  let previousZ = 0;
  let total = 0;
  for (let index = 0; index <= TAIL_TUNING.lutSamples; index += 1) {
    const t = index / TAIL_TUNING.lutSamples;
    const inverse = 1 - t;
    const a = inverse * inverse * inverse;
    const b = 3 * inverse * inverse * t;
    const c = 3 * inverse * t * t;
    const d = t * t * t;
    const x = a * curve.p0.x + b * curve.p1.x + c * curve.p2.x + d * curve.p3.x;
    const y = a * curve.p0.y + b * curve.p1.y + c * curve.p2.y + d * curve.p3.y;
    const z = a * curve.p0.z + b * curve.p1.z + c * curve.p2.z + d * curve.p3.z;
    const pointOffset = index * 3;
    curve.points[pointOffset] = x;
    curve.points[pointOffset + 1] = y;
    curve.points[pointOffset + 2] = z;
    if (index > 0) total += Math.hypot(x - previousX, y - previousY, z - previousZ);
    curve.cumulativeLengths[index] = total;
    previousX = x;
    previousY = y;
    previousZ = z;
  }
  curve.totalLength = total;
}

function stableLocalLateral(direction: THREE.Vector3, out: THREE.Vector3): THREE.Vector3 {
  _localDirection.copy(direction).normalize();
  if (_localDirection.lengthSq() <= 1e-12) return out.set(0, 0, 1);
  const ax = Math.abs(_localDirection.x);
  const ay = Math.abs(_localDirection.y);
  const az = Math.abs(_localDirection.z);
  if (ax <= ay && ax <= az) _referenceAxis.set(1, 0, 0);
  else if (ay <= az) _referenceAxis.set(0, 1, 0);
  else _referenceAxis.set(0, 0, 1);
  return out.crossVectors(_localDirection, _referenceAxis).normalize();
}

function smoothstep(value: number): number {
  const clamped = THREE.MathUtils.clamp(value, 0, 1);
  return clamped * clamped * (3 - 2 * clamped);
}

const _baseP1 = new THREE.Vector3();
const _baseP2 = new THREE.Vector3();
const _endTangent = new THREE.Vector3();
const _boneWorldPosition = new THREE.Vector3();
const _childWorldPosition = new THREE.Vector3();
const _currentDirection = new THREE.Vector3();
const _desiredDirection = new THREE.Vector3();
const _stableAxis = new THREE.Vector3();
const _localDirection = new THREE.Vector3();
const _referenceAxis = new THREE.Vector3();
const _boneWorldQuaternion = new THREE.Quaternion();
const _parentWorldQuaternion = new THREE.Quaternion();
const _desiredWorldQuaternion = new THREE.Quaternion();
const _delta = new THREE.Quaternion();
