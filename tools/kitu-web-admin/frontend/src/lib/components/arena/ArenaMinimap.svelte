<script lang="ts">
  import {
    entities,
    mapPoint,
    mapAim,
    type InspectionSnapshot,
  } from "$lib/arena-inspection";
  let {
    snapshot,
    selected,
    onselect,
  }: {
    snapshot: InspectionSnapshot;
    selected: string | null;
    onselect: (key: string) => void;
  } = $props();
  let actors = $derived(entities(snapshot));
  const colors = {
    player: "#2563eb",
    enemy: "#dc2626",
    projectile: "#a16207",
    grenade: "#7c3aed",
    effect: "#059669",
  };
  let player = $derived(
    mapPoint(snapshot.state.playerPosition, snapshot.arena),
  );
  let aim = $derived(
    mapAim(
      snapshot.state.playerPosition,
      snapshot.state.aimDirection,
      snapshot.arena,
    ),
  );
</script>

<div class="mx-auto max-w-[540px]">
  <svg
    viewBox="-5 -5 110 110"
    class="w-full rounded bg-slate-50"
    aria-label="Arena minimap. Select an entity to inspect it; north is positive Y."
  >
    <title>Arena at tick {snapshot.state.tick}</title>
    <rect
      x="0"
      y="0"
      width="100"
      height="100"
      fill="white"
      stroke="#94a3b8"
      stroke-width="0.4"
    />
    <path d="M50 0V100 M0 50H100" stroke="#e2e8f0" stroke-width="0.25" />
    <text x="50" y="-1.7" text-anchor="middle" font-size="2.4" fill="#64748b"
      >+Y · 10</text
    >
    <text x="50" y="103.5" text-anchor="middle" font-size="2.4" fill="#64748b"
      >−10</text
    >
    {#if snapshot.state.chestAvailable}
      {@const p = mapPoint(snapshot.arena.chest, snapshot.arena)}
      <g
        ><title
          >Chest · interaction radius {snapshot.arena.chest
            .interactionRadius}</title
        ><circle
          cx={p.x}
          cy={p.y}
          r={snapshot.arena.chest.interactionRadius * 5}
          fill="#fef3c7"
          stroke="#d97706"
          stroke-width="0.25"
          stroke-dasharray="1 1"
        /><rect
          x={p.x - 1.5}
          y={p.y - 1.5}
          width="3"
          height="3"
          fill="#d97706"
        /></g
      >
    {/if}
    {#if snapshot.state.portalAvailable}
      {@const p = mapPoint(snapshot.arena.portal, snapshot.arena)}
      <g
        ><title
          >Portal · trigger radius {snapshot.arena.portal.triggerRadius}</title
        ><circle
          cx={p.x}
          cy={p.y}
          r={snapshot.arena.portal.triggerRadius * 5}
          fill="#ede9fe"
          stroke="#7c3aed"
          stroke-width="0.4"
        /></g
      >
    {/if}
    {#each snapshot.presentation.bosses as cue (cue.id)}
      {@const enemy = snapshot.state.enemies.find((v) => v.Id === cue.entityId)}
      {#if enemy}
        {@const p = mapPoint(enemy.Position, snapshot.arena)}
        <circle
          cx={p.x}
          cy={p.y}
          r={cue.radius * 5}
          fill="#f97316"
          fill-opacity={Math.min(0.25, cue.intensity * 0.2)}
          stroke="#ea580c"
          stroke-width="0.4"
          stroke-dasharray="1 1"
          ><title
            >Boss cue · radius {cue.radius} · intensity {cue.intensity}</title
          ></circle
        >
      {/if}
    {/each}
    {#each snapshot.state.grenades as grenade (grenade.Id)}
      {@const p = mapPoint(grenade.Position, snapshot.arena)}
      {@const target = mapPoint(grenade.Target, snapshot.arena)}
      <path
        d={`M${p.x} ${p.y}L${target.x} ${target.y} M${target.x - 1} ${target.y - 1}l2 2 M${target.x + 1} ${target.y - 1}l-2 2`}
        stroke="#7c3aed"
        stroke-width="0.3"
        stroke-dasharray="0.7 0.7"
        fill="none"
      />
    {/each}
    <line
      x1={player.x}
      y1={player.y}
      x2={aim.x}
      y2={aim.y}
      stroke="#2563eb"
      stroke-width="0.6"
    />
    {#each actors as entity (entity.key)}
      {@const p = mapPoint(entity.position, snapshot.arena)}
      <g
        role="button"
        tabindex="0"
        aria-label={`Inspect ${entity.label}`}
        aria-pressed={selected === entity.key}
        data-entity-key={entity.key}
        onclick={() => onselect(entity.key)}
        onkeydown={(event) => {
          if (event.key === "Enter" || event.key === " ") {
            event.preventDefault();
            onselect(entity.key);
          }
        }}
        class="cursor-pointer outline-none focus:stroke-black"
      >
        <title
          >{entity.label} · ({entity.position.x}, {entity.position
            .y}){entity.health === undefined
            ? ""
            : ` · HP ${entity.health}/${entity.maxHealth}`}</title
        >
        <circle
          cx={p.x}
          cy={p.y}
          r={Math.max(2, (entity.radius ?? 0.12) * 5)}
          fill="transparent"
        />
        <circle
          cx={p.x}
          cy={p.y}
          r={(entity.radius ?? 0.12) * 5}
          class="entity-circle"
          fill={entity.kind === "projectile" &&
          (entity.raw as { EnemyOwned: boolean }).EnemyOwned
            ? "#be123c"
            : colors[entity.kind]}
          fill-opacity={entity.kind === "effect" ? 0.15 : 0.75}
          stroke={selected === entity.key ? "#0f172a" : colors[entity.kind]}
          stroke-width={selected === entity.key ? 0.7 : 0.3}
        />
        {#if entity.health !== undefined && entity.maxHealth && entity.maxHealth > 0}
          <line
            x1={p.x - 2}
            y1={p.y - (entity.radius ?? 0) * 5 - 1}
            x2={p.x -
              2 +
              4 * Math.max(0, Math.min(1, entity.health / entity.maxHealth))}
            y2={p.y - (entity.radius ?? 0) * 5 - 1}
            stroke="#16a34a"
            stroke-width="0.5"
          />
        {/if}
      </g>
    {/each}
  </svg>
  <div
    class="mt-2 flex flex-wrap gap-x-4 gap-y-1 text-xs text-muted-foreground"
  >
    {#each Object.entries(colors) as [kind, color]}<span
        class="flex items-center gap-1.5"
        ><span class="h-2 w-2 rounded-full" style:background={color}
        ></span>{kind}</span
      >{/each}
  </div>
  <p class="mt-2 text-xs text-muted-foreground">
    Authoritative positions and radii · 20 × 20 units · select with click, Enter
    or Space. Enemy projectiles are rose; player projectiles are ochre. Grenade
    dots mark position; dashed lines show targets.
  </p>
</div>

<style>
  g[role="button"]:focus .entity-circle {
    stroke: #0f172a;
    stroke-width: 1;
  }
</style>
