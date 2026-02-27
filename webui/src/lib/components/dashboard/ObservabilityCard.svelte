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

<article class="rounded-2xl border border-white/60 bg-white/85 p-4 shadow-panel backdrop-blur">
  <div class="mb-3 flex items-center justify-between gap-2">
    <h2 class="text-base font-bold text-slate-800">Observability</h2>
    {#if isEditing}
      <div class="flex gap-2">
        <button class="rounded-md border border-slate-300 bg-white px-3 py-1 text-xs font-semibold text-slate-700 hover:bg-slate-100" on:click={cancelEdit}>
          Cancel
        </button>
        <button class="rounded-md border border-emerald-300 bg-emerald-50 px-3 py-1 text-xs font-semibold text-emerald-700 hover:bg-emerald-100" on:click={saveEdit}>
          Save
        </button>
      </div>
    {:else}
      <button class="rounded-md border border-aqua/40 bg-aqua/10 px-3 py-1 text-xs font-semibold text-sky-800 hover:bg-aqua/20" on:click={startEdit}>
        Edit
      </button>
    {/if}
  </div>
  <div class="overflow-hidden rounded-xl border border-slate-200 bg-white">
    <table class="min-w-full divide-y divide-slate-200 text-sm">
      <thead class="bg-slate-100 text-slate-700">
        <tr>
          <th class="w-40 px-4 py-3 text-left font-semibold">Key</th>
          <th class="px-4 py-3 text-left font-semibold">Value</th>
        </tr>
      </thead>
      <tbody class="divide-y divide-slate-200">
        <tr>
          <td class="px-4 py-3 font-medium text-slate-800">log_level</td>
          <td class="px-4 py-3">
            {#if isEditing}
              <select
                class="w-full rounded-lg border border-slate-300 bg-white px-3 py-2"
                value={draftLogLevel}
                on:change={(e) => (draftLogLevel = selectValue(e))}
              >
                {#each logLevelOptions as level}
                  <option value={level}>{level}</option>
                {/each}
              </select>
            {:else}
              <span class="text-slate-700">{observability.log_level}</span>
            {/if}
          </td>
        </tr>
        <tr>
          <td class="px-4 py-3 font-medium text-slate-800">prometheus_listen</td>
          <td class="px-4 py-3">
            {#if isEditing}
              <input
                class="w-full rounded-lg border border-slate-300 px-3 py-2"
                value={draftPrometheusListen}
                on:input={(e) => (draftPrometheusListen = inputValue(e))}
              />
            {:else}
              <span class="text-slate-700">{observability.prometheus_listen || '-'}</span>
            {/if}
          </td>
        </tr>
        <tr>
          <td class="px-4 py-3 font-medium text-slate-800">access_log</td>
          <td class="px-4 py-3">
            {#if isEditing}
              <label class="flex items-center gap-2 text-sm text-slate-700">
                <input
                  type="checkbox"
                  checked={draftAccessLog}
                  on:change={(e) => (draftAccessLog = checkedValue(e))}
                />
                Enabled
              </label>
            {:else}
              <span class="text-slate-700">{observability.access_log ? 'true' : 'false'}</span>
            {/if}
          </td>
        </tr>
      </tbody>
    </table>
  </div>
</article>
