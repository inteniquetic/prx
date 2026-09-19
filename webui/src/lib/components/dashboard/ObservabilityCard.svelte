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

<article class="rounded-2xl border border-border/80 bg-card/80 p-4 backdrop-blur">
  <div class="mb-3 flex items-center justify-between gap-2">
    <h2 class="text-base font-bold text-foreground">Observability</h2>
    {#if isEditing}
      <div class="flex gap-2">
        <button class="rounded-md border border-border bg-card px-3 py-1 text-xs font-semibold text-foreground hover:bg-muted" on:click={cancelEdit}>
          Cancel
        </button>
        <button class="rounded-md border border-success/40 bg-success/10 px-3 py-1 text-xs font-semibold text-success hover:bg-success/20" on:click={saveEdit}>
          Save
        </button>
      </div>
    {:else}
      <button class="rounded-md border border-primary/40 bg-primary/10 px-3 py-1 text-xs font-semibold text-primary hover:bg-primary/20" on:click={startEdit}>
        Edit
      </button>
    {/if}
  </div>
  <div class="overflow-hidden rounded-xl border border-border bg-background/70">
    <table class="min-w-full divide-y divide-border text-sm">
      <thead class="bg-card text-foreground/80">
        <tr>
          <th class="w-40 px-4 py-3 text-left font-semibold">Key</th>
          <th class="px-4 py-3 text-left font-semibold">Value</th>
        </tr>
      </thead>
      <tbody class="divide-y divide-border">
        <tr>
          <td class="px-4 py-3 font-medium text-foreground">log_level</td>
          <td class="px-4 py-3">
            {#if isEditing}
              <select
                class="w-full rounded-lg border border-border bg-card px-3 py-2 text-foreground"
                value={draftLogLevel}
                on:change={(e) => (draftLogLevel = selectValue(e))}
              >
                {#each logLevelOptions as level}
                  <option value={level}>{level}</option>
                {/each}
              </select>
            {:else}
              <span class="text-foreground/80">{observability.log_level}</span>
            {/if}
          </td>
        </tr>
        <tr>
          <td class="px-4 py-3 font-medium text-foreground">prometheus_listen</td>
          <td class="px-4 py-3">
            {#if isEditing}
              <input
                class="w-full rounded-lg border border-border bg-card px-3 py-2 text-foreground placeholder:text-muted-foreground"
                value={draftPrometheusListen}
                on:input={(e) => (draftPrometheusListen = inputValue(e))}
              />
            {:else}
              <span class="text-foreground/80">{observability.prometheus_listen || '-'}</span>
            {/if}
          </td>
        </tr>
        <tr>
          <td class="px-4 py-3 font-medium text-foreground">access_log</td>
          <td class="px-4 py-3">
            {#if isEditing}
              <label class="flex items-center gap-2 text-sm text-foreground/80">
                <input
                  type="checkbox"
                  checked={draftAccessLog}
                  on:change={(e) => (draftAccessLog = checkedValue(e))}
                />
                Enabled
              </label>
            {:else}
              <span class="text-foreground/80">{observability.access_log ? 'true' : 'false'}</span>
            {/if}
          </td>
        </tr>
      </tbody>
    </table>
  </div>
</article>
