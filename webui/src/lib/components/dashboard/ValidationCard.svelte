<script lang="ts">
  export let issues: string[] = [];
</script>

<article class="rounded-2xl border border-amber-200 bg-amber-50/90 p-4 shadow-panel">
  <h2 class="mb-3 text-base font-bold text-amber-900">Validation</h2>
  {#if issues.length === 0}
    <p class="text-sm font-medium text-emerald-700">Configuration looks valid.</p>
  {:else}
    <ul class="space-y-2 text-sm text-amber-900">
      {#each issues as issue}
        <li class="rounded-md bg-white/80 px-3 py-2">{issue}</li>
      {/each}
    </ul>
  {/if}
</article>
