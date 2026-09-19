<script lang="ts" module>
  export type KeyValueItem = {
    key: string;
    value: string;
    /** Extra context shown under the value, e.g. where the value came from. */
    hint?: string;
    /** Render the value in the mono face — for hosts, paths and durations. */
    mono?: boolean;
    /** Offer a copy button for this value. */
    copyable?: boolean;
  };
</script>

<script lang="ts">
  import { CopyButton } from '../copy-button';
  import { cn } from '$lib/utils';

  let {
    items = [],
    /** `inline` puts key and value on one row; `stacked` puts the key above. */
    layout = 'inline',
    class: className
  }: { items?: KeyValueItem[]; layout?: 'inline' | 'stacked'; class?: string } = $props();
</script>

<dl
  data-slot="key-value-list"
  class={cn('divide-y divide-border text-sm', className)}
>
  {#each items as item (item.key)}
    <div
      class={cn(
        'gap-1 py-2',
        layout === 'inline'
          ? 'grid grid-cols-[minmax(6rem,1fr)_2fr] items-baseline gap-x-4'
          : 'grid'
      )}
    >
      <dt class="text-xs font-medium text-muted-foreground">{item.key}</dt>
      <dd class="flex min-w-0 items-start gap-1">
        <span class={cn('min-w-0 break-words', item.mono && 'font-mono text-xs')}>
          {item.value}
        </span>
        {#if item.copyable}
          <CopyButton text={item.value} label={`Copy ${item.key}`} class="-my-1 shrink-0" />
        {/if}
      </dd>
      {#if item.hint}
        <dd class={cn('text-xs text-muted-foreground', layout === 'inline' && 'col-start-2')}>
          {item.hint}
        </dd>
      {/if}
    </div>
  {/each}
</dl>
