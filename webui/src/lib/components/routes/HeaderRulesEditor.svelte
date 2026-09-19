<script lang="ts">
  import PlusIcon from '@lucide/svelte/icons/plus';
  import Trash2Icon from '@lucide/svelte/icons/trash-2';
  import { Button } from '$lib/components/ui/button';
  import { Input } from '$lib/components/ui/input';
  import { Label } from '$lib/components/ui/label';
  import type { HeaderRules } from '$lib/types/config';
  import { t } from '$lib/i18n';

  let {
    rules = $bindable(),
    /** `request` or `response`, used for ids and wording. */
    direction,
    disabled = false
  }: { rules: HeaderRules; direction: 'request' | 'response'; disabled?: boolean } = $props();

  // The config stores set/add as maps, but a map cannot hold a half-typed key,
  // so the editor works on pairs and writes the map back on every change.
  type Pair = { key: string; value: string };

  const toPairs = (map: Record<string, string>): Pair[] =>
    Object.entries(map).map(([key, value]) => ({ key, value }));

  let setPairs = $state<Pair[]>(toPairs(rules.set));
  let addPairs = $state<Pair[]>(toPairs(rules.add));
  let removeText = $state(rules.remove.join(', '));

  const toMap = (pairs: Pair[]): Record<string, string> => {
    const map: Record<string, string> = {};
    for (const pair of pairs) {
      if (pair.key.trim()) map[pair.key.trim()] = pair.value;
    }
    return map;
  };

  function commit() {
    rules = {
      set: toMap(setPairs),
      add: toMap(addPairs),
      remove: removeText
        .split(',')
        .map((name) => name.trim())
        .filter(Boolean)
    };
  }
</script>

{#snippet pairList(title: string, hint: string, pairs: Pair[], kind: 'set' | 'add')}
  <div class="grid gap-2">
    <div>
      <Label class="text-xs uppercase tracking-wide text-muted-foreground">{title}</Label>
      <p class="text-xs text-muted-foreground">{hint}</p>
    </div>

    {#each pairs as pair, i (i)}
      <div class="flex items-center gap-2">
        <Input
          {disabled}
          class="font-mono text-xs"
          placeholder={$t('headerRules.xHeaderName')}
          aria-label={`${kind} ${direction} header name ${i + 1}`}
          bind:value={pair.key}
          oninput={commit}
        />
        <Input
          {disabled}
          class="font-mono text-xs"
          placeholder={$t('headerRules.value')}
          aria-label={`${kind} ${direction} header value ${i + 1}`}
          bind:value={pair.value}
          oninput={commit}
        />
        <Button
          variant="ghost"
          size="icon-sm"
          {disabled}
          aria-label={$t('headerRules.removeRule', { kind, index: i + 1 })}
          onclick={() => {
            pairs.splice(i, 1);
            commit();
          }}
        >
          <Trash2Icon aria-hidden="true" />
        </Button>
      </div>
    {/each}

    <div>
      <Button
        variant="outline"
        size="sm"
        {disabled}
        onclick={() => {
          pairs.push({ key: '', value: '' });
          commit();
        }}
      >
        <PlusIcon aria-hidden="true" />
        {$t('headerRules.add')}
      </Button>
    </div>
  </div>
{/snippet}

<div class="grid gap-4">
  {@render pairList(
    'Set',
    `Replaces the header on every ${direction}, whether or not it was there.`,
    setPairs,
    'set'
  )}
  {@render pairList(
    'Add',
    'Appends a value, keeping any the client or upstream already sent.',
    addPairs,
    'add'
  )}

  <div class="grid gap-1.5">
    <Label for={`${direction}-remove`} class="text-xs uppercase tracking-wide text-muted-foreground">
      {$t('headerRules.remove')}
    </Label>
    <Input
      id={`${direction}-remove`}
      {disabled}
      class="font-mono text-xs"
      placeholder={$t('headerRules.xInternalTokenX')}
      bind:value={removeText}
      oninput={commit}
    />
    <p class="text-xs text-muted-foreground">{$t('headerRules.commaSeparated')}</p>
  </div>
</div>
