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
      ? 'border-emerald-400/40 bg-emerald-500/10 text-emerald-200'
      : statusTone === 'error'
        ? 'border-rose-400/40 bg-rose-500/10 text-rose-200'
        : 'border-slate-700 bg-slate-900/60 text-slate-300';
</script>

<header class="sticky top-3 z-20 rounded-2xl border border-slate-700/80 bg-slate-900/70 p-4 backdrop-blur lg:p-5">
  <div class="flex flex-wrap items-center justify-between gap-3">
    <div>
      <p class="text-[11px] font-semibold uppercase tracking-[0.22em] text-slate-400">PRX PROXY</p>
      <h1 class="text-2xl font-black tracking-tight text-slate-100 md:text-3xl">Web UI</h1>
    </div>
    <div class="flex flex-wrap gap-2">
      <button
        class="rounded-lg border border-cyan-400/40 bg-cyan-500/10 px-4 py-2 text-sm font-semibold text-cyan-200 hover:bg-cyan-500/20 disabled:cursor-not-allowed disabled:opacity-60"
        disabled={reloadDisabled}
        on:click={() => dispatch('reloadFromServer')}
      >
        {loading ? 'Reloading...' : 'Reload Server'}
      </button>
      <button
        class="rounded-lg border border-emerald-400/40 bg-emerald-500/10 px-4 py-2 text-sm font-semibold text-emerald-200 hover:bg-emerald-500/20 disabled:cursor-not-allowed disabled:opacity-60"
        disabled={saveDisabled}
        on:click={() => dispatch('saveToServer')}
      >
        {saving ? 'Saving...' : 'Save to Server'}
      </button>
      <button class="rounded-lg border border-slate-600 bg-slate-900 px-4 py-2 text-sm font-semibold text-slate-200 hover:bg-slate-800" on:click={() => dispatch('exportJson')}>Export JSON</button>
      <label class="cursor-pointer rounded-lg border border-slate-600 bg-slate-900 px-4 py-2 text-sm font-semibold text-slate-200 hover:bg-slate-800">
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
