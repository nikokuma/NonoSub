<script lang="ts">
  import { onMount } from "svelte";
  import * as THREE from "three";
  import { GLTFLoader } from "three/examples/jsm/loaders/GLTFLoader.js";
  import { applyHairMotion, resolveHairMotionRig, type HairMotionRig } from "./hairMotion";
  import { applyNonoMaterials, nonoAssetFromLocation, shaderVariantFromLocation } from "./nonoToon";
  import {
    applyTravelingWave,
    blendGuidePolyline,
    buildCableCurve,
    createCableCurve,
    curvePointByArc,
    distributeStretch,
    fitChainToPolyline,
    presentationTargetOffset,
    TAIL_TUNING,
    type CableCurve,
    type CableCurveInput,
  } from "./tailCable";
  import {
    createTailDynamicsState,
    resetTailDynamics,
    seedVec3Spring,
    stepScalarSpring,
    stepVec3Spring,
    TAIL_SPRING,
    type TailDynamicsState,
  } from "./tailDynamics";
  import {
    captureTailRestPose,
    CURRENT_TAIL_BONES,
    requiredTailStretch,
    restoreTailRestPose,
    SEMANTIC_TAIL_BONES,
    chooseNearestTail,
    cueScreenPoint,
    presentationStrength,
    type ScreenPoint,
    type TailPresentation,
    type TailRestPose,
  } from "./tailPresentation";

  type NonoMood = "idle" | "think" | "present";
  type TailSide = "left" | "right";
  type TailRig = Record<TailSide, THREE.Bone[]>;
  interface TailCableBuffers {
    restWorldJoints: THREE.Vector3[];
    curveJoints: THREE.Vector3[];
    blendedJoints: THREE.Vector3[];
    laterals: THREE.Vector3[];
    restWorldSegmentLengths: number[];
    segmentLengths: number[];
    target: THREE.Vector3;
    baseTangent: THREE.Vector3;
    tipTangent: THREE.Vector3;
    bulgeDirection: THREE.Vector3;
    curve: CableCurve;
    curveInput: CableCurveInput;
    segmentLateral: (index: number) => THREE.Vector3;
  }

  let {
    presentation,
    mood = "idle",
    onRigStatus,
    onTailTip,
  }: {
    presentation: TailPresentation;
    mood?: NonoMood;
    onRigStatus?: (available: boolean) => void;
    onTailTip?: (report: { cueId: string; x: number; y: number }) => void;
  } = $props();

  let container: HTMLDivElement;
  let failed = $state(false);

  onMount(() => {
    const scene = new THREE.Scene();
    const camera = new THREE.PerspectiveCamera(32, 1, 0.1, 100);
    camera.position.set(0, 0.35, 5);
    camera.lookAt(0, 0.35, 0);

    const renderer = new THREE.WebGLRenderer({ alpha: true, antialias: true });
    renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
    renderer.outputColorSpace = THREE.SRGBColorSpace;
    renderer.setClearColor(0x000000, 0);
    container.appendChild(renderer.domElement);
    const handleContextLost = (event: Event) => {
      event.preventDefault();
      failed = true;
    };
    const handleContextRestored = () => failed = false;
    renderer.domElement.addEventListener("webglcontextlost", handleContextLost);
    renderer.domElement.addEventListener("webglcontextrestored", handleContextRestored);

    scene.add(new THREE.HemisphereLight(0xf7e7ff, 0x30295a, 2.8));
    const key = new THREE.DirectionalLight(0xffe8f4, 4.2);
    key.position.set(2.5, 4, 3);
    scene.add(key);
    const rim = new THREE.DirectionalLight(0x8d7cff, 3.2);
    rim.position.set(-3, 2, -2);
    scene.add(rim);

    let model: THREE.Object3D | undefined;
    let mixer: THREE.AnimationMixer | undefined;
    let activeAction: THREE.AnimationAction | undefined;
    let activeMood: NonoMood | undefined;
    let tailRig: TailRig | undefined;
    let tailRestPoses: Record<TailSide, TailRestPose> | undefined;
    let tailDynamics: Record<TailSide, TailDynamicsState> | undefined;
    let tailCableBuffers: Record<TailSide, TailCableBuffers> | undefined;
    let tailDebugLines: Record<TailSide, THREE.Line> | undefined;
    let hairRig: HairMotionRig | undefined;
    let assignedPointTail: TailSide | undefined;
    let assignedUnderlineTail: TailSide | undefined;
    let assignedSequence = -1;
    const motionPreference = window.matchMedia("(prefers-reduced-motion: reduce)");
    let reducedMotion = motionPreference.matches;
    const updateMotionPreference = () => reducedMotion = motionPreference.matches;
    motionPreference.addEventListener("change", updateMotionPreference);
    const loader = new GLTFLoader();
    const shaderVariant = shaderVariantFromLocation(window.location.search);
    const tailDebugEnabled = import.meta.env.DEV && new URLSearchParams(window.location.search).get("tailDebug") === "1";
    if (import.meta.env.DEV) container.dataset.nonoShader = shaderVariant;

    loader.load(nonoAssetFromLocation(window.location.search), (gltf) => {
      model = gltf.scene;
      applyNonoMaterials(model, shaderVariant);
      const bounds = new THREE.Box3().setFromObject(model);
      const size = bounds.getSize(new THREE.Vector3());
      const center = bounds.getCenter(new THREE.Vector3());
      const scale = 1.15 / Math.max(size.y, 0.001);
      model.scale.setScalar(scale);
      model.position.set(-1.15 - center.x * scale, 0.77 - center.y * scale, -center.z * scale);
      scene.add(model);

      if (gltf.animations.length > 0) mixer = new THREE.AnimationMixer(model);
      if (tailDynamics) {
        resetTailDynamics(tailDynamics.left);
        resetTailDynamics(tailDynamics.right);
      }
      tailDynamics = undefined;
      tailCableBuffers = undefined;
      tailRig = resolveTailRig(model);
      if (tailRig) {
        tailRestPoses = {
          left: captureTailRestPose(tailRig.left),
          right: captureTailRestPose(tailRig.right),
        };
        tailDynamics = {
          left: createTailDynamicsState(),
          right: createTailDynamicsState(),
        };
        tailCableBuffers = {
          left: createTailCableBuffers(tailRig.left),
          right: createTailCableBuffers(tailRig.right),
        };
        if (tailDebugEnabled) {
          tailDebugLines = {
            left: createTailDebugLine(0xff70b7),
            right: createTailDebugLine(0x8d7cff),
          };
          scene.add(tailDebugLines.left, tailDebugLines.right);
        }
        onRigStatus?.(true);
      } else {
        tailRestPoses = undefined;
        onRigStatus?.(false);
        if (import.meta.env.DEV) console.warn("Nono tail rig unavailable: the GLB needs skinned tail geometry and both 12-bone chains.");
      }
      hairRig = resolveHairMotionRig(model);

      function updateAnimation(nextMood: NonoMood) {
        if (!mixer || nextMood === activeMood) return;
        activeMood = nextMood;
        const preferred = nextMood === "think" ? "think" : nextMood === "present" ? "present" : "idle";
        const clip = gltf.animations.find((candidate) => candidate.name.toLowerCase() === preferred);
        if (!clip) return;
        const nextAction = mixer.clipAction(clip);
        nextAction.reset();
        if (nextMood === "present") {
          nextAction.setLoop(THREE.LoopOnce, 1);
          nextAction.clampWhenFinished = true;
        } else {
          nextAction.setLoop(THREE.LoopRepeat, Number.POSITIVE_INFINITY);
          nextAction.clampWhenFinished = false;
        }
        nextAction.fadeIn(0.18).play();
        activeAction?.fadeOut(0.18);
        activeAction = nextAction;
      }
      updateAnimation(mood);
      model.userData.updateAnimation = updateAnimation;
    }, undefined, () => {
      failed = true;
      onRigStatus?.(false);
    });

    let animationFrame = 0;
    let previousTime = performance.now();
    let fpsWindowStarted = previousTime;
    let fpsFrames = 0;
    const resize = () => {
      const width = Math.max(container.clientWidth, 1);
      const height = Math.max(container.clientHeight, 1);
      renderer.setSize(width, height, false);
      camera.aspect = width / height;
      camera.updateProjectionMatrix();
    };
    const observer = new ResizeObserver(resize);
    observer.observe(container);
    resize();

    const render = (timestamp: number) => {
      if (import.meta.env.DEV) {
        fpsFrames += 1;
        if (timestamp - fpsWindowStarted >= 1_000) {
          container.dataset.nonoFps = String(Math.round(fpsFrames * 1_000 / (timestamp - fpsWindowStarted)));
          fpsWindowStarted = timestamp;
          fpsFrames = 0;
        }
      }
      const delta = Math.min((timestamp - previousTime) / 1_000, 0.05);
      previousTime = timestamp;
      mixer?.update(delta);
      if (model) {
        (model.userData.updateAnimation as ((nextMood: NonoMood) => void) | undefined)?.(mood);
        model.rotation.y = reducedMotion ? 0 : Math.sin(timestamp * 0.00055) * 0.018;
        model.updateWorldMatrix(true, true);
        if (hairRig) applyHairMotion(hairRig, delta, timestamp, reducedMotion);
      }
      if (model && tailRig && tailRestPoses && tailDynamics && tailCableBuffers) {
        restoreTailRestPose(tailRig.left, tailRestPoses.left);
        restoreTailRestPose(tailRig.right, tailRestPoses.right);
        if (!reducedMotion) {
          ({ assignedPointTail, assignedUnderlineTail, assignedSequence } = applyPresentation({
            presentation,
            rig: tailRig,
            restPoses: tailRestPoses,
            dynamics: tailDynamics,
            cableBuffers: tailCableBuffers,
            debugLines: tailDebugLines,
            camera,
            canvas: renderer.domElement,
            delta,
            timestamp,
            assignedPointTail,
            assignedUnderlineTail,
            assignedSequence,
          }));
        } else {
          resetTailDynamics(tailDynamics.left);
          resetTailDynamics(tailDynamics.right);
          if (tailDebugLines) {
            tailDebugLines.left.visible = false;
            tailDebugLines.right.visible = false;
          }
        }
      }
      renderer.render(scene, camera);
      animationFrame = requestAnimationFrame(render);
    };
    animationFrame = requestAnimationFrame(render);

    return () => {
      cancelAnimationFrame(animationFrame);
      observer.disconnect();
      motionPreference.removeEventListener("change", updateMotionPreference);
      mixer?.stopAllAction();
      renderer.domElement.removeEventListener("webglcontextlost", handleContextLost);
      renderer.domElement.removeEventListener("webglcontextrestored", handleContextRestored);
      if (tailDebugLines) {
        for (const line of Object.values(tailDebugLines)) {
          scene.remove(line);
          line.geometry.dispose();
          (line.material as THREE.Material).dispose();
        }
      }
      if (model) disposeModel(model);
      renderer.dispose();
      renderer.domElement.remove();
    };
  });

  function resolveTailRig(model: THREE.Object3D): TailRig | undefined {
    let tailMesh: THREE.SkinnedMesh | undefined;
    model.traverse((object) => {
      if (object.name === "Nono_Tails" && "isSkinnedMesh" in object && object.isSkinnedMesh) {
        tailMesh = object as THREE.SkinnedMesh;
      }
    });
    if (!tailMesh?.geometry.getAttribute("skinIndex") || !tailMesh.geometry.getAttribute("skinWeight")) {
      if (import.meta.env.DEV) console.warn("Nono tail rig diagnostic", { tailMeshFound: Boolean(tailMesh), hasSkinIndex: Boolean(tailMesh?.geometry.getAttribute("skinIndex")), hasSkinWeight: Boolean(tailMesh?.geometry.getAttribute("skinWeight")) });
      return undefined;
    }
    const resolve = (names: readonly string[]) => names.map((name) => model.getObjectByName(name) ?? model.getObjectByName(THREE.PropertyBinding.sanitizeNodeName(name))).filter((bone): bone is THREE.Bone => Boolean(bone && "isBone" in bone && bone.isBone));
    const currentLeft = resolve(CURRENT_TAIL_BONES.left);
    const currentRight = resolve(CURRENT_TAIL_BONES.right);
    const semanticLeft = resolve(SEMANTIC_TAIL_BONES.left);
    const semanticRight = resolve(SEMANTIC_TAIL_BONES.right);
    const left = currentLeft.length === 12 ? currentLeft : semanticLeft;
    const right = currentRight.length === 12 ? currentRight : semanticRight;
    if (left.length !== 12 || right.length !== 12) {
      if (import.meta.env.DEV) console.warn(`Nono tail-chain diagnostic ${JSON.stringify({ currentLeft: currentLeft.length, currentRight: currentRight.length, semanticLeft: semanticLeft.length, semanticRight: semanticRight.length })}`);
      return undefined;
    }
    return { left, right };
  }

  function applyPresentation({
    presentation,
    rig,
    restPoses,
    dynamics,
    cableBuffers,
    debugLines,
    camera,
    canvas,
    delta,
    timestamp,
    assignedPointTail,
    assignedUnderlineTail,
    assignedSequence,
  }: {
    presentation: TailPresentation;
    rig: TailRig;
    restPoses: Record<TailSide, TailRestPose>;
    dynamics: Record<TailSide, TailDynamicsState>;
    cableBuffers: Record<TailSide, TailCableBuffers>;
    debugLines?: Record<TailSide, THREE.Line>;
    camera: THREE.Camera;
    canvas: HTMLCanvasElement;
    delta: number;
    timestamp: number;
    assignedPointTail?: TailSide;
    assignedUnderlineTail?: TailSide;
    assignedSequence: number;
  }) {
    const pointElement = cueElement(presentation.pointCueId);
    const underlineElement = cueElement(presentation.underlineCueId);
    if (presentation.sequenceId !== assignedSequence) {
      const primaryElement = pointElement ?? underlineElement;
      if (primaryElement) {
        const target = cueScreenPoint(primaryElement.getBoundingClientRect(), pointElement ? "point" : "underline", 0, false, _cueScreenPoint);
        const leftTip = projectTip(rig.left.at(-1)!, camera, canvas, _leftTipPoint);
        const rightTip = projectTip(rig.right.at(-1)!, camera, canvas, _rightTipPoint);
        const nearest = chooseNearestTail(leftTip, rightTip, target);
        if (pointElement) {
          assignedPointTail = nearest;
          assignedUnderlineTail = nearest === "left" ? "right" : "left";
        } else {
          assignedUnderlineTail = nearest;
          assignedPointTail = undefined;
        }
        if (assignedPointTail) seedTailForSequence(assignedPointTail, presentation.sequenceId, rig, dynamics);
        if (assignedUnderlineTail && assignedUnderlineTail !== assignedPointTail) {
          seedTailForSequence(assignedUnderlineTail, presentation.sequenceId, rig, dynamics);
        }
      }
      assignedSequence = presentation.sequenceId;
    }

    const baseStrength = presentationStrength(presentation);
    const targetSpring = presentation.phase === "point" || presentation.phase === "hold"
      ? TAIL_SPRING.target.point
      : presentation.phase === "underline"
        ? TAIL_SPRING.target.underline
        : TAIL_SPRING.target.retract;
    for (const side of TAIL_SIDES) {
      const state = dynamics[side];
      const isPointTail = assignedPointTail === side;
      const isUnderlineTail = assignedUnderlineTail === side;
      const pointEngaged = isPointTail && Boolean(pointElement);
      const underlineEngaged = isUnderlineTail && Boolean(underlineElement) && presentation.phase !== "point";
      const phaseTarget = underlineEngaged && presentation.phase === "hold"
        ? Math.max(0, Math.min(1, presentation.progress))
        : pointEngaged || underlineEngaged
          ? baseStrength
          : 0;
      stepScalarSpring(
        state.strength,
        phaseTarget,
        delta,
        TAIL_SPRING.strength.frequency,
        TAIL_SPRING.strength.damping,
      );

      let screenPoint: ScreenPoint | undefined;
      if (underlineEngaged && underlineElement) {
        const rtl = getComputedStyle(underlineElement).direction === "rtl";
        const underlineProgress = presentation.phase === "underline" ? presentation.progress : presentation.phase === "retract" ? 1 : 0;
        screenPoint = cueScreenPoint(underlineElement.getBoundingClientRect(), "underline", underlineProgress, rtl, _cueScreenPoint);
      } else if (pointEngaged && pointElement) {
        screenPoint = cueScreenPoint(pointElement.getBoundingClientRect(), "point", 0, false, _cueScreenPoint);
      }
      if (screenPoint) {
        const rawTarget = screenToWorld(screenPoint, rig[side][0], camera, canvas);
        if (rawTarget) {
          state.lastRawTarget.copy(rawTarget);
          state.hasRawTarget = true;
        }
      }
      if (state.hasRawTarget) {
        _modifiedTailTarget.copy(state.lastRawTarget);
        if (pointEngaged || underlineEngaged) {
          const offsets = presentationTargetOffset(
            presentation.phase,
            presentation.progress,
            isPointTail,
            _presentationTargetOffsets,
          );
          // Cable measurements refresh later this frame; the prior frame's root and length are sufficient for these small offsets.
          const buffers = cableBuffers[side];
          const chainRestLength = sumLengths(buffers.restWorldSegmentLengths);
          _targetPullbackDirection.subVectors(buffers.restWorldJoints[0], state.lastRawTarget);
          if (_targetPullbackDirection.lengthSq() > 1e-10) _targetPullbackDirection.normalize();
          else _targetPullbackDirection.set(0, 0, 0);
          _modifiedTailTarget
            .addScaledVector(_targetPullbackDirection, offsets.pullback * chainRestLength)
            .addScaledVector(WORLD_DOWN, offsets.droop * chainRestLength);
        }
        stepVec3Spring(state.target, _modifiedTailTarget, delta, targetSpring.frequency, targetSpring.damping);
      }

      const solved = applyCableTail(
        side,
        rig[side],
        restPoses[side],
        state,
        cableBuffers[side],
        presentation,
        pointElement,
        underlineElement,
        isPointTail,
        isUnderlineTail,
        camera,
        canvas,
        timestamp,
        debugLines?.[side],
      );
      if (
        solved
        && isUnderlineTail
        && (presentation.phase === "underline" || presentation.phase === "retract")
        && presentation.underlineCueId
        && onTailTip
      ) {
        const tip = projectTip(rig[side].at(-1)!, camera, canvas, _reportedTipPoint);
        onTailTip({ cueId: presentation.underlineCueId, x: tip.x, y: tip.y });
      }
      if (state.strength.value <= 0.01 && presentation.phase === "idle") resetTailDynamics(state);
    }
    return { assignedPointTail, assignedUnderlineTail, assignedSequence };
  }

  function seedTailForSequence(
    side: TailSide,
    sequenceId: number,
    rig: TailRig,
    dynamics: Record<TailSide, TailDynamicsState>,
  ) {
    const state = dynamics[side];
    if (state.lastSequenceId === sequenceId) return;
    rig[side].at(-1)!.getWorldPosition(_tipWorldPosition);
    seedVec3Spring(state.target, _tipWorldPosition);
    state.lastSequenceId = sequenceId;
  }

  function cueElement(cueId?: string): HTMLElement | undefined {
    return cueId ? document.querySelector<HTMLElement>(`[data-cue-id="${cueId}"]`) ?? undefined : undefined;
  }

  function projectTip(tip: THREE.Bone, camera: THREE.Camera, canvas: HTMLCanvasElement, result: ScreenPoint): ScreenPoint {
    const rect = canvas.getBoundingClientRect();
    tip.getWorldPosition(_projectedTipPosition).project(camera);
    result.x = rect.left + (_projectedTipPosition.x + 1) * rect.width / 2;
    result.y = rect.top + (1 - _projectedTipPosition.y) * rect.height / 2;
    return result;
  }

  function applyCableTail(
    side: TailSide,
    chain: THREE.Bone[],
    restPose: TailRestPose,
    state: TailDynamicsState,
    buffers: TailCableBuffers,
    presentation: TailPresentation,
    pointElement: HTMLElement | undefined,
    underlineElement: HTMLElement | undefined,
    isPointTail: boolean,
    isUnderlineTail: boolean,
    camera: THREE.Camera,
    canvas: HTMLCanvasElement,
    timestamp: number,
    debugLine?: THREE.Line,
  ): boolean {
    measureRestCable(chain, buffers, camera);
    const chainRestLength = sumLengths(buffers.restWorldSegmentLengths);
    const strength = THREE.MathUtils.clamp(state.strength.value, 0, 1);
    const engaged = strength > 0.01 && state.hasRawTarget;
    if (!engaged) {
      for (let index = 0; index < buffers.restWorldJoints.length; index += 1) {
        buffers.blendedJoints[index].copy(buffers.restWorldJoints[index]);
      }
      for (let index = 0; index < buffers.segmentLengths.length; index += 1) {
        buffers.segmentLengths[index] = buffers.restWorldSegmentLengths[index];
      }
      applyTravelingWave(
        buffers.blendedJoints,
        buffers.segmentLateral,
        timestamp / 1_000,
        side,
        strength,
        0,
        chainRestLength,
      );
      fitChainToPolyline(chain, restPose.positions, buffers.blendedJoints, buffers.segmentLengths);
      if (debugLine) debugLine.visible = false;
      return false;
    }

    const root = buffers.restWorldJoints[0];
    buffers.target.copy(state.target.position);
    const targetDistance = buffers.target.distanceTo(root);
    const requiredStretch = requiredTailStretch(targetDistance, chainRestLength);
    const stretch = THREE.MathUtils.lerp(1, requiredStretch, strength);
    distributeStretch(buffers.restWorldSegmentLengths, stretch, buffers.segmentLengths);
    const chainLength = sumLengths(buffers.segmentLengths);
    buffers.curveInput.chainLength = chainLength;
    const maximumReach = 0.97 * chainLength;
    if (targetDistance > maximumReach) buffers.target.sub(root).setLength(maximumReach).add(root);

    buffers.baseTangent.subVectors(buffers.restWorldJoints[2], root).normalize();
    resolveTipTangent(
      buffers.tipTangent,
      buffers.target,
      root,
      state.target.velocity,
      presentation,
      pointElement,
      underlineElement,
      isPointTail,
      isUnderlineTail,
      chain[0],
      camera,
      canvas,
    );
    _cameraForward.set(0, 0, -1).applyQuaternion(camera.getWorldQuaternion(_cameraQuaternion)).normalize();
    _chord.subVectors(buffers.target, root).normalize();
    buffers.bulgeDirection.crossVectors(_chord, _cameraForward);
    if (buffers.bulgeDirection.lengthSq() < 1e-10) buffers.bulgeDirection.set(side === "left" ? -1 : 1, 0, 0);
    else buffers.bulgeDirection.normalize();
    if ((side === "left" && buffers.bulgeDirection.x > 0) || (side === "right" && buffers.bulgeDirection.x < 0)) {
      buffers.bulgeDirection.negate();
    }

    buildCableCurve(buffers.curveInput, buffers.curve);
    if (buffers.curve.degenerate) {
      if (debugLine) debugLine.visible = false;
      return false;
    }
    buffers.curveJoints[0].copy(root);
    let arcDistance = 0;
    for (let index = 1; index < buffers.curveJoints.length; index += 1) {
      arcDistance += buffers.segmentLengths[index - 1];
      curvePointByArc(buffers.curve, arcDistance, buffers.curveJoints[index]);
    }
    blendGuidePolyline(buffers.restWorldJoints, buffers.curveJoints, strength, buffers.blendedJoints);
    applyTravelingWave(
      buffers.blendedJoints,
      buffers.segmentLateral,
      timestamp / 1_000,
      side,
      strength,
      presentation.phase === "underline" ? 2 : 0,
      chainRestLength,
    );
    fitChainToPolyline(chain, restPose.positions, buffers.blendedJoints, buffers.segmentLengths);
    if (debugLine) updateTailDebugLine(debugLine, buffers.curve);
    return true;
  }

  function resolveTipTangent(
    out: THREE.Vector3,
    target: THREE.Vector3,
    root: THREE.Vector3,
    velocity: THREE.Vector3,
    presentation: TailPresentation,
    pointElement: HTMLElement | undefined,
    underlineElement: HTMLElement | undefined,
    isPointTail: boolean,
    isUnderlineTail: boolean,
    rootBone: THREE.Bone,
    camera: THREE.Camera,
    canvas: HTMLCanvasElement,
  ): void {
    out.subVectors(target, root).normalize();
    if (isPointTail && pointElement) {
      const rect = pointElement.getBoundingClientRect();
      const cue = cueScreenPoint(rect, "point", 0, false, _tipScreenA);
      _tipScreenA.x = cue.x;
      _tipScreenA.y = cue.y;
      const towardX = rect.left + rect.width / 2 - cue.x;
      const towardY = rect.top + rect.height / 2 - cue.y;
      const towardLength = Math.hypot(towardX, towardY);
      if (towardLength > 1e-5) {
        _tipScreenA.x += towardX * 20 / towardLength;
        _tipScreenA.y += towardY * 20 / towardLength;
      }
      if (screenToWorld(_tipScreenA, rootBone, camera, canvas, _tipWorldA)) out.subVectors(_tipWorldA, target).normalize();
    } else if (isUnderlineTail && underlineElement && presentation.phase === "underline") {
      const rect = underlineElement.getBoundingClientRect();
      const rtl = getComputedStyle(underlineElement).direction === "rtl";
      cueScreenPoint(rect, "underline", presentation.progress, rtl, _tipScreenA);
      cueScreenPoint(rect, "underline", presentation.progress + 0.05, rtl, _tipScreenB);
      if (
        screenToWorld(_tipScreenA, rootBone, camera, canvas, _tipWorldA)
        && screenToWorld(_tipScreenB, rootBone, camera, canvas, _tipWorldB)
      ) {
        out.subVectors(_tipWorldB, _tipWorldA).normalize();
      }
    }
    if (out.lengthSq() < 1e-10) out.subVectors(target, root);
    out.addScaledVector(velocity, -TAIL_TUNING.followThrough);
    if (out.lengthSq() < 1e-10) out.set(1, 0, 0);
    else out.normalize();
  }

  function createTailCableBuffers(chain: THREE.Bone[]): TailCableBuffers {
    const restWorldJoints = chain.map(() => new THREE.Vector3());
    const curveJoints = chain.map(() => new THREE.Vector3());
    const blendedJoints = chain.map(() => new THREE.Vector3());
    const laterals = chain.map(() => new THREE.Vector3());
    const restWorldSegmentLengths = new Array<number>(Math.max(0, chain.length - 1)).fill(0);
    const segmentLengths = new Array<number>(Math.max(0, chain.length - 1)).fill(0);
    const target = new THREE.Vector3();
    const baseTangent = new THREE.Vector3();
    const tipTangent = new THREE.Vector3();
    const bulgeDirection = new THREE.Vector3();
    const curve = createCableCurve();
    const buffers: TailCableBuffers = {
      restWorldJoints,
      curveJoints,
      blendedJoints,
      laterals,
      restWorldSegmentLengths,
      segmentLengths,
      target,
      baseTangent,
      tipTangent,
      bulgeDirection,
      curve,
      curveInput: {
        root: restWorldJoints[0],
        baseTangent,
        target,
        tipTangent,
        chainLength: 0,
        bulgeDirection,
        sagDirection: WORLD_DOWN,
      },
      segmentLateral: (index) => laterals[Math.min(index, laterals.length - 1)],
    };
    return buffers;
  }

  function measureRestCable(chain: THREE.Bone[], buffers: TailCableBuffers, camera: THREE.Camera): void {
    chain[0].updateWorldMatrix(true, true);
    camera.getWorldDirection(_cameraForward).normalize();
    for (let index = 0; index < chain.length; index += 1) {
      chain[index].getWorldPosition(buffers.restWorldJoints[index]);
    }
    for (let index = 0; index < buffers.restWorldSegmentLengths.length; index += 1) {
      _restSegment.subVectors(buffers.restWorldJoints[index + 1], buffers.restWorldJoints[index]);
      buffers.restWorldSegmentLengths[index] = _restSegment.length();
      buffers.laterals[index].crossVectors(_restSegment, _cameraForward);
      if (buffers.laterals[index].lengthSq() < 1e-10) buffers.laterals[index].set(0, 1, 0);
      else buffers.laterals[index].normalize();
    }
    if (buffers.laterals.length > 1) buffers.laterals.at(-1)!.copy(buffers.laterals.at(-2)!);
  }

  function sumLengths(lengths: readonly number[]): number {
    let total = 0;
    for (let index = 0; index < lengths.length; index += 1) total += lengths[index];
    return total;
  }

  function createTailDebugLine(color: number): THREE.Line {
    const geometry = new THREE.BufferGeometry();
    geometry.setAttribute("position", new THREE.BufferAttribute(new Float32Array(TAIL_DEBUG_POINTS * 3), 3));
    const line = new THREE.Line(geometry, new THREE.LineBasicMaterial({ color, transparent: true, opacity: 0.8 }));
    line.frustumCulled = false;
    line.visible = false;
    return line;
  }

  function updateTailDebugLine(line: THREE.Line, curve: CableCurve): void {
    const positions = line.geometry.getAttribute("position") as THREE.BufferAttribute;
    for (let index = 0; index < TAIL_DEBUG_POINTS; index += 1) {
      curvePointByArc(curve, curve.totalLength * index / (TAIL_DEBUG_POINTS - 1), _debugCurvePoint);
      positions.setXYZ(index, _debugCurvePoint.x, _debugCurvePoint.y, _debugCurvePoint.z);
    }
    positions.needsUpdate = true;
    line.visible = true;
  }

  function screenToWorld(
    point: ScreenPoint,
    root: THREE.Bone,
    camera: THREE.Camera,
    canvas: HTMLCanvasElement,
    out = _screenWorldTarget,
  ): THREE.Vector3 | undefined {
    const rect = canvas.getBoundingClientRect();
    _screenNdc.set(
      ((point.x - rect.left) / rect.width) * 2 - 1,
      -((point.y - rect.top) / rect.height) * 2 + 1,
    );
    _screenRaycaster.setFromCamera(_screenNdc, camera);
    root.getWorldPosition(_screenRootPosition);
    camera.getWorldDirection(_screenPlaneNormal);
    _screenPlane.setFromNormalAndCoplanarPoint(_screenPlaneNormal, _screenRootPosition);
    return _screenRaycaster.ray.intersectPlane(_screenPlane, out) ?? undefined;
  }

  function disposeModel(model: THREE.Object3D) {
    model.traverse((object) => {
      if (!("isMesh" in object) || !object.isMesh) return;
      const mesh = object as THREE.Mesh;
      mesh.geometry.dispose();
      const materials = Array.isArray(mesh.material) ? mesh.material : [mesh.material];
      for (const material of materials) material.dispose();
    });
  }

  const TAIL_SIDES = ["left", "right"] as const;
  const TAIL_DEBUG_POINTS = 32;
  const WORLD_DOWN = new THREE.Vector3(0, -1, 0);
  const _leftTipPoint: ScreenPoint = { x: 0, y: 0 };
  const _rightTipPoint: ScreenPoint = { x: 0, y: 0 };
  const _reportedTipPoint: ScreenPoint = { x: 0, y: 0 };
  const _cueScreenPoint: ScreenPoint = { x: 0, y: 0 };
  const _tipWorldPosition = new THREE.Vector3();
  const _projectedTipPosition = new THREE.Vector3();
  const _cameraForward = new THREE.Vector3();
  const _cameraQuaternion = new THREE.Quaternion();
  const _chord = new THREE.Vector3();
  const _restSegment = new THREE.Vector3();
  const _tipWorldA = new THREE.Vector3();
  const _tipWorldB = new THREE.Vector3();
  const _debugCurvePoint = new THREE.Vector3();
  const _tipScreenA: ScreenPoint = { x: 0, y: 0 };
  const _tipScreenB: ScreenPoint = { x: 0, y: 0 };
  const _screenNdc = new THREE.Vector2();
  const _screenRaycaster = new THREE.Raycaster();
  const _screenRootPosition = new THREE.Vector3();
  const _screenPlaneNormal = new THREE.Vector3();
  const _screenPlane = new THREE.Plane();
  const _screenWorldTarget = new THREE.Vector3();
  const _modifiedTailTarget = new THREE.Vector3();
  const _targetPullbackDirection = new THREE.Vector3();
  const _presentationTargetOffsets = { pullback: 0, droop: 0 };
</script>

<div class="nono-scene" bind:this={container} aria-label="Nono 3D guide">
  {#if failed}
    <div class="fallback"><span>の</span><small>Nono is still here to help.</small></div>
  {/if}
</div>

<style>
  .nono-scene{position:absolute;inset:0;z-index:7;overflow:hidden;pointer-events:none;background:transparent}
  .nono-scene :global(canvas){display:block;width:100%;height:100%}
  .fallback{position:absolute;left:24px;top:24px;display:grid;place-content:center;text-align:center;gap:5px;color:#765f6b}.fallback span{width:68px;height:68px;display:grid;place-items:center;margin:auto;border-radius:50%;background:linear-gradient(135deg,#ff70b7,#8d7cff);color:white;font-size:31px}.fallback small{font-size:9px}
</style>
