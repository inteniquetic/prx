<script lang="ts">
  import { createEventDispatcher } from 'svelte';
  import type { PrxConfig } from '../../types/config';

  export let server: PrxConfig['server'];

  const dispatch = createEventDispatcher<{
    save: PrxConfig['server'];
  }>();

  const inputValue = (event: Event): string =>
    (event.currentTarget as HTMLInputElement).value;
  const checkedValue = (event: Event): boolean =>
    (event.currentTarget as HTMLInputElement).checked;

  let isEditing = false;
  let draft: PrxConfig['server'] = {
    listen: ['0.0.0.0:8080'],
    health_path: '/healthz',
    ready_path: '/readyz',
    threads: null,
    grace_period_seconds: null,
    graceful_shutdown_timeout_seconds: null,
    config_reload_debounce_ms: 250,
    tls: null
  };

  const cloneServer = (src: PrxConfig['server']): PrxConfig['server'] => ({
    ...src,
    listen: [...src.listen],
    tls: src.tls ? { ...src.tls } : null
  });

  $: if (!isEditing) {
    draft = cloneServer(server);
  }

  const startEdit = () => {
    draft = cloneServer(server);
    isEditing = true;
  };

  const cancelEdit = () => {
    isEditing = false;
  };

  const parseNullableNumber = (value: string): number | null => {
    const trimmed = value.trim();
    if (!trimmed) {
      return null;
    }
    const num = Number(trimmed);
    return Number.isFinite(num) ? Math.max(0, Math.floor(num)) : null;
  };

  const saveEdit = () => {
    draft.listen = draft.listen.map((x) => x.trim()).filter(Boolean);
    if (draft.listen.length === 0) {
      draft.listen = ['0.0.0.0:8080'];
    }

    if (draft.tls) {
      draft.tls.listen = draft.tls.listen.trim();
      draft.tls.cert_path = draft.tls.cert_path.trim();
      draft.tls.key_path = draft.tls.key_path.trim();
    }

    dispatch('save', cloneServer(draft));
    isEditing = false;
  };
</script>

<article class="rounded-2xl border border-white/60 bg-white/85 p-4 shadow-panel backdrop-blur">
  <div class="mb-3 flex items-center justify-between gap-2">
    <h2 class="text-base font-bold text-slate-800">Server</h2>
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
          <th class="w-52 px-4 py-3 text-left font-semibold">Key</th>
          <th class="px-4 py-3 text-left font-semibold">Value</th>
        </tr>
      </thead>
      <tbody class="divide-y divide-slate-200">
        <tr>
          <td class="px-4 py-3 font-medium text-slate-800">listen</td>
          <td class="px-4 py-3">
            {#if isEditing}
              <input class="w-full rounded-lg border border-slate-300 px-3 py-2" value={draft.listen.join(', ')} on:input={(e) => (draft.listen = inputValue(e).split(',').map((x) => x.trim()).filter(Boolean))} />
            {:else}
              <span class="text-slate-700">{server.listen.join(', ')}</span>
            {/if}
          </td>
        </tr>
        <tr>
          <td class="px-4 py-3 font-medium text-slate-800">health_path</td>
          <td class="px-4 py-3">
            {#if isEditing}
              <input class="w-full rounded-lg border border-slate-300 px-3 py-2" value={draft.health_path} on:input={(e) => (draft.health_path = inputValue(e))} />
            {:else}
              <span class="text-slate-700">{server.health_path}</span>
            {/if}
          </td>
        </tr>
        <tr>
          <td class="px-4 py-3 font-medium text-slate-800">ready_path</td>
          <td class="px-4 py-3">
            {#if isEditing}
              <input class="w-full rounded-lg border border-slate-300 px-3 py-2" value={draft.ready_path} on:input={(e) => (draft.ready_path = inputValue(e))} />
            {:else}
              <span class="text-slate-700">{server.ready_path}</span>
            {/if}
          </td>
        </tr>
        <tr>
          <td class="px-4 py-3 font-medium text-slate-800">threads</td>
          <td class="px-4 py-3">
            {#if isEditing}
              <input class="w-full rounded-lg border border-slate-300 px-3 py-2" placeholder="null" value={draft.threads ?? ''} on:input={(e) => (draft.threads = parseNullableNumber(inputValue(e)))} />
            {:else}
              <span class="text-slate-700">{server.threads ?? '-'}</span>
            {/if}
          </td>
        </tr>
        <tr>
          <td class="px-4 py-3 font-medium text-slate-800">grace_period_seconds</td>
          <td class="px-4 py-3">
            {#if isEditing}
              <input class="w-full rounded-lg border border-slate-300 px-3 py-2" placeholder="null" value={draft.grace_period_seconds ?? ''} on:input={(e) => (draft.grace_period_seconds = parseNullableNumber(inputValue(e)))} />
            {:else}
              <span class="text-slate-700">{server.grace_period_seconds ?? '-'}</span>
            {/if}
          </td>
        </tr>
        <tr>
          <td class="px-4 py-3 font-medium text-slate-800">graceful_shutdown_timeout_seconds</td>
          <td class="px-4 py-3">
            {#if isEditing}
              <input class="w-full rounded-lg border border-slate-300 px-3 py-2" placeholder="null" value={draft.graceful_shutdown_timeout_seconds ?? ''} on:input={(e) => (draft.graceful_shutdown_timeout_seconds = parseNullableNumber(inputValue(e)))} />
            {:else}
              <span class="text-slate-700">{server.graceful_shutdown_timeout_seconds ?? '-'}</span>
            {/if}
          </td>
        </tr>
        <tr>
          <td class="px-4 py-3 font-medium text-slate-800">config_reload_debounce_ms</td>
          <td class="px-4 py-3">
            {#if isEditing}
              <input class="w-full rounded-lg border border-slate-300 px-3 py-2" value={draft.config_reload_debounce_ms} on:input={(e) => (draft.config_reload_debounce_ms = parseNullableNumber(inputValue(e)) ?? 0)} />
            {:else}
              <span class="text-slate-700">{server.config_reload_debounce_ms}</span>
            {/if}
          </td>
        </tr>
        <tr>
          <td class="px-4 py-3 font-medium text-slate-800 align-top">server.tls</td>
          <td class="px-4 py-3">
            {#if isEditing}
              <label class="mb-2 flex items-center gap-2 text-sm text-slate-700">
                <input type="checkbox" checked={draft.tls !== null} on:change={(e) => (draft.tls = checkedValue(e) ? { listen: '0.0.0.0:8443', cert_path: '', key_path: '', enable_h2: true } : null)} />
                Enable TLS
              </label>
              {#if draft.tls}
                <div class="grid gap-2 md:grid-cols-2">
                  <input class="rounded-lg border border-slate-300 px-3 py-2" placeholder="listen" value={draft.tls.listen} on:input={(e) => draft.tls && (draft.tls.listen = inputValue(e))} />
                  <label class="mt-2 flex items-center gap-2 text-sm text-slate-700">
                    <input type="checkbox" checked={draft.tls.enable_h2} on:change={(e) => draft.tls && (draft.tls.enable_h2 = checkedValue(e))} />
                    enable_h2
                  </label>
                  <input class="rounded-lg border border-slate-300 px-3 py-2 md:col-span-2" placeholder="cert_path" value={draft.tls.cert_path} on:input={(e) => draft.tls && (draft.tls.cert_path = inputValue(e))} />
                  <input class="rounded-lg border border-slate-300 px-3 py-2 md:col-span-2" placeholder="key_path" value={draft.tls.key_path} on:input={(e) => draft.tls && (draft.tls.key_path = inputValue(e))} />
                </div>
              {/if}
            {:else}
              {#if server.tls}
                <div class="space-y-1 text-slate-700">
                  <div>listen: {server.tls.listen}</div>
                  <div>cert_path: {server.tls.cert_path}</div>
                  <div>key_path: {server.tls.key_path}</div>
                  <div>enable_h2: {server.tls.enable_h2 ? 'true' : 'false'}</div>
                </div>
              {:else}
                <span class="text-slate-700">disabled</span>
              {/if}
            {/if}
          </td>
        </tr>
      </tbody>
    </table>
  </div>
</article>
