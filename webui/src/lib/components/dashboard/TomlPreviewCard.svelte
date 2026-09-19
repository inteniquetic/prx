<script lang="ts">
  import { createEventDispatcher } from 'svelte';

  export let toml = '';
  export let copied = false;
  export let issues: string[] = [];

  const dispatch = createEventDispatcher<{ copy: void }>();
  $: isValid = issues.length === 0;
</script>

<article class="rounded-2xl border border-border/80 bg-[#0f172a] p-4 text-foreground">
  <div class="mb-3 flex flex-wrap items-center justify-between gap-3">
    <h2 class="text-base font-bold">TOML Preview</h2>
    <button class="rounded-lg border border-primary/40 bg-primary/10 px-3 py-1.5 text-sm font-semibold text-primary hover:bg-primary/20" on:click={() => dispatch('copy')}>
      {copied ? 'Copied' : 'Copy'}
    </button>
  </div>

  <div class={isValid ? 'mb-3 w-full rounded-lg border border-success/40 bg-success/15 px-3 py-2 text-sm font-semibold text-success' : 'mb-3 w-full rounded-lg border border-destructive/40 bg-destructive/15 px-3 py-2 text-sm font-semibold text-destructive'}>
    {isValid ? 'VALIDATION: PASS' : `VALIDATION: FAIL (${issues.length} issue(s))`}
  </div>

  {#if !isValid}
    <div class="mb-3 rounded-lg border border-destructive/30 bg-destructive/10 p-3 text-xs text-destructive">
      {issues[0]}
    </div>
  {/if}

  <pre class="max-h-[36vh] overflow-auto rounded-xl bg-black/30 p-4 text-xs leading-6 md:text-sm">{toml}</pre>
</article>
