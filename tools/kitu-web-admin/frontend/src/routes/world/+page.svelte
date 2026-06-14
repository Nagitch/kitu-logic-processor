<script lang="ts">
  import { Boxes, Move3D, Plus, RotateCcw } from "@lucide/svelte";
  import {
    moveObject,
    resetWorld,
    spawnObject,
    worldSnapshot,
  } from "$lib/admin-client";
  import WorldCanvas from "$lib/components/WorldCanvas.svelte";
  import Button from "$lib/components/ui/Button.svelte";
  import Field from "$lib/components/ui/Field.svelte";
  import Input from "$lib/components/ui/Input.svelte";
  import Panel from "$lib/components/ui/Panel.svelte";

  let kind = "spawn-point";
  let spawnX = 0;
  let spawnY = 0;
  let spawnZ = 0;
  let selectedId = "";
  let moveX = 0;
  let moveY = 0;
  let moveZ = 0;

  $: selectedObject = $worldSnapshot.objects.find(
    (object) => object.id === selectedId,
  );

  function submitSpawn() {
    spawnObject(kind, Number(spawnX), Number(spawnY), Number(spawnZ));
  }

  function selectObject(id: string) {
    const object = $worldSnapshot.objects.find(
      (candidate) => candidate.id === id,
    );
    if (!object) return;
    selectedId = object.id;
    moveX = object.x;
    moveY = object.y;
    moveZ = object.z;
  }

  function submitMove() {
    if (!selectedId) return;
    moveObject(selectedId, Number(moveX), Number(moveY), Number(moveZ));
  }
</script>

<svelte:head>
  <title>World - Kitu Admin</title>
</svelte:head>

<div
  class="grid min-w-0 grid-cols-[minmax(0,1fr)_360px] gap-4 max-xl:grid-cols-1"
>
  <Panel title="World View" eyebrow="Scene">
    <div class="h-[620px] min-h-[420px] min-w-0">
      <WorldCanvas objects={$worldSnapshot.objects} />
    </div>
  </Panel>

  <div class="grid min-w-0 content-start gap-4">
    <Panel title="Place Object" eyebrow="OSC /admin/world/spawn">
      <form
        class="grid gap-3"
        onsubmit={(event) => {
          event.preventDefault();
          submitSpawn();
        }}
      >
        <Field label="Kind">
          <select
            bind:value={kind}
            class="h-10 rounded-md border border-input bg-background px-3 text-sm outline-none focus:border-ring focus:ring-2 focus:ring-ring/20"
          >
            <option value="spawn-point">Spawn Point</option>
            <option value="enemy">Enemy</option>
            <option value="treasure">Treasure</option>
            <option value="trigger">Trigger</option>
            <option value="marker">Marker</option>
          </select>
        </Field>
        <div class="grid grid-cols-3 gap-2">
          <Field label="X">
            <Input type="number" step="0.5" bind:value={spawnX} />
          </Field>
          <Field label="Y">
            <Input type="number" step="0.5" bind:value={spawnY} />
          </Field>
          <Field label="Z">
            <Input type="number" step="0.5" bind:value={spawnZ} />
          </Field>
        </div>
        <Button type="submit" class="w-full">
          <Plus size={16} />
          Spawn
        </Button>
      </form>
    </Panel>

    <Panel title="Move Object" eyebrow="OSC /admin/world/move">
      <form
        class="grid gap-3"
        onsubmit={(event) => {
          event.preventDefault();
          submitMove();
        }}
      >
        <Field label="Object">
          <select
            bind:value={selectedId}
            onchange={() => selectObject(selectedId)}
            class="h-10 rounded-md border border-input bg-background px-3 text-sm outline-none focus:border-ring focus:ring-2 focus:ring-ring/20"
          >
            <option value="">Select</option>
            {#each $worldSnapshot.objects as object}
              <option value={object.id}>{object.id} · {object.kind}</option>
            {/each}
          </select>
        </Field>
        <div class="grid grid-cols-3 gap-2">
          <Field label="X">
            <Input
              type="number"
              step="0.5"
              bind:value={moveX}
              disabled={!selectedObject}
            />
          </Field>
          <Field label="Y">
            <Input
              type="number"
              step="0.5"
              bind:value={moveY}
              disabled={!selectedObject}
            />
          </Field>
          <Field label="Z">
            <Input
              type="number"
              step="0.5"
              bind:value={moveZ}
              disabled={!selectedObject}
            />
          </Field>
        </div>
        <Button
          type="submit"
          class="w-full"
          variant="secondary"
          disabled={!selectedObject}
        >
          <Move3D size={16} />
          Move
        </Button>
      </form>
    </Panel>

    <Panel title="Objects" eyebrow="World State">
      <div class="mb-3 flex justify-end">
        <Button
          variant="outline"
          size="icon"
          aria-label="Reset world"
          onclick={resetWorld}
        >
          <RotateCcw size={15} />
        </Button>
      </div>
      <div class="grid max-h-80 gap-2 overflow-auto pr-1">
        {#each $worldSnapshot.objects as object}
          <button
            type="button"
            class={`grid rounded-md border px-3 py-2 text-left transition-colors ${
              object.id === selectedId
                ? "border-accent bg-accent/10"
                : "border-border bg-white hover:bg-muted"
            }`}
            onclick={() => selectObject(object.id)}
          >
            <span class="flex items-center gap-2 text-sm font-semibold">
              <span
                class="h-3 w-3 rounded-sm"
                style={`background: ${object.color}`}
              ></span>
              <Boxes size={14} />
              {object.id}
            </span>
            <span class="mt-1 text-xs text-muted-foreground">
              {object.kind} · {object.x.toFixed(1)}, {object.y.toFixed(1)}, {object.z.toFixed(
                1,
              )}
            </span>
          </button>
        {:else}
          <p class="text-sm text-muted-foreground">No objects</p>
        {/each}
      </div>
    </Panel>
  </div>
</div>
