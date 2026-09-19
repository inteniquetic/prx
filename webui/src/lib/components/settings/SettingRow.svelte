<script lang="ts">
  /**
   * One setting: what it is called, what it does, and what changing it costs
   * (T308).
   *
   * The "restart" mark is the important part. Most of the config takes effect
   * on the next reload, but a listen address, a thread count or the log level
   * is read once when prx starts — applying those writes the file and changes
   * nothing until the process is restarted, and a form that does not say so is
   * lying by omission.
   */
  import type { Snippet } from 'svelte';
  import PowerIcon from '@lucide/svelte/icons/power';

  import { Label } from '$lib/components/ui/label';

  let {
    label,
    /** One line under the label: what it does, not what it is called again. */
    description,
    /** The TOML key, shown so the file and the form are obviously the same thing. */
    path,
    /** True when prx has to be restarted for a change to take effect. */
    restart = false,
    /** `for` of the control inside, so the label is clickable. */
    id,
    children
  }: {
    label: string;
    description?: string;
    path?: string;
    restart?: boolean;
    id?: string;
    children: Snippet;
  } = $props();
</script>

<div
  class="grid gap-2 border-b border-border/60 py-4 last:border-0 sm:grid-cols-[minmax(0,18rem)_minmax(0,1fr)] sm:gap-6"
  data-slot="setting-row"
  data-restart={restart ? 'true' : undefined}
>
  <div class="min-w-0">
    <Label for={id} class="text-sm font-medium text-foreground">{label}</Label>
    {#if restart}
      <span
        class="ms-2 inline-flex items-center gap-1 rounded-full bg-warning/15 px-1.5 py-0.5 align-middle text-[10px] font-semibold text-warning-emphasis"
        title="prx reads this once at startup"
      >
        <PowerIcon class="size-2.5" aria-hidden="true" />
        needs a restart
      </span>
    {/if}
    {#if description}
      <p class="mt-1 text-xs text-muted-foreground">{description}</p>
    {/if}
    {#if path}
      <p class="mt-1 font-mono text-[11px] text-muted-foreground/80">{path}</p>
    {/if}
  </div>

  <div class="min-w-0">
    {@render children()}
  </div>
</div>
