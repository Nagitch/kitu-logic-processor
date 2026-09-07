<script lang="ts">
  import { onDestroy, onMount } from 'svelte'
  import type { WorldObject } from '../client/types.js'
  import type { BufferGeometry, Mesh, MeshStandardMaterial, PerspectiveCamera, Scene, WebGLRenderer } from 'three'
  type ThreeModule = typeof import('three')
  export type WorldAppearance = { shape?: 'box' | 'sphere' | 'torus'; scale?: number; color?: string }
  let { objects, appearance = () => ({}) }: { objects: WorldObject[]; appearance?: (object: WorldObject) => WorldAppearance } =
    $props()
  let host: HTMLDivElement
  let renderer: WebGLRenderer | undefined
  let scene: Scene | undefined
  let camera: PerspectiveCamera | undefined
  let frame = 0
  let THREE: ThreeModule | undefined
  const meshes = new Map<string, Mesh<BufferGeometry, MeshStandardMaterial>>()
  onMount(() => {
    let observer: ResizeObserver | undefined
    let cancelled = false
    void (async () => {
      THREE = await import('three')
      if (cancelled) return
      scene = new THREE.Scene()
      scene.background = new THREE.Color(0xf8fafc)
      camera = new THREE.PerspectiveCamera(42, 1, 0.1, 1000)
      camera.position.set(22, 24, 22)
      camera.lookAt(0, 0, 0)
      renderer = new THREE.WebGLRenderer({ antialias: true })
      renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2))
      renderer.domElement.style.width = '100%'
      renderer.domElement.style.height = '100%'
      host.appendChild(renderer.domElement)
      scene.add(new THREE.GridHelper(36, 18, 0x94a3b8, 0xd1d5db))
      scene.add(new THREE.AmbientLight(0xffffff, 0.8))
      const light = new THREE.DirectionalLight(0xffffff, 1.2)
      light.position.set(8, 14, 6)
      scene.add(light)
      const resize = () => {
        const rect = host.getBoundingClientRect()
        renderer?.setSize(rect.width, rect.height, false)
        if (camera) {
          camera.aspect = rect.width / Math.max(rect.height, 1)
          camera.updateProjectionMatrix()
        }
      }
      observer = new ResizeObserver(resize)
      observer.observe(host)
      resize()
      sync()
      const render = () => {
        frame = requestAnimationFrame(render)
        for (const mesh of meshes.values()) mesh.rotation.y += 0.006
        if (scene && camera) renderer?.render(scene, camera)
      }
      render()
    })()
    return () => {
      cancelled = true
      observer?.disconnect()
    }
  })
  onDestroy(() => {
    cancelAnimationFrame(frame)
    for (const mesh of meshes.values()) {
      mesh.geometry.dispose()
      mesh.material.dispose()
    }
    renderer?.dispose()
  })
  $effect(() => {
    objects
    appearance
    if (scene && THREE) sync()
  })
  function sync() {
    if (!scene || !THREE) return
    const live = new Set(objects.map(object => object.id))
    for (const [id, mesh] of meshes)
      if (!live.has(id)) {
        scene.remove(mesh)
        mesh.geometry.dispose()
        mesh.material.dispose()
        meshes.delete(id)
      }
    for (const object of objects) {
      const look = appearance(object)
      let mesh = meshes.get(object.id)
      if (!mesh) {
        const shape = look.shape ?? 'box'
        const geometry =
          shape === 'sphere'
            ? new THREE.SphereGeometry(0.55, 24, 16)
            : shape === 'torus'
              ? new THREE.TorusGeometry(0.55, 0.15, 12, 32)
              : new THREE.BoxGeometry(1, 1, 1)
        mesh = new THREE.Mesh(
          geometry,
          new THREE.MeshStandardMaterial({ color: look.color ?? object.color, roughness: 0.55, metalness: 0.08 })
        )
        meshes.set(object.id, mesh)
        scene.add(mesh)
      }
      mesh.position.set(object.x, 0.5 + object.y, object.z)
      mesh.scale.setScalar(look.scale ?? 1)
    }
  }
</script>

<div
  bind:this={host}
  class="h-full min-h-[360px] w-full min-w-0 overflow-hidden rounded-md border border-border bg-white [contain:layout_paint_size]">
</div>
