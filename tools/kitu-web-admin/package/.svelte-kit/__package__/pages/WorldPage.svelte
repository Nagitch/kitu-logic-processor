<script lang="ts">
  import { Boxes, Move3D, Plus, RotateCcw } from '@lucide/svelte'
  import { getAdminClient } from '../client/context.js'
  import type { WorldObject } from '../client/types.js'
  import Button from '../ui/Button.svelte'
  import Field from '../ui/Field.svelte'
  import Input from '../ui/Input.svelte'
  import Panel from '../ui/Panel.svelte'
  import WorldCanvas, { type WorldAppearance } from '../ui/WorldCanvas.svelte'
  export type WorldKind = { value: string; label: string }
  let { kinds, appearance = defaultAppearance }: { kinds: WorldKind[]; appearance?: (object: WorldObject) => WorldAppearance } =
    $props()
  const client = getAdminClient()
  let kind = $state('')
  let spawnX = $state(0),
    spawnY = $state(0),
    spawnZ = $state(0)
  let selectedId = $state(''),
    moveX = $state(0),
    moveY = $state(0),
    moveZ = $state(0)
  $effect(() => {
    if (!kind && kinds[0]) kind = kinds[0].value
  })
  let selectedObject = $derived($client.worldSnapshot.objects.find(object => object.id === selectedId))
  function selectObject(id: string) {
    const object = $client.worldSnapshot.objects.find(candidate => candidate.id === id)
    if (!object) return
    selectedId = object.id
    moveX = object.x
    moveY = object.y
    moveZ = object.z
  }
  function defaultAppearance(_object: WorldObject): WorldAppearance {
    return { shape: 'box', scale: 1 }
  }
</script>

<div class="grid min-w-0 grid-cols-[minmax(0,1fr)_360px] gap-4 max-xl:grid-cols-1">
  <Panel title="World View" eyebrow="Scene">
    <div class="h-[620px] min-h-[420px] min-w-0"><WorldCanvas objects={$client.worldSnapshot.objects} {appearance} /></div>
  </Panel>
  <div class="grid min-w-0 content-start gap-4">
    <Panel title="Place Object" eyebrow="OSC /admin/world/spawn">
      <form
        class="grid gap-3"
        onsubmit={event => {
          event.preventDefault()
          void client.spawnObject(kind, Number(spawnX), Number(spawnY), Number(spawnZ))
        }}>
        <Field label="Kind">
          <select bind:value={kind} class="h-10 rounded-md border border-input bg-background px-3 text-sm">
            {#each kinds as option}<option value={option.value}>{option.label}</option>{/each}
          </select>
        </Field>
        <div class="grid grid-cols-3 gap-2">
          <Field label="X"><Input type="number" step="0.5" bind:value={spawnX} /></Field><Field label="Y">
            <Input type="number" step="0.5" bind:value={spawnY} />
          </Field><Field label="Z"><Input type="number" step="0.5" bind:value={spawnZ} /></Field>
        </div>
        <Button type="submit" class="w-full"><Plus size={16} />Spawn</Button>
      </form>
    </Panel><Panel title="Move Object" eyebrow="OSC /admin/world/move">
      <form
        class="grid gap-3"
        onsubmit={event => {
          event.preventDefault()
          if (selectedId) void client.moveObject(selectedId, Number(moveX), Number(moveY), Number(moveZ))
        }}>
        <Field label="Object">
          <select
            bind:value={selectedId}
            onchange={() => selectObject(selectedId)}
            class="h-10 rounded-md border border-input bg-background px-3 text-sm">
            <option value="">Select</option>
            {#each $client.worldSnapshot.objects as object}<option value={object.id}>{object.id} · {object.kind}</option>{/each}
          </select>
        </Field>
        <div class="grid grid-cols-3 gap-2">
          <Field label="X"><Input type="number" step="0.5" bind:value={moveX} disabled={!selectedObject} /></Field><Field
            label="Y">
            <Input type="number" step="0.5" bind:value={moveY} disabled={!selectedObject} />
          </Field><Field label="Z"><Input type="number" step="0.5" bind:value={moveZ} disabled={!selectedObject} /></Field>
        </div>
        <Button type="submit" class="w-full" variant="secondary" disabled={!selectedObject}><Move3D size={16} />Move</Button>
      </form>
    </Panel><Panel title="Objects" eyebrow="World State">
      <div class="mb-3 flex justify-end">
        <Button variant="outline" size="icon" aria-label="Reset world" onclick={() => client.resetWorld()}>
          <RotateCcw size={15} />
        </Button>
      </div>
      <div class="grid max-h-80 gap-2 overflow-auto pr-1">
        {#each $client.worldSnapshot.objects as object}<button
            type="button"
            class={`grid rounded-md border px-3 py-2 text-left ${object.id === selectedId ? 'border-accent bg-accent/10' : 'border-border bg-white hover:bg-muted'}`}
            onclick={() => selectObject(object.id)}>
            <span class="flex items-center gap-2 text-sm font-semibold">
              <span class="h-3 w-3 rounded-sm" style={`background: ${object.color}`}></span>
              <Boxes size={14} />{object.id}
            </span>
            <span class="mt-1 text-xs text-muted-foreground">
              {object.kind} · {object.x.toFixed(1)}, {object.y.toFixed(1)}, {object.z.toFixed(1)}
            </span>
          </button>{:else}<p class="text-sm text-muted-foreground">No objects</p>{/each}
      </div>
    </Panel>
  </div>
</div>
