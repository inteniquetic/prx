<script lang="ts">
  /**
   * The shape of a table before its rows arrive (T309).
   *
   * Rows of the same height as the real ones, so the page does not jump when
   * the data lands — which is the whole point of a skeleton over a spinner.
   */
  import Skeleton from './skeleton.svelte';
  import { cn } from '$lib/utils';

  let {
    rows = 5,
    columns = 4,
    /** Read out instead of the boxes, which are hidden from assistive tech. */
    label,
    class: className
  }: {
    rows?: number;
    columns?: number;
    label: string;
    class?: string;
  } = $props();
</script>

<div
  class={cn('rounded-xl border border-border bg-card', className)}
  role="status"
  aria-busy="true"
  aria-label={label}
  data-slot="skeleton-table"
>
  <div class="flex gap-4 border-b border-border px-4 py-3">
    {#each { length: columns } as _, column (column)}
      <Skeleton class="h-4 flex-1" />
    {/each}
  </div>
  {#each { length: rows } as _, row (row)}
    <div class="flex gap-4 border-b border-border/60 px-4 py-4 last:border-0">
      {#each { length: columns } as _, column (column)}
        <Skeleton class={cn('h-4 flex-1', column === 0 && 'max-w-40')} />
      {/each}
    </div>
  {/each}
</div>
