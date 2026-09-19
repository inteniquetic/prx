<script lang="ts" module>
  export type SystemState = 'ok' | 'degraded' | 'down' | 'unknown';

  export interface SystemProblem {
    /** One line saying what is wrong. */
    text: string;
    /** Where to go to fix it. */
    action?: { label: string; go: () => void };
  }
</script>

<script lang="ts">
  import CircleCheckIcon from '@lucide/svelte/icons/circle-check';
  import CircleXIcon from '@lucide/svelte/icons/circle-x';
  import PlugZapIcon from '@lucide/svelte/icons/plug-zap';
  import TriangleAlertIcon from '@lucide/svelte/icons/triangle-alert';

  import { Button } from '$lib/components/ui/button';
  import type { LiveStatus } from '$lib/stores/stats';
  import { cn } from '$lib/utils';

  let {
    state,
    problems = [],
    connection,
    /** Why the connection is in this state, when the browser told us. */
    connectionError = null
  }: {
    state: SystemState;
    problems?: SystemProblem[];
    connection: LiveStatus;
    connectionError?: string | null;
  } = $props();

  // A dashboard that cannot reach the proxy has nothing true to say about the
  // proxy, so that takes precedence over whatever the last sample claimed.
  const disconnected = $derived(connection === 'reconnecting' || connection === 'offline');

  const tone = $derived(
    disconnected ? 'connection' : state === 'ok' ? 'ok' : state === 'down' ? 'down' : 'degraded'
  );

  const Icon = $derived(
    tone === 'connection'
      ? PlugZapIcon
      : tone === 'ok'
        ? CircleCheckIcon
        : tone === 'down'
          ? CircleXIcon
          : TriangleAlertIcon
  );

  const headline = $derived.by(() => {
    if (connection === 'offline') return 'Lost contact with the admin API';
    if (connection === 'reconnecting') return 'Reconnecting to the admin API…';
    if (state === 'down') return 'Traffic is failing';
    if (state === 'degraded') return 'Running degraded';
    if (state === 'unknown') return 'Waiting for the first samples';
    return 'Everything is healthy';
  });

  const detail = $derived.by(() => {
    if (disconnected) {
      return connectionError
        ? `These numbers stopped updating: ${connectionError}`
        : 'These numbers are the last ones that arrived, not what is happening now.';
    }
    if (connection === 'polling') {
      return 'The live stream is unavailable, so this page is asking every couple of seconds instead.';
    }
    return null;
  });
</script>

<!-- Colour is the confirmation; the icon and the words are the message. -->
<section
  class={cn(
    'flex flex-wrap items-center gap-x-4 gap-y-2 rounded-xl border px-4 py-3',
    tone === 'ok' && 'border-success/40 bg-success/10 text-success-emphasis',
    tone === 'degraded' && 'border-warning/50 bg-warning/10 text-warning-emphasis',
    tone === 'down' && 'border-destructive/50 bg-destructive/10 text-destructive-emphasis',
    tone === 'connection' && 'border-border bg-muted text-foreground'
  )}
  role={tone === 'ok' ? undefined : 'status'}
  aria-live="polite"
  data-system-state={disconnected ? connection : state}
>
  <Icon class="size-5 shrink-0" aria-hidden="true" />

  <div class="min-w-0 flex-1">
    <p class="text-sm font-semibold">{headline}</p>
    {#if detail}
      <p class="text-xs text-muted-foreground">{detail}</p>
    {/if}
    {#if !disconnected && problems.length > 0}
      <ul class="mt-1 space-y-0.5 text-xs text-foreground/80">
        {#each problems.slice(0, 4) as problem (problem.text)}
          <li class="flex flex-wrap items-center gap-2">
            <span>{problem.text}</span>
            {#if problem.action}
              <!-- Naming the problem without saying where to go leaves the
                   reader hunting through three pages for it. -->
              <Button
                variant="link"
                size="sm"
                class="h-auto p-0 text-xs"
                onclick={problem.action.go}
              >
                {problem.action.label} →
              </Button>
            {/if}
          </li>
        {/each}
      </ul>
    {/if}
  </div>
</section>
