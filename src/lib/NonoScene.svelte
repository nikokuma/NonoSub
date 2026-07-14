<script lang="ts">
  import { onMount } from "svelte";
  import * as THREE from "three";
  import { GLTFLoader } from "three/examples/jsm/loaders/GLTFLoader.js";

  let container: HTMLDivElement;
  let failed = $state(false);

  onMount(() => {
    const scene = new THREE.Scene();
    const camera = new THREE.PerspectiveCamera(28, 1, 0.1, 100);
    camera.position.set(0, 1.3, 4.1);
    camera.lookAt(0, 1.2, 0);

    const renderer = new THREE.WebGLRenderer({ alpha: true, antialias: true });
    renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
    renderer.outputColorSpace = THREE.SRGBColorSpace;
    container.appendChild(renderer.domElement);

    scene.add(new THREE.HemisphereLight(0xf7e7ff, 0x30295a, 2.8));
    const key = new THREE.DirectionalLight(0xffe8f4, 4.2);
    key.position.set(2.5, 4, 3);
    scene.add(key);
    const rim = new THREE.DirectionalLight(0x8d7cff, 3.2);
    rim.position.set(-3, 2, -2);
    scene.add(rim);

    let model: THREE.Object3D | undefined;
    const loader = new GLTFLoader();
    loader.load("/assets/Nono.glb", (gltf) => {
      model = gltf.scene;
      const bounds = new THREE.Box3().setFromObject(model);
      const size = bounds.getSize(new THREE.Vector3());
      const center = bounds.getCenter(new THREE.Vector3());
      const scale = 2.45 / Math.max(size.y, 0.001);
      model.scale.setScalar(scale);
      model.position.set(-center.x * scale, -bounds.min.y * scale - 1.14, -center.z * scale);
      scene.add(model);
    }, undefined, () => { failed = true; });

    let animationFrame = 0;
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

    const startedAt = performance.now();
    const render = (timestamp: number) => {
      const elapsed = (timestamp - startedAt) / 1_000;
      if (model) {
        model.rotation.y = Math.sin(elapsed * 0.55) * 0.025;
        model.position.y += Math.sin(elapsed * 1.1) * 0.00018;
      }
      renderer.render(scene, camera);
      animationFrame = requestAnimationFrame(render);
    };
    animationFrame = requestAnimationFrame(render);

    return () => {
      cancelAnimationFrame(animationFrame);
      observer.disconnect();
      renderer.dispose();
      renderer.domElement.remove();
    };
  });
</script>

<div class="scene" bind:this={container} aria-label="Nono 3D guide">
  {#if failed}
    <div class="fallback"><span>の</span><small>Nono is still here to help.</small></div>
  {/if}
</div>

<style>
  .scene { position: relative; width: 100%; height: 205px; overflow: hidden; background: radial-gradient(circle at 50% 76%, rgba(255,114,182,.18), transparent 58%); }
  .scene :global(canvas) { display: block; width: 100%; height: 100%; }
  .fallback { position: absolute; inset: 0; display: grid; place-content: center; text-align: center; gap: 8px; color: var(--muted); }
  .fallback span { width: 70px; height: 70px; display: grid; place-items: center; margin: auto; border-radius: 50%; background: linear-gradient(135deg, var(--pink), var(--violet)); color: white; font-size: 32px; }
  .fallback small { font-size: 11px; }
</style>
