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

  const tabs: { id: Side; label: string }[] = [
    { id: 'theirs', label: 'Their change' },
    { id: 'mine', label: 'Your change' },
    { id: 'result', label: 'What Apply would do' }
  ];
</script>

<Dialog.Root bind:open onOpenChange={(next) => !next && oncancel()}>
  <Dialog.Content class="sm:max-w-4xl">
    <Dialog.Header>
      <Dialog.Title>The config changed while you were editing</Dialog.Title>
      <Dialog.Description>
        Someone else applied a config after this draft was loaded — another tab, the Routes
        page, or an edit to the file itself. Nothing has been written.
      </Dialog.Description>
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
          {tab.label}
        </button>
      {/each}
    </div>

    {#if side === 'theirs'}
      <DiffView
        ops={theirChanges}
        beforeLabel="What you started from"
        afterLabel="What the proxy is running now"
        mode="split"
      />
    {:else if side === 'mine'}
      <DiffView
        ops={myChanges}
        beforeLabel="What you started from"
        afterLabel="Your draft"
        mode="split"
      />
    {:else}
      <DiffView
        ops={whatApplyingWouldDo}
        beforeLabel="Running now"
        afterLabel="After applying your draft"
        mode="split"
      />
      <p class="text-xs text-muted-foreground">
        Applying keeps every line of your draft, including the lines that would undo their
        change.
      </p>
    {/if}

    <Dialog.Footer>
      <Button variant="ghost" onclick={oncancel} disabled={applying}>Cancel</Button>
      <Button variant="outline" onclick={ontakeTheirs} disabled={applying}>
        Discard mine, edit theirs
      </Button>
      <Button onclick={onkeepMine} disabled={applying}>
        {applying ? 'Applying…' : 'Apply mine over theirs'}
      </Button>
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>
