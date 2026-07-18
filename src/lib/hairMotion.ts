import * as THREE from "three";

export const DYNAMIC_HAIR_ROOTS = [
  "spine.021",
  "spine.031",
  "spine.039",
  "spine.085",
  "spine.093",
] as const;

const TAIL_BONES = new Set(
  Array.from({ length: 24 }, (_, index) => THREE.PropertyBinding.sanitizeNodeName(`spine.${String(55 + index).padStart(3, "0")}`)),
);

export interface HairSpringState {
  pitch: number;
  pitchVelocity: number;
  yaw: number;
  yawVelocity: number;
}

export interface HairChainRuntime {
  root: THREE.Bone;
  bones: THREE.Bone[];
  restRotations: Map<THREE.Bone, THREE.Quaternion>;
  depths: Map<THREE.Bone, number>;
  state: HairSpringState;
  phase: number;
}

export interface HairMotionRig {
  chains: HairChainRuntime[];
  head?: THREE.Bone;
  previousHeadEuler: THREE.Euler;
  hasPreviousHeadPose: boolean;
}

export function resolveHairMotionRig(model: THREE.Object3D): HairMotionRig | undefined {
  const roots = DYNAMIC_HAIR_ROOTS
    .map((name) => findRigObject(model, name))
    .filter((bone): bone is THREE.Bone => Boolean(bone && "isBone" in bone && bone.isBone));
  if (roots.length === 0) return undefined;

  const chains = roots.map((root, chainIndex) => {
    const bones: THREE.Bone[] = [];
    const depths = new Map<THREE.Bone, number>();
    const visit = (bone: THREE.Bone, depth: number) => {
      if (TAIL_BONES.has(bone.name) || bone.name.startsWith("skirt_root")) return;
      bones.push(bone);
      depths.set(bone, depth);
      for (const child of bone.children) {
        if ("isBone" in child && child.isBone) visit(child as THREE.Bone, depth + 1);
      }
    };
    visit(root, 0);
    return {
      root,
      bones,
      restRotations: new Map(bones.map((bone) => [bone, bone.quaternion.clone()])),
      depths,
      state: { pitch: 0, pitchVelocity: 0, yaw: 0, yawVelocity: 0 },
      phase: chainIndex * 0.83,
    };
  }).filter((chain) => chain.bones.length > 0);

  return {
    chains,
    head: findBone(model, "spine.006"),
    previousHeadEuler: new THREE.Euler(0, 0, 0, "YXZ"),
    hasPreviousHeadPose: false,
  };
}

export function restoreHairPose(rig: HairMotionRig): void {
  for (const chain of rig.chains) {
    for (const [bone, rotation] of chain.restRotations) bone.quaternion.copy(rotation);
  }
}

export function stepHairSpring(
  state: HairSpringState,
  targetPitch: number,
  targetYaw: number,
  deltaSeconds: number,
  frequency = 5,
  damping = 1,
): HairSpringState {
  const delta = THREE.MathUtils.clamp(deltaSeconds, 0, 0.05);
  if (delta === 0) return { ...state };
  const omega = Math.max(0.01, frequency) * Math.PI * 2;
  const accelerationPitch = omega * omega * (targetPitch - state.pitch) - 2 * damping * omega * state.pitchVelocity;
  const accelerationYaw = omega * omega * (targetYaw - state.yaw) - 2 * damping * omega * state.yawVelocity;
  const pitchVelocity = state.pitchVelocity + accelerationPitch * delta;
  const yawVelocity = state.yawVelocity + accelerationYaw * delta;
  return {
    pitch: state.pitch + pitchVelocity * delta,
    pitchVelocity,
    yaw: state.yaw + yawVelocity * delta,
    yawVelocity,
  };
}

export function applyHairMotion(
  rig: HairMotionRig,
  deltaSeconds: number,
  timestampMilliseconds: number,
  reducedMotion: boolean,
): void {
  restoreHairPose(rig);
  if (reducedMotion) {
    for (const chain of rig.chains) chain.state = { pitch: 0, pitchVelocity: 0, yaw: 0, yawVelocity: 0 };
    rig.hasPreviousHeadPose = false;
    return;
  }

  const headEuler = new THREE.Euler(0, 0, 0, "YXZ");
  if (rig.head) {
    rig.head.getWorldQuaternion(_headQuaternion);
    headEuler.setFromQuaternion(_headQuaternion, "YXZ");
  }
  const delta = Math.max(deltaSeconds, 1 / 240);
  const pitchVelocity = rig.hasPreviousHeadPose ? shortestAngle(headEuler.x - rig.previousHeadEuler.x) / delta : 0;
  const yawVelocity = rig.hasPreviousHeadPose ? shortestAngle(headEuler.y - rig.previousHeadEuler.y) / delta : 0;
  rig.previousHeadEuler.copy(headEuler);
  rig.hasPreviousHeadPose = true;

  const time = timestampMilliseconds / 1_000;
  rig.chains.forEach((chain, index) => {
    const idlePitch = Math.sin(time * 0.92 + chain.phase) * THREE.MathUtils.degToRad(0.7);
    const idleYaw = Math.sin(time * 0.71 + chain.phase * 1.37) * THREE.MathUtils.degToRad(0.9);
    const pitchTarget = THREE.MathUtils.clamp(-pitchVelocity * 0.018 + idlePitch, -0.06, 0.06);
    const yawTarget = THREE.MathUtils.clamp(-yawVelocity * 0.018 + idleYaw, -0.075, 0.075);
    chain.state = stepHairSpring(chain.state, pitchTarget, yawTarget, deltaSeconds, 3.2 + index * 0.16, 1.05);
    const maximumDepth = Math.max(1, ...chain.depths.values());
    for (const bone of chain.bones) {
      const depth = chain.depths.get(bone) ?? 0;
      const influence = THREE.MathUtils.lerp(0.32, 1, depth / maximumDepth);
      bone.rotateX(chain.state.pitch * influence);
      bone.rotateZ(chain.state.yaw * influence);
    }
    chain.root.updateWorldMatrix(true, true);
  });
}

function findBone(model: THREE.Object3D, name: string): THREE.Bone | undefined {
  const object = findRigObject(model, name);
  return object && "isBone" in object && object.isBone ? object as THREE.Bone : undefined;
}

function findRigObject(model: THREE.Object3D, name: string): THREE.Object3D | undefined {
  return model.getObjectByName(name) ?? model.getObjectByName(THREE.PropertyBinding.sanitizeNodeName(name));
}

function shortestAngle(value: number): number {
  return Math.atan2(Math.sin(value), Math.cos(value));
}

const _headQuaternion = new THREE.Quaternion();
