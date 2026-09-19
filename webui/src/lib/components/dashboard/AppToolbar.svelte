<script lang="ts">
  import { createEventDispatcher } from 'svelte';

  export let saveDisabled = false;
  export let reloadDisabled = false;
  export let saving = false;
  export let loading = false;
  export let statusMessage = '';
  export let statusTone: 'neutral' | 'ok' | 'error' = 'neutral';
  export let lastSynced = '';

  const dispatch = createEventDispatcher<{
    exportJson: void;
    importJson: Event;
    saveToServer: void;
    reloadFromServer: void;
  }>();

  $: statusClass =
    statusTone === 'ok'
      ? 'border-success/40 bg-success/10 text-success'
      : statusTone === 'error'
        ? 'border-destructive/40 bg-destructive/10 text-destructive'
        : 'border-border bg-card/60 text-foreground/80';
</script>

<header class="sticky top-3 z-20 rounded-2xl border border-border/80 bg-card/70 p-4 backdrop-blur lg:p-5">
  <div class="flex flex-wrap items-center justify-between gap-3">
    <div>
      <p class="text-[11px] font-semibold uppercase tracking-[0.22em] text-muted-foreground">PRX PROXY</p>
      <h1 class="text-2xl font-black tracking-tight text-foreground md:text-3xl">Web UI</h1>
    </div>
    <div class="flex flex-wrap gap-2">
      <button
        class="rounded-lg border border-primary/40 bg-primary/10 px-4 py-2 text-sm font-semibold text-primary hover:bg-primary/20 disabled:cursor-not-allowed disabled:opacity-60"
        disabled={reloadDisabled}
        on:click={() => dispatch('reloadFromServer')}
      >
        {loading ? 'Reloading...' : 'Reload Server'}
      </button>
      <button
        class="rounded-lg border border-success/40 bg-success/10 px-4 py-2 text-sm font-semibold text-success hover:bg-success/20 disabled:cursor-not-allowed disabled:opacity-60"
        disabled={saveDisabled}
        on:click={() => dispatch('saveToServer')}
      >
        {saving ? 'Saving...' : 'Save to Server'}
      </button>
      <button class="rounded-lg border border-border bg-card px-4 py-2 text-sm font-semibold text-foreground hover:bg-muted" on:click={() => dispatch('exportJson')}>Export JSON</button>
      <label class="cursor-pointer rounded-lg border border-border bg-card px-4 py-2 text-sm font-semibold text-foreground hover:bg-muted">
        Import JSON
        <input class="hidden" type="file" accept="application/json" on:change={(e) => dispatch('importJson', e)} />
      </label>
    </div>
  </div>

  {#if statusMessage || lastSynced}
    <div class={`mt-3 rounded-lg border px-3 py-2 text-sm ${statusClass}`}>
      <div class="flex flex-wrap items-center justify-between gap-2">
        <p>{statusMessage || 'Ready'}</p>
        {#if lastSynced}
          <p class="text-xs font-medium uppercase tracking-wide">Last Sync: {lastSynced}</p>
        {/if}
      </div>
    </div>
  {/if}
</header>
