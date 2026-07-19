<script lang="ts">
  import { onMount } from "svelte";
  import * as THREE from "three";
  import { GLTFLoader } from "three/examples/jsm/loaders/GLTFLoader.js";
  import { applyHairMotion, resolveHairMotionRig, type HairMotionRig } from "./hairMotion";
  import { applyNonoMaterials, nonoAssetFromLocation, shaderVariantFromLocation } from "./nonoToon";
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
    applyTailExtension,
    captureTailRestPose,
    CURRENT_TAIL_BONES,
    requiredTailStretch,
    restoreTailRestPose,
    SEMANTIC_TAIL_BONES,
    chooseNearestTail,
    cueScreenPoint,
    presentationStrength,
    solveCcdChain,
    type ScreenPoint,
    type TailPresentation,
    type TailRestPose,
  } from "./tailPresentation";

  type NonoMood = "idle" | "think" | "present";
  type TailSide = "left" | "right";
  type TailRig = Record<TailSide, THREE.Bone[]>;

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
      if (model && tailRig && tailRestPoses && tailDynamics) {
        restoreTailRestPose(tailRig.left, tailRestPoses.left);
        restoreTailRestPose(tailRig.right, tailRestPoses.right);
        if (!reducedMotion) {
          applyTailIdle(tailRig, timestamp);
        } else {
          resetTailDynamics(tailDynamics.left);
          resetTailDynamics(tailDynamics.right);
        }
        ({ assignedPointTail, assignedUnderlineTail, assignedSequence } = applyPresentation({
          presentation,
          rig: tailRig,
          restPoses: tailRestPoses,
          dynamics: tailDynamics,
          camera,
          canvas: renderer.domElement,
          delta,
          assignedPointTail,
          assignedUnderlineTail,
          assignedSequence,
        }));
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

  function applyTailIdle(rig: TailRig, timestamp: number) {
    const sway = Math.sin(timestamp * 0.0012) * 0.018;
    rig.left[4].rotateZ(sway);
    rig.left[8].rotateZ(-sway * 0.55);
    rig.right[4].rotateZ(-sway);
    rig.right[8].rotateZ(sway * 0.55);
    rig.left[0].updateWorldMatrix(true, true);
    rig.right[0].updateWorldMatrix(true, true);
  }

  function applyPresentation({
    presentation,
    rig,
    restPoses,
    dynamics,
    camera,
    canvas,
    delta,
    assignedPointTail,
    assignedUnderlineTail,
    assignedSequence,
  }: {
    presentation: TailPresentation;
    rig: TailRig;
    restPoses: Record<TailSide, TailRestPose>;
    dynamics: Record<TailSide, TailDynamicsState>;
    camera: THREE.Camera;
    canvas: HTMLCanvasElement;
    delta: number;
    assignedPointTail?: TailSide;
    assignedUnderlineTail?: TailSide;
    assignedSequence: number;
  }) {
    const pointElement = cueElement(presentation.pointCueId);
    const underlineElement = cueElement(presentation.underlineCueId);
    if (presentation.sequenceId !== assignedSequence) {
      const primaryElement = pointElement ?? underlineElement;
      if (primaryElement) {
        const target = cueScreenPoint(primaryElement.getBoundingClientRect(), pointElement ? "point" : "underline");
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
        screenPoint = cueScreenPoint(underlineElement.getBoundingClientRect(), "underline", underlineProgress, rtl);
      } else if (pointEngaged && pointElement) {
        screenPoint = cueScreenPoint(pointElement.getBoundingClientRect(), "point");
      }
      if (screenPoint) {
        const rawTarget = screenToWorld(screenPoint, rig[side][0], camera, canvas);
        if (rawTarget) {
          state.lastRawTarget.copy(rawTarget);
          state.hasRawTarget = true;
        }
      }
      if (state.hasRawTarget) {
        stepVec3Spring(state.target, state.lastRawTarget, delta, targetSpring.frequency, targetSpring.damping);
      }

      const solved = state.strength.value > 0.01 && state.hasRawTarget && solveToward(
        rig[side],
        restPoses[side],
        state,
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

  function solveToward(
    chain: THREE.Bone[],
    restPose: TailRestPose,
    state: TailDynamicsState,
  ): boolean {
    const target = _solveTarget.copy(state.target.position);
    const root = chain[0].getWorldPosition(_solveRootPosition);
    const authoredReach = worldChainReach(chain);
    const targetDistance = target.distanceTo(root);
    const requiredStretch = requiredTailStretch(targetDistance, authoredReach);
    const strength = state.strength.value;
    applyTailExtension(chain, restPose.positions, THREE.MathUtils.lerp(1, requiredStretch, Math.max(0, Math.min(1, strength))));
    const reach = worldChainReach(chain) * 0.97;
    if (targetDistance > reach) target.sub(root).setLength(reach).add(root);
    solveCcdChain(chain, target, Math.min(0.9, strength), 4);
    return true;
  }

  function worldChainReach(chain: THREE.Bone[]): number {
    chain[0]?.updateWorldMatrix(true, true);
    let reach = 0;
    chain[0]?.getWorldPosition(_chainPreviousPosition);
    for (let index = 1; index < chain.length; index += 1) {
      const bone = chain[index];
      bone.getWorldPosition(_chainCurrentPosition);
      reach += _chainCurrentPosition.distanceTo(_chainPreviousPosition);
      _chainPreviousPosition.copy(_chainCurrentPosition);
    }
    return reach;
  }

  function screenToWorld(point: ScreenPoint, root: THREE.Bone, camera: THREE.Camera, canvas: HTMLCanvasElement): THREE.Vector3 | undefined {
    const rect = canvas.getBoundingClientRect();
    _screenNdc.set(
      ((point.x - rect.left) / rect.width) * 2 - 1,
      -((point.y - rect.top) / rect.height) * 2 + 1,
    );
    _screenRaycaster.setFromCamera(_screenNdc, camera);
    root.getWorldPosition(_screenRootPosition);
    camera.getWorldDirection(_screenPlaneNormal);
    _screenPlane.setFromNormalAndCoplanarPoint(_screenPlaneNormal, _screenRootPosition);
    return _screenRaycaster.ray.intersectPlane(_screenPlane, _screenWorldTarget) ?? undefined;
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
  const _leftTipPoint: ScreenPoint = { x: 0, y: 0 };
  const _rightTipPoint: ScreenPoint = { x: 0, y: 0 };
  const _reportedTipPoint: ScreenPoint = { x: 0, y: 0 };
  const _tipWorldPosition = new THREE.Vector3();
  const _projectedTipPosition = new THREE.Vector3();
  const _solveTarget = new THREE.Vector3();
  const _solveRootPosition = new THREE.Vector3();
  const _chainPreviousPosition = new THREE.Vector3();
  const _chainCurrentPosition = new THREE.Vector3();
  const _screenNdc = new THREE.Vector2();
  const _screenRaycaster = new THREE.Raycaster();
  const _screenRootPosition = new THREE.Vector3();
  const _screenPlaneNormal = new THREE.Vector3();
  const _screenPlane = new THREE.Plane();
  const _screenWorldTarget = new THREE.Vector3();
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
