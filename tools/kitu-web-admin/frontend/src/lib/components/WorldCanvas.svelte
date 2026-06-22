<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import { browser } from "$app/environment";
  import type { WorldObject } from "$lib/types";
  import type {
    BufferGeometry,
    Mesh,
    MeshStandardMaterial,
    PerspectiveCamera,
    Scene,
    WebGLRenderer,
  } from "three";

  type ThreeModule = typeof import("three");

  let { objects }: { objects: WorldObject[] } = $props();

  let host: HTMLDivElement;
  let renderer: WebGLRenderer | undefined;
  let scene: Scene | undefined;
  let camera: PerspectiveCamera | undefined;
  let animationFrame = 0;
  let THREE: ThreeModule | undefined;
  const meshes = new Map<string, Mesh<BufferGeometry, MeshStandardMaterial>>();

  onMount(() => {
    let observer: ResizeObserver | undefined;
    let cancelled = false;

    void (async () => {
      THREE = await import("three");
      if (cancelled) return;
      scene = new THREE.Scene();
      scene.background = new THREE.Color(0xf8fafc);

      camera = new THREE.PerspectiveCamera(42, 1, 0.1, 1000);
      camera.position.set(22, 24, 22);
      camera.lookAt(0, 0, 0);

      renderer = new THREE.WebGLRenderer({ antialias: true });
      renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
      renderer.domElement.style.width = "100%";
      renderer.domElement.style.height = "100%";
      host.appendChild(renderer.domElement);

      const grid = new THREE.GridHelper(36, 18, 0x94a3b8, 0xd1d5db);
      scene.add(grid);

      const ambient = new THREE.AmbientLight(0xffffff, 0.8);
      scene.add(ambient);
      const directional = new THREE.DirectionalLight(0xffffff, 1.2);
      directional.position.set(8, 14, 6);
      scene.add(directional);

      const resize = () => {
        const rect = host.getBoundingClientRect();
        renderer?.setSize(rect.width, rect.height, false);
        if (camera) {
          camera.aspect = rect.width / Math.max(rect.height, 1);
          camera.updateProjectionMatrix();
        }
      };

      observer = new ResizeObserver(resize);
      observer.observe(host);
      resize();
      syncObjects();

      const render = () => {
        animationFrame = requestAnimationFrame(render);
        for (const mesh of meshes.values()) {
          mesh.rotation.y += 0.006;
        }
        if (scene && camera) {
          renderer?.render(scene, camera);
        }
      };
      render();
    })();

    return () => {
      cancelled = true;
      observer?.disconnect();
    };
  });

  onDestroy(() => {
    if (browser) {
      cancelAnimationFrame(animationFrame);
      renderer?.dispose();
    }
  });

  $effect(() => {
    const currentObjects = objects;
    if (scene && THREE) {
      syncObjects(currentObjects);
    }
  });

  function syncObjects(currentObjects = objects) {
    if (!scene || !THREE) return;

    const liveIds = new Set(currentObjects.map((object) => object.id));

    for (const [id, mesh] of meshes) {
      if (!liveIds.has(id)) {
        scene.remove(mesh);
        mesh.geometry.dispose();
        mesh.material.dispose();
        meshes.delete(id);
      }
    }

    for (const object of currentObjects) {
      let mesh = meshes.get(object.id);
      if (!mesh) {
        const geometry = geometryForKind(object.kind);
        const material = new THREE.MeshStandardMaterial({
          color: object.color,
          roughness: 0.55,
          metalness: 0.08,
        });
        mesh = new THREE.Mesh(geometry, material);
        meshes.set(object.id, mesh);
        scene.add(mesh);
      }
      mesh.position.set(object.x, 0.5 + object.y, object.z);
      mesh.scale.setScalar(object.kind === "treasure" ? 0.8 : 1);
    }
  }

  function geometryForKind(kind: string): BufferGeometry {
    if (!THREE) {
      throw new Error("Three.js is not ready");
    }
    if (kind === "player") {
      return new THREE.SphereGeometry(0.55, 24, 16);
    }
    if (kind === "trigger") {
      return new THREE.TorusGeometry(0.55, 0.15, 12, 32);
    }
    return new THREE.BoxGeometry(1, 1, 1);
  }
</script>

<div
  bind:this={host}
  class="h-full min-h-[360px] w-full min-w-0 overflow-hidden rounded-md border border-border bg-white [contain:layout_paint_size]"
></div>
