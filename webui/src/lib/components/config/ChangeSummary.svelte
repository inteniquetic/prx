<script lang="ts">
  /**
   * What the diff means, in words (T307).
   *
   * The line count on the Apply button says how much changed. This says what:
   * which route now points somewhere else, which upstream lost its weight,
   * which service disappeared. Anything that moves traffic is marked, because
   * that is the part worth a second look before applying.
   */
  import ArrowRightIcon from '@lucide/svelte/icons/arrow-right';
  import MinusIcon from '@lucide/svelte/icons/minus';
  import PencilIcon from '@lucide/svelte/icons/pencil';
  import PlusIcon from '@lucide/svelte/icons/plus';

  import type { ChangeSummary } from '$lib/configChanges';

  let {
    summary,
    class: className = ''
  }: {
    summary: ChangeSummary | null;
    class?: string;
  } = $props();

  const iconFor = (kind: 'add' | 'remove' | 'modify') =>
    kind === 'add' ? PlusIcon : kind === 'remove' ? MinusIcon : PencilIcon;

  const badgeTint = (kind: 'add' | 'remove' | 'modify'): string =>
    kind === 'add'
      ? 'bg-success/15 text-success-emphasis'
      : kind === 'remove'
        ? 'bg-destructive/15 text-destructive-emphasis'
        : 'bg-primary/15 text-primary';
</script>

{#if summary && !summary.identical}
  <ul class="space-y-2 {className}">
    {#each summary.changes as change, index (index)}
      {@const Icon = iconFor(change.kind)}
      <li class="flex gap-2.5 rounded-lg border border-border/70 bg-card/60 px-3 py-2">
        <span
          class="mt-0.5 flex size-5 shrink-0 items-center justify-center rounded-full {badgeTint(
            change.kind
          )}"
        >
          <Icon class="size-3" aria-hidden="true" />
        </span>
        <div class="min-w-0 flex-1">
          <p class="text-sm font-medium text-foreground">
            {change.subject}
            <span class="text-muted-foreground">
              {change.kind === 'add' ? 'added' : change.kind === 'remove' ? 'removed' : 'changed'}
            </span>
            {#if change.traffic}
              <span
                class="ml-1.5 inline-flex items-center gap-1 rounded-full bg-warning/15 px-1.5 py-0.5 align-middle text-[10px] font-semibold text-warning-emphasis"
              >
                <ArrowRightIcon class="size-2.5" aria-hidden="true" />
                affects traffic
              </span>
            {/if}
          </p>
          {#if change.details.length > 0}
            <ul class="mt-1 space-y-0.5">
              {#each change.details as detail, detailIndex (detailIndex)}
                <li class="font-mono text-xs text-muted-foreground">{detail}</li>
              {/each}
            </ul>
          {/if}
        </div>
      </li>
    {/each}
  </ul>
{:else if summary}
  <p class="text-sm text-muted-foreground {className}">
    Nothing in the config itself changed — only comments or formatting.
  </p>
{:else}
  <p class="text-sm text-muted-foreground {className}">
    The draft has to be valid before its changes can be described.
  </p>
{/if}
