<script lang="ts">
  /**
   * Somebody else changed the config while this draft was open (T307).
   *
   * The one thing this must never do is overwrite their change quietly, so it
   * shows all three sides: what this tab started from, what the proxy is
   * running now, and what the draft would make of it. Then it asks which one
   * survives — it does not choose.
   */
  import * as Dialog from '$lib/components/ui/dialog';
  import { Button } from '$lib/components/ui/button';
  import { diffText } from '$lib/configDiff';
  import DiffView from './DiffView.svelte';
  import { t } from '$lib/i18n';

  let {
    open = $bindable(false),
    /** The version this draft was made from. */
    baseToml,
    /** What the proxy is running now. */
    currentToml,
    draftToml,
    applying = false,
    onkeepMine,
    ontakeTheirs,
    oncancel
  }: {
    open?: boolean;
    baseToml: string;
    currentToml: string;
    draftToml: string;
    applying?: boolean;
    /** Rebase the draft onto what is running and apply it. */
    onkeepMine: () => void;
    /** Throw the draft away and edit what is running. */
    ontakeTheirs: () => void;
    oncancel: () => void;
  } = $props();

  type Side = 'theirs' | 'mine' | 'result';
  let side: Side = $state('theirs');

  const theirChanges = $derived(diffText(baseToml, currentToml));
  const myChanges = $derived(diffText(baseToml, draftToml));
  const whatApplyingWouldDo = $derived(diffText(currentToml, draftToml));

  const tabs: { id: Side; key: string }[] = [
    { id: 'theirs', key: 'conflict.tab.theirs' },
    { id: 'mine', key: 'conflict.tab.mine' },
    { id: 'result', key: 'conflict.tab.result' }
  ];
</script>

<Dialog.Root bind:open onOpenChange={(next) => !next && oncancel()}>
  <Dialog.Content class="sm:max-w-4xl">
    <Dialog.Header>
      <Dialog.Title>{$t('conflict.title')}</Dialog.Title>
      <Dialog.Description>{$t('conflict.body')}</Dialog.Description>
    </Dialog.Header>

    <div class="flex gap-1 border-b border-border">
      {#each tabs as tab (tab.id)}
        <button
          type="button"
          class="border-b-2 px-3 py-1.5 text-sm font-medium transition-colors"
          class:border-primary={side === tab.id}
          class:text-foreground={side === tab.id}
          class:border-transparent={side !== tab.id}
          class:text-muted-foreground={side !== tab.id}
          onclick={() => (side = tab.id)}
          aria-pressed={side === tab.id}
        >
          {$t(tab.key)}
        </button>
      {/each}
    </div>

    {#if side === 'theirs'}
      <DiffView
        ops={theirChanges}
        beforeLabel={$t('conflict.label.base')}
        afterLabel={$t('conflict.label.current')}
        mode="split"
      />
    {:else if side === 'mine'}
      <DiffView
        ops={myChanges}
        beforeLabel={$t('conflict.label.base')}
        afterLabel={$t('conflict.label.mine')}
        mode="split"
      />
    {:else}
      <DiffView
        ops={whatApplyingWouldDo}
        beforeLabel={$t('conflict.label.runningNow')}
        afterLabel={$t('conflict.label.afterApply')}
        mode="split"
      />
      <p class="text-xs text-muted-foreground">{$t('conflict.note')}</p>
    {/if}

    <Dialog.Footer>
      <Button variant="ghost" onclick={oncancel} disabled={applying}>{$t('common.cancel')}</Button>
      <Button variant="outline" onclick={ontakeTheirs} disabled={applying}>
        {$t('conflict.takeTheirs')}
      </Button>
      <Button onclick={onkeepMine} disabled={applying}>
        {applying ? $t('common.applying') : $t('conflict.keepMine')}
      </Button>
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>
