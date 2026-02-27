<script lang="ts">
  import { createEventDispatcher } from 'svelte';

  export let toml = '';
  export let copied = false;
  export let issues: string[] = [];

  const dispatch = createEventDispatcher<{ copy: void }>();
  $: isValid = issues.length === 0;
</script>

<article class="rounded-2xl border border-slate-700/80 bg-[#0f172a] p-4 text-slate-100">
  <div class="mb-3 flex flex-wrap items-center justify-between gap-3">
    <h2 class="text-base font-bold">TOML Preview</h2>
    <button class="rounded-lg border border-cyan-300/40 bg-cyan-300/10 px-3 py-1.5 text-sm font-semibold text-cyan-100 hover:bg-cyan-300/20" on:click={() => dispatch('copy')}>
      {copied ? 'Copied' : 'Copy'}
    </button>
  </div>

  <div class={isValid ? 'mb-3 w-full rounded-lg border border-emerald-300/40 bg-emerald-400/15 px-3 py-2 text-sm font-semibold text-emerald-200' : 'mb-3 w-full rounded-lg border border-rose-300/40 bg-rose-400/15 px-3 py-2 text-sm font-semibold text-rose-200'}>
    {isValid ? 'VALIDATION: PASS' : `VALIDATION: FAIL (${issues.length} issue(s))`}
  </div>

  {#if !isValid}
    <div class="mb-3 rounded-lg border border-rose-300/30 bg-rose-400/10 p-3 text-xs text-rose-100">
      {issues[0]}
    </div>
  {/if}

  <pre class="max-h-[36vh] overflow-auto rounded-xl bg-black/30 p-4 text-xs leading-6 md:text-sm">{toml}</pre>
</article>
