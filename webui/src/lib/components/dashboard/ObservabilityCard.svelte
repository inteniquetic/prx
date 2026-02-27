<script lang="ts">
  import { createEventDispatcher } from 'svelte';
  import type { PrxConfig } from '../../types/config';

  export let observability: PrxConfig['observability'];

  const dispatch = createEventDispatcher<{
    change: {
      key: keyof PrxConfig['observability'];
      value: string | boolean;
    };
  }>();

  const inputValue = (event: Event): string =>
    (event.currentTarget as HTMLInputElement).value;
  const checkedValue = (event: Event): boolean =>
    (event.currentTarget as HTMLInputElement).checked;
  const selectValue = (event: Event): string =>
    (event.currentTarget as HTMLSelectElement).value;
  const logLevelOptions = ['trace', 'debug', 'info', 'warn', 'error'];

  let isEditing = false;
  let draftLogLevel = '';
  let draftPrometheusListen = '';
  let draftAccessLog = false;

  $: if (!isEditing) {
    draftLogLevel = observability.log_level;
    draftPrometheusListen = observability.prometheus_listen;
    draftAccessLog = observability.access_log;
  }

  const startEdit = () => {
    isEditing = true;
  };

  const cancelEdit = () => {
    isEditing = false;
  };

  const saveEdit = () => {
    dispatch('change', { key: 'log_level', value: draftLogLevel });
    dispatch('change', {
      key: 'prometheus_listen',
      value: draftPrometheusListen
    });
    dispatch('change', { key: 'access_log', value: draftAccessLog });
    isEditing = false;
  };
</script>

<article class="rounded-2xl border border-slate-700/80 bg-slate-900/80 p-4 backdrop-blur">
  <div class="mb-3 flex items-center justify-between gap-2">
    <h2 class="text-base font-bold text-slate-100">Observability</h2>
    {#if isEditing}
      <div class="flex gap-2">
        <button class="rounded-md border border-slate-600 bg-slate-900 px-3 py-1 text-xs font-semibold text-slate-200 hover:bg-slate-800" on:click={cancelEdit}>
          Cancel
        </button>
        <button class="rounded-md border border-emerald-400/40 bg-emerald-500/10 px-3 py-1 text-xs font-semibold text-emerald-200 hover:bg-emerald-500/20" on:click={saveEdit}>
          Save
        </button>
      </div>
    {:else}
      <button class="rounded-md border border-cyan-400/40 bg-cyan-500/10 px-3 py-1 text-xs font-semibold text-cyan-200 hover:bg-cyan-500/20" on:click={startEdit}>
        Edit
      </button>
    {/if}
  </div>
  <div class="overflow-hidden rounded-xl border border-slate-700 bg-slate-950/70">
    <table class="min-w-full divide-y divide-slate-800 text-sm">
      <thead class="bg-slate-900 text-slate-300">
        <tr>
          <th class="w-40 px-4 py-3 text-left font-semibold">Key</th>
          <th class="px-4 py-3 text-left font-semibold">Value</th>
        </tr>
      </thead>
      <tbody class="divide-y divide-slate-800">
        <tr>
          <td class="px-4 py-3 font-medium text-slate-200">log_level</td>
          <td class="px-4 py-3">
            {#if isEditing}
              <select
                class="w-full rounded-lg border border-slate-600 bg-slate-900 px-3 py-2 text-slate-100"
                value={draftLogLevel}
                on:change={(e) => (draftLogLevel = selectValue(e))}
              >
                {#each logLevelOptions as level}
                  <option value={level}>{level}</option>
                {/each}
              </select>
            {:else}
              <span class="text-slate-300">{observability.log_level}</span>
            {/if}
          </td>
        </tr>
        <tr>
          <td class="px-4 py-3 font-medium text-slate-200">prometheus_listen</td>
          <td class="px-4 py-3">
            {#if isEditing}
              <input
                class="w-full rounded-lg border border-slate-600 bg-slate-900 px-3 py-2 text-slate-100 placeholder:text-slate-500"
                value={draftPrometheusListen}
                on:input={(e) => (draftPrometheusListen = inputValue(e))}
              />
            {:else}
              <span class="text-slate-300">{observability.prometheus_listen || '-'}</span>
            {/if}
          </td>
        </tr>
        <tr>
          <td class="px-4 py-3 font-medium text-slate-200">access_log</td>
          <td class="px-4 py-3">
            {#if isEditing}
              <label class="flex items-center gap-2 text-sm text-slate-300">
                <input
                  type="checkbox"
                  checked={draftAccessLog}
                  on:change={(e) => (draftAccessLog = checkedValue(e))}
                />
                Enabled
              </label>
            {:else}
              <span class="text-slate-300">{observability.access_log ? 'true' : 'false'}</span>
            {/if}
          </td>
        </tr>
      </tbody>
    </table>
  </div>
</article>
