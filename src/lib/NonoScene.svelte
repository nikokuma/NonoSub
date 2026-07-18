<script lang="ts">
  import { onMount } from "svelte";
  import * as THREE from "three";
  import { GLTFLoader } from "three/examples/jsm/loaders/GLTFLoader.js";
  import { applyHairMotion, resolveHairMotionRig, type HairMotionRig } from "./hairMotion";
  import { applyNonoMaterials, nonoAssetFromLocation, shaderVariantFromLocation } from "./nonoToon";
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
  }: {
    presentation: TailPresentation;
    mood?: NonoMood;
    onRigStatus?: (available: boolean) => void;
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
      tailRig = resolveTailRig(model);
      if (tailRig) {
        tailRestPoses = {
          left: captureTailRestPose(tailRig.left),
          right: captureTailRestPose(tailRig.right),
        };
        onRigStatus?.(true);
      } else {
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
      if (model && tailRig && tailRestPoses) {
        restoreTailRestPose(tailRig.left, tailRestPoses.left);
        restoreTailRestPose(tailRig.right, tailRestPoses.right);
        if (!reducedMotion) applyTailIdle(tailRig, timestamp);
        ({ assignedPointTail, assignedUnderlineTail, assignedSequence } = applyPresentation({
          presentation,
          rig: tailRig,
          restPoses: tailRestPoses,
          camera,
          canvas: renderer.domElement,
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
    camera,
    canvas,
    assignedPointTail,
    assignedUnderlineTail,
    assignedSequence,
  }: {
    presentation: TailPresentation;
    rig: TailRig;
    restPoses: Record<TailSide, TailRestPose>;
    camera: THREE.Camera;
    canvas: HTMLCanvasElement;
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
        const leftTip = projectTip(rig.left.at(-1)!, camera, canvas);
        const rightTip = projectTip(rig.right.at(-1)!, camera, canvas);
        const nearest = chooseNearestTail(leftTip, rightTip, target);
        if (pointElement) {
          assignedPointTail = nearest;
          assignedUnderlineTail = nearest === "left" ? "right" : "left";
        } else {
          assignedUnderlineTail = nearest;
          assignedPointTail = undefined;
        }
      }
      assignedSequence = presentation.sequenceId;
    }

    const strength = presentationStrength(presentation);
    if (strength > 0 && pointElement && assignedPointTail) {
      const point = cueScreenPoint(pointElement.getBoundingClientRect(), "point");
      solveToward(rig[assignedPointTail], restPoses[assignedPointTail], point, camera, canvas, strength);
    }
    if (strength > 0 && underlineElement && assignedUnderlineTail && presentation.phase !== "point") {
      const rtl = getComputedStyle(underlineElement).direction === "rtl";
      const underlineProgress = presentation.phase === "underline" ? presentation.progress : presentation.phase === "retract" ? 1 : 0;
      const point = cueScreenPoint(underlineElement.getBoundingClientRect(), "underline", underlineProgress, rtl);
      const underlineStrength = presentation.phase === "hold" ? Math.max(0, Math.min(1, presentation.progress)) : strength;
      solveToward(rig[assignedUnderlineTail], restPoses[assignedUnderlineTail], point, camera, canvas, underlineStrength);
    }
    return { assignedPointTail, assignedUnderlineTail, assignedSequence };
  }

  function cueElement(cueId?: string): HTMLElement | undefined {
    return cueId ? document.querySelector<HTMLElement>(`[data-cue-id="${cueId}"]`) ?? undefined : undefined;
  }

  function projectTip(tip: THREE.Bone, camera: THREE.Camera, canvas: HTMLCanvasElement): ScreenPoint {
    const rect = canvas.getBoundingClientRect();
    const point = tip.getWorldPosition(new THREE.Vector3()).project(camera);
    return { x: rect.left + (point.x + 1) * rect.width / 2, y: rect.top + (1 - point.y) * rect.height / 2 };
  }

  function solveToward(chain: THREE.Bone[], restPose: TailRestPose, screenPoint: ScreenPoint, camera: THREE.Camera, canvas: HTMLCanvasElement, strength: number) {
    const target = screenToWorld(screenPoint, chain[0], camera, canvas);
    if (!target) return;
    const root = chain[0].getWorldPosition(new THREE.Vector3());
    const authoredReach = worldChainReach(chain);
    const targetDistance = target.distanceTo(root);
    const requiredStretch = requiredTailStretch(targetDistance, authoredReach);
    applyTailExtension(chain, restPose.positions, THREE.MathUtils.lerp(1, requiredStretch, Math.max(0, Math.min(1, strength))));
    const reach = worldChainReach(chain) * 0.97;
    if (targetDistance > reach) target.sub(root).setLength(reach).add(root);
    solveCcdChain(chain, target, Math.min(0.9, strength), 4);
  }

  function worldChainReach(chain: THREE.Bone[]): number {
    chain[0]?.updateWorldMatrix(true, true);
    let reach = 0;
    const previous = new THREE.Vector3();
    const current = new THREE.Vector3();
    chain[0]?.getWorldPosition(previous);
    for (const bone of chain.slice(1)) {
      bone.getWorldPosition(current);
      reach += current.distanceTo(previous);
      previous.copy(current);
    }
    return reach;
  }

  function screenToWorld(point: ScreenPoint, root: THREE.Bone, camera: THREE.Camera, canvas: HTMLCanvasElement): THREE.Vector3 | undefined {
    const rect = canvas.getBoundingClientRect();
    const ndc = new THREE.Vector2(
      ((point.x - rect.left) / rect.width) * 2 - 1,
      -((point.y - rect.top) / rect.height) * 2 + 1,
    );
    const ray = new THREE.Raycaster();
    ray.setFromCamera(ndc, camera);
    const rootPosition = root.getWorldPosition(new THREE.Vector3());
    const normal = camera.getWorldDirection(new THREE.Vector3());
    const plane = new THREE.Plane().setFromNormalAndCoplanarPoint(normal, rootPosition);
    return ray.ray.intersectPlane(plane, new THREE.Vector3()) ?? undefined;
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
