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
      ? 'border-emerald-200 bg-emerald-50 text-emerald-800'
      : statusTone === 'error'
        ? 'border-rose-200 bg-rose-50 text-rose-800'
        : 'border-slate-200 bg-slate-50 text-slate-700';
</script>

<header class="sticky top-3 z-20 rounded-2xl border border-white/70 bg-white/75 p-4 shadow-panel backdrop-blur lg:p-5">
  <div class="flex flex-wrap items-center justify-between gap-3">
    <div>
      <p class="text-[11px] font-semibold uppercase tracking-[0.22em] text-slate-500">INTENIQUETIC PRX</p>
      <h1 class="text-2xl font-black tracking-tight text-ink md:text-3xl">Config Manager</h1>
    </div>
    <div class="flex flex-wrap gap-2">
      <button
        class="rounded-lg border border-cyan-300 bg-cyan-50 px-4 py-2 text-sm font-semibold text-cyan-800 hover:bg-cyan-100 disabled:cursor-not-allowed disabled:opacity-60"
        disabled={reloadDisabled}
        on:click={() => dispatch('reloadFromServer')}
      >
        {loading ? 'Reloading...' : 'Reload Server'}
      </button>
      <button
        class="rounded-lg border border-emerald-300 bg-emerald-50 px-4 py-2 text-sm font-semibold text-emerald-800 hover:bg-emerald-100 disabled:cursor-not-allowed disabled:opacity-60"
        disabled={saveDisabled}
        on:click={() => dispatch('saveToServer')}
      >
        {saving ? 'Saving...' : 'Save to Server'}
      </button>
      <button class="rounded-lg border border-slate-300 bg-white px-4 py-2 text-sm font-semibold text-slate-700 hover:bg-slate-100" on:click={() => dispatch('exportJson')}>Export JSON</button>
      <label class="cursor-pointer rounded-lg border border-slate-300 bg-white px-4 py-2 text-sm font-semibold text-slate-700 hover:bg-slate-100">
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
