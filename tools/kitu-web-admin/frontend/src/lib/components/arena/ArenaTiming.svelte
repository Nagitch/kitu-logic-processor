<script lang="ts">
  import type { InspectionTiming } from "$lib/arena-inspection";
  let { timing }: { timing: InspectionTiming } = $props();
  const ms = (value: number | null) =>
    value === null ? "—" : `${(value / 1000).toFixed(3)} ms`;
  let ceiling = $derived(
    Math.max(timing.budgetUs * 1.5, ...timing.samples.map((s) => s.durationUs)),
  );
  let points = $derived(
    timing.samples
      .map(
        (sample, i) =>
          `${(i * 300) / Math.max(1, timing.samples.length - 1)},${70 - (sample.durationUs / ceiling) * 65}`,
      )
      .join(" "),
  );
  let clipped = $derived(
    timing.samples.some((s) => s.durationClamped || s.lockWaitClamped),
  );
</script>

<div class="grid grid-cols-2 gap-3 sm:grid-cols-4">
  {#each [["Latest", timing.last?.durationUs ?? null], ["Mean", timing.meanUs], ["p95", timing.p95Us], ["Maximum", timing.maxUs]] as [label, value]}<div
    >
      <p class="text-xs text-muted-foreground">{label}</p>
      <p class="font-mono text-sm font-semibold">
        {ms(value as number | null)}
      </p>
    </div>{/each}
</div>
<svg
  viewBox="0 0 300 80"
  class="mt-3 w-full"
  role="img"
  aria-label="Host update durations, oldest to newest. Dashed line is the 60 Hz budget."
  ><title>Retained host update durations</title><line
    x1="0"
    x2="300"
    y1={70 - (timing.budgetUs / ceiling) * 65}
    y2={70 - (timing.budgetUs / ceiling) * 65}
    stroke="#f97316"
    stroke-width="0.7"
    stroke-dasharray="3 2"
  /><polyline {points} fill="none" stroke="#2563eb" stroke-width="1" /></svg
>
<p class="text-xs text-muted-foreground">
  Host update · budget 16.667 ms · {timing.samples.length}/{timing.capacity} retained
  · {timing.overBudgetSamples}/{timing.totalSamples} over budget in this run/context
</p>
{#if timing.last}<p class="mt-2 text-xs">
    Attempt {timing.last.attempt}: {timing.last.outcome} · Runtime {timing.last
      .runtimeAdvanced
      ? "advanced"
      : "held"} · simulation {timing.last.simulationAdvanced
      ? "advanced"
      : "held"} · lock wait {ms(timing.last.lockWaitUs)}
  </p>{/if}
<p class="mt-2 text-xs text-muted-foreground">
  Wall-clock host measurements; they exclude rendering and do not represent
  Unity FPS. Statistics use the retained window.
</p>
{#if clipped}<p class="mt-2 text-xs text-amber-800">
    A duration reached the measurement cap. Clamped values and statistics are
    lower bounds.
  </p>{/if}
