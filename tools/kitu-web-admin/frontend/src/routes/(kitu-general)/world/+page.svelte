<script lang="ts">
  import { WorldPage, type WorldKind } from "@kitu/admin/pages";
  import type { WorldAppearance } from "@kitu/admin/ui";
  import type { WorldObject } from "@kitu/admin/client";
  const kinds: WorldKind[] = [
    { value: "spawn-point", label: "Spawn Point" },
    { value: "enemy", label: "Enemy" },
    { value: "treasure", label: "Treasure" },
    { value: "trigger", label: "Trigger" },
    { value: "marker", label: "Marker" },
  ];
  function appearance(object: WorldObject): WorldAppearance {
    return {
      shape:
        object.kind === "player"
          ? "sphere"
          : object.kind === "trigger"
            ? "torus"
            : "box",
      scale: object.kind === "treasure" ? 0.8 : 1,
    };
  }
</script>

<svelte:head><title>World - Kitu Admin</title></svelte:head>
<WorldPage {kinds} {appearance} />
