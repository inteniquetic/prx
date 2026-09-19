<script lang="ts">
  export let issues: string[] = [];
</script>

<article class="rounded-2xl border border-warning/40 bg-warning/10 p-4">
  <h2 class="mb-3 text-base font-bold text-warning">Validation</h2>
  {#if issues.length === 0}
    <p class="text-sm font-medium text-success">Configuration looks valid.</p>
  {:else}
    <ul class="space-y-2 text-sm text-warning">
      {#each issues as issue}
        <li class="rounded-md border border-warning/30 bg-card/60 px-3 py-2">{issue}</li>
      {/each}
    </ul>
  {/if}
</article>
