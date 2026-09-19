<script lang="ts" module>
  export interface ChartSeries {
    key: string;
    label: string;
    /** Oldest first, one entry per sample; `null` is a gap, not a zero. */
    values: (number | null)[];
    /** A CSS colour expression — a design token, never a literal. */
    color: string;
    /** Secondary encoding, so the lines still separate without colour. */
    dash?: string;
  }
</script>

<script lang="ts">
  import { cn } from '$lib/utils';

  let {
    title,
    description,
    series,
    /** Epoch milliseconds, aligned with every series' values. */
    timestamps,
    format,
    /**
     * Axis ticks, where the unit is already established by the title and the
     * legend. Repeating "req/s" five times down the side wraps every label onto
     * two lines and says nothing the reader did not already know.
     */
    formatTick,
    /** Stacked areas share one axis and add up to the total; lines do not. */
    mode = 'line',
    height = 180,
    /** Keeps an idle chart from magnifying noise into a mountain range. */
    floor = 0,
    class: className
  }: {
    title: string;
    description?: string;
    series: ChartSeries[];
    timestamps: number[];
    format: (value: number | null) => string;
    formatTick?: (value: number) => string;
    mode?: 'line' | 'stacked';
    height?: number;
    floor?: number;
    class?: string;
  } = $props();

  // The plot is drawn in its own coordinate space and stretched to fit, which
  // is why every stroke carries vector-effect. Text lives in HTML outside the
  // SVG so it never gets stretched with it.
  const VIEW_W = 1000;
  const VIEW_H = 100;

  let hovered = $state<number | null>(null);
  let plot = $state<HTMLDivElement | null>(null);

  const count = $derived(Math.max(timestamps.length, 0));
  const hasData = $derived(
    count > 1 && series.some((entry) => entry.values.some((value) => value !== null))
  );

  /** Stacked series are drawn from cumulative tops, in the order given. */
  const stacks = $derived.by(() => {
    if (mode !== 'stacked') return [];
    const tops: number[][] = [];
    const running = new Array(count).fill(0);
    for (const entry of series) {
      const top = new Array(count).fill(0);
      for (let index = 0; index < count; index += 1) {
        running[index] += entry.values[index] ?? 0;
        top[index] = running[index];
      }
      tops.push(top);
    }
    return tops;
  });

  const max = $derived.by(() => {
    let highest = floor;
    if (mode === 'stacked') {
      for (const top of stacks) {
        for (const value of top) if (value > highest) highest = value;
      }
    } else {
      for (const entry of series) {
        for (const value of entry.values) {
          if (value !== null && value > highest) highest = value;
        }
      }
    }
    return niceCeiling(highest);
  });

  /** A round number above the data, so the axis reads 0 / 25 / 50 / 75 / 100. */
  function niceCeiling(value: number): number {
    if (!Number.isFinite(value) || value <= 0) return 1;
    const magnitude = 10 ** Math.floor(Math.log10(value));
    const normalized = value / magnitude;
    // The 2.5 step is what keeps a chart peaking at 2.6k from being drawn
    // against a ceiling of 5k, with the data crammed into the bottom half.
    const step =
      normalized <= 1 ? 1 : normalized <= 2 ? 2 : normalized <= 2.5 ? 2.5 : normalized <= 5 ? 5 : 10;
    return step * magnitude;
  }

  const tickLabel = $derived(formatTick ?? ((value: number) => format(value)));

  const ticks = $derived([1, 0.75, 0.5, 0.25, 0].map((fraction) => fraction * max));

  const xAt = (index: number): number =>
    count <= 1 ? 0 : (index / (count - 1)) * VIEW_W;
  const yAt = (value: number): number =>
    VIEW_H - (Math.min(Math.max(value, 0), max) / max) * VIEW_H;

  /** Line path, split at gaps so a missing sample is a break rather than a lie. */
  function linePath(values: (number | null)[]): string {
    let path = '';
    let pen = false;
    for (let index = 0; index < count; index += 1) {
      const value = values[index];
      if (value === null || value === undefined) {
        pen = false;
        continue;
      }
      path += `${pen ? 'L' : 'M'}${xAt(index).toFixed(2)},${yAt(value).toFixed(2)}`;
      pen = true;
    }
    return path;
  }

  /** Area between this stack's top and the one below it. */
  function areaPath(seriesIndex: number): string {
    const top = stacks[seriesIndex];
    if (!top) return '';
    const bottom = seriesIndex === 0 ? new Array(count).fill(0) : stacks[seriesIndex - 1];
    let path = '';
    for (let index = 0; index < count; index += 1) {
      path += `${index === 0 ? 'M' : 'L'}${xAt(index).toFixed(2)},${yAt(top[index]).toFixed(2)}`;
    }
    for (let index = count - 1; index >= 0; index -= 1) {
      path += `L${xAt(index).toFixed(2)},${yAt(bottom[index]).toFixed(2)}`;
    }
    return `${path}Z`;
  }

  const valueAt = (entry: ChartSeries, index: number | null): number | null =>
    index === null ? null : (entry.values[index] ?? null);

  const latest = (entry: ChartSeries): number | null => {
    for (let index = entry.values.length - 1; index >= 0; index -= 1) {
      const value = entry.values[index];
      if (value !== null && value !== undefined) return value;
    }
    return null;
  };

  /** Age of a sample, which reads better than a clock time on a live chart. */
  function agoLabel(index: number): string {
    const at = timestamps[index];
    if (!at) return '';
    const seconds = Math.max(0, Math.round((Date.now() - at) / 1000));
    if (seconds < 5) return 'now';
    if (seconds < 60) return `${seconds}s ago`;
    return `${Math.round(seconds / 60)}m ago`;
  }

  const onPointerMove = (event: PointerEvent) => {
    if (!plot || count === 0) return;
    const rect = plot.getBoundingClientRect();
    const ratio = (event.clientX - rect.left) / Math.max(rect.width, 1);
    hovered = Math.min(count - 1, Math.max(0, Math.round(ratio * (count - 1))));
  };

  const onKeyDown = (event: KeyboardEvent) => {
    if (count === 0) return;
    if (event.key === 'ArrowLeft' || event.key === 'ArrowRight') {
      event.preventDefault();
      const step = event.key === 'ArrowLeft' ? -1 : 1;
      hovered = Math.min(count - 1, Math.max(0, (hovered ?? count - 1) + step));
    } else if (event.key === 'Escape') {
      hovered = null;
    }
  };

  // The tooltip follows the crosshair but never hangs off the edge.
  const tooltipLeft = $derived(
    hovered === null ? 0 : Math.min(88, Math.max(2, (hovered / Math.max(count - 1, 1)) * 100))
  );

  const summary = $derived(
    series
      .map((entry) => `${entry.label}: ${format(latest(entry))}`)
      .join(', ')
  );
</script>

<figure
  class={cn(
    'flex flex-col gap-3 rounded-xl border border-border bg-card p-4 text-card-foreground shadow-sm',
    className
  )}
>
  <figcaption class="flex flex-wrap items-baseline justify-between gap-2">
    <div>
      <h3 class="text-sm font-semibold">{title}</h3>
      {#if description}
        <p class="text-xs text-muted-foreground">{description}</p>
      {/if}
    </div>

    <!-- Legend and direct labels in one: identity is never colour alone, and
         the current value is next to the name rather than only on the line. -->
    <ul class="flex flex-wrap items-center gap-x-4 gap-y-1">
      {#each series as entry (entry.key)}
        <li class="flex items-center gap-1.5 text-xs">
          <svg
            class="h-2 w-5 shrink-0"
            style="color: {entry.color}"
            viewBox="0 0 20 8"
            aria-hidden="true"
          >
            <line
              x1="0"
              y1="4"
              x2="20"
              y2="4"
              stroke="currentColor"
              stroke-width="2.5"
              stroke-dasharray={entry.dash ?? undefined}
              stroke-linecap="round"
            />
          </svg>
          <span class="text-muted-foreground">{entry.label}</span>
          <span class="font-medium tabular-nums">{format(latest(entry))}</span>
        </li>
      {/each}
    </ul>
  </figcaption>

  <div class="flex gap-2">
    <!-- Axis labels are HTML: the plot is stretched to fit its box, and
         stretched text is unreadable. -->
    <div
      class="flex w-14 shrink-0 flex-col justify-between py-0.5 text-right text-[10px] tabular-nums text-muted-foreground"
      style="height: {height}px"
      aria-hidden="true"
    >
      {#each ticks as tick (tick)}
        <span>{tickLabel(tick)}</span>
      {/each}
    </div>

    <div class="min-w-0 flex-1">
      <div bind:this={plot} class="relative" style="height: {height}px">
        <svg
          class="h-full w-full overflow-visible"
          viewBox="0 0 {VIEW_W} {VIEW_H}"
          preserveAspectRatio="none"
          aria-hidden="true"
          focusable="false"
        >
          <!-- Grid stays recessive: it is a reference, not a subject. -->
          {#each ticks as tick (tick)}
            <line
              x1="0"
              y1={yAt(tick)}
              x2={VIEW_W}
              y2={yAt(tick)}
              class="stroke-border"
              stroke-width="1"
              vector-effect="non-scaling-stroke"
            />
          {/each}

          {#if hasData}
            {#if mode === 'stacked'}
              {#each series as entry, index (entry.key)}
                <g style="color: {entry.color}">
                  <path d={areaPath(index)} fill="currentColor" fill-opacity="0.18" />
                  <path
                    d={linePath(stacks[index] ?? [])}
                    fill="none"
                    stroke="currentColor"
                    stroke-width="2"
                    stroke-dasharray={entry.dash ?? undefined}
                    stroke-linecap="round"
                    stroke-linejoin="round"
                    vector-effect="non-scaling-stroke"
                  />
                </g>
              {/each}
            {:else}
              {#each series as entry (entry.key)}
                <g style="color: {entry.color}">
                  <path
                    d={linePath(entry.values)}
                    fill="none"
                    stroke="currentColor"
                    stroke-width="2"
                    stroke-dasharray={entry.dash ?? undefined}
                    stroke-linecap="round"
                    stroke-linejoin="round"
                    vector-effect="non-scaling-stroke"
                  />
                </g>
              {/each}
            {/if}

            {#if hovered !== null}
              <line
                x1={xAt(hovered)}
                y1="0"
                x2={xAt(hovered)}
                y2={VIEW_H}
                class="stroke-foreground/40"
                stroke-width="1"
                vector-effect="non-scaling-stroke"
              />
              {#each series as entry, index (entry.key)}
                {@const value =
                  mode === 'stacked'
                    ? (stacks[index]?.[hovered] ?? null)
                    : valueAt(entry, hovered)}
                {#if value !== null}
                  <!-- A ring in the surface colour keeps overlapping points
                       from merging into one blob. -->
                  <circle
                    cx={xAt(hovered)}
                    cy={yAt(value)}
                    r="4"
                    style="color: {entry.color}"
                    fill="currentColor"
                    class="stroke-card"
                    stroke-width="2"
                    vector-effect="non-scaling-stroke"
                  />
                {/if}
              {/each}
            {/if}
          {/if}
        </svg>

        <!-- The reader explores the chart through this layer: pointer for a
             crosshair, arrow keys for the same thing without a mouse. It is a
             real control rather than a div with handlers, so it is reachable by
             keyboard and announced. -->
        <button
          type="button"
          class="absolute inset-0 cursor-default rounded-md focus-visible:ring-[3px] focus-visible:ring-ring/50 focus-visible:outline-none"
          aria-label="{title}. {summary}. Use the arrow keys to read earlier samples."
          onpointermove={onPointerMove}
          onpointerleave={() => (hovered = null)}
          onkeydown={onKeyDown}
          onblur={() => (hovered = null)}
        ></button>

        {#if !hasData}
          <div
            class="absolute inset-0 flex items-center justify-center text-xs text-muted-foreground"
          >
            Waiting for the first samples…
          </div>
        {/if}

        {#if hovered !== null && hasData}
          <div
            class="pointer-events-none absolute top-2 z-10 min-w-40 rounded-md border border-border bg-popover p-2 text-xs shadow-md"
            style="left: {tooltipLeft}%"
          >
            <p class="mb-1 font-medium text-muted-foreground">{agoLabel(hovered)}</p>
            <ul class="space-y-0.5">
              {#each series as entry (entry.key)}
                <li class="flex items-center justify-between gap-3">
                  <span class="flex items-center gap-1.5">
                    <span
                      class="inline-block h-0.5 w-3 rounded-full"
                      style="background: {entry.color}"
                      aria-hidden="true"
                    ></span>
                    <span class="text-muted-foreground">{entry.label}</span>
                  </span>
                  <span class="font-medium tabular-nums">
                    {format(valueAt(entry, hovered))}
                  </span>
                </li>
              {/each}
            </ul>
          </div>
        {/if}
      </div>

      <div
        class="mt-1 flex justify-between text-[10px] text-muted-foreground"
        aria-hidden="true"
      >
        <span>{count > 1 ? agoLabel(0) : ''}</span>
        <span>{count > 1 ? agoLabel(count - 1) : ''}</span>
      </div>
    </div>
  </div>
</figure>
