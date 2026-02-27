<script lang="ts">
  export let issues: string[] = [];
</script>

<article class="rounded-2xl border border-amber-400/40 bg-amber-500/10 p-4">
  <h2 class="mb-3 text-base font-bold text-amber-200">Validation</h2>
  {#if issues.length === 0}
    <p class="text-sm font-medium text-emerald-200">Configuration looks valid.</p>
  {:else}
    <ul class="space-y-2 text-sm text-amber-100">
      {#each issues as issue}
        <li class="rounded-md border border-amber-400/30 bg-slate-900/60 px-3 py-2">{issue}</li>
      {/each}
    </ul>
  {/if}
</article>
