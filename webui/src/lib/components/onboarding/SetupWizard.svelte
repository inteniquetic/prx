<script lang="ts">
  /**
   * The first two minutes (T309).
   *
   * A proxy with no services answers every request with a 404, and the way out
   * of that is three facts: what to call the thing, where it runs, and which
   * requests should reach it. The wizard asks those, shows the config they add
   * up to, and puts it in the draft — where it goes through the same review and
   * Apply as every other change, because a config nobody read is how the first
   * outage happens.
   */
  import CheckIcon from '@lucide/svelte/icons/check';
  import PlusIcon from '@lucide/svelte/icons/plus';

  import * as Dialog from '$lib/components/ui/dialog';
  import { Button } from '$lib/components/ui/button';
  import { Input } from '$lib/components/ui/input';
  import { Label } from '$lib/components/ui/label';
  import { toast } from '$lib/components/ui/sonner';
  import { looksLikeAddress, wizardToml } from '$lib/configTemplates';
  import { t } from '$lib/i18n';
  import { setDraft } from '$lib/stores/configDraft';
  import TemplatePicker from './TemplatePicker.svelte';

  let {
    open = $bindable(false),
    /** Called once the draft holds a config, so the page can show the diff. */
    onfinished,
    ondismissed
  }: {
    open?: boolean;
    onfinished?: () => void;
    ondismissed?: () => void;
  } = $props();

  const STEPS = 3;

  let step = $state(1);
  let serviceName = $state('app');
  let upstreams = $state(['127.0.0.1:3000']);
  let host = $state('');
  let pathPrefix = $state('/');

  const answers = $derived({ serviceName, upstreams, host, pathPrefix });
  const preview = $derived(wizardToml(answers));

  const badAddresses = $derived(
    upstreams.filter((address) => address.trim() !== '' && !looksLikeAddress(address))
  );
  const canContinue = $derived(
    step === 1
      ? serviceName.trim().length > 0
      : step === 2
        ? upstreams.some((address) => looksLikeAddress(address))
        : pathPrefix.trim().startsWith('/')
  );

  function finish() {
    setDraft(preview);
    open = false;
    toast.success($t('wizard.finished'));
    onfinished?.();
  }

  function dismiss() {
    open = false;
    ondismissed?.();
  }
</script>

<Dialog.Root bind:open onOpenChange={(next) => !next && ondismissed?.()}>
  <Dialog.Content class="sm:max-w-2xl" data-slot="setup-wizard">
    <Dialog.Header>
      <Dialog.Title>{$t('wizard.title')}</Dialog.Title>
      <Dialog.Description>{$t('wizard.subtitle')}</Dialog.Description>
    </Dialog.Header>

    <!-- Where you are, in words as well as in the bar. -->
    <div class="flex items-center gap-3">
      <div class="h-1.5 flex-1 overflow-hidden rounded-full bg-muted">
        <div
          class="h-full rounded-full bg-primary transition-[width]"
          style={`width: ${(step / STEPS) * 100}%`}
        ></div>
      </div>
      <span class="shrink-0 text-xs text-muted-foreground" aria-live="polite">
        {$t('wizard.step', { step, total: STEPS })}
      </span>
    </div>

    {#if step === 1}
      <div class="grid gap-2">
        <h3 class="text-sm font-medium text-foreground">{$t('wizard.step1.title')}</h3>
        <p class="text-xs text-muted-foreground">{$t('wizard.step1.help')}</p>
        <Label for="wizard-service" class="sr-only">{$t('wizard.step1.label')}</Label>
        <Input id="wizard-service" bind:value={serviceName} autocomplete="off" />
      </div>

      <TemplatePicker
        class="border-t border-border pt-4"
        onpicked={() => {
          open = false;
          onfinished?.();
        }}
      />
    {:else if step === 2}
      <div class="grid gap-2">
        <h3 class="text-sm font-medium text-foreground">{$t('wizard.step2.title')}</h3>
        <p class="text-xs text-muted-foreground">{$t('wizard.step2.help')}</p>

        {#each upstreams as _, index (index)}
          <Input
            bind:value={upstreams[index]}
            aria-label={$t('wizard.step2.label', { index: index + 1 })}
            placeholder="127.0.0.1:3000"
            autocomplete="off"
          />
        {/each}

        {#if badAddresses.length > 0}
          <p class="text-xs text-destructive-emphasis" role="alert">
            {$t('wizard.step2.invalid')}
          </p>
        {/if}

        <div>
          <Button
            variant="outline"
            size="sm"
            onclick={() => (upstreams = [...upstreams, ''])}
          >
            <PlusIcon class="size-4" aria-hidden="true" />
            {$t('wizard.step2.add')}
          </Button>
        </div>
      </div>
    {:else}
      <div class="grid gap-2">
        <h3 class="text-sm font-medium text-foreground">{$t('wizard.step3.title')}</h3>
        <p class="text-xs text-muted-foreground">{$t('wizard.step3.help')}</p>

        <Label for="wizard-host">{$t('wizard.step3.host')}</Label>
        <Input id="wizard-host" bind:value={host} placeholder="api.example.com" autocomplete="off" />

        <Label for="wizard-path">{$t('wizard.step3.path')}</Label>
        <Input id="wizard-path" bind:value={pathPrefix} autocomplete="off" />
      </div>

      <div class="grid gap-1.5">
        <p class="text-xs text-muted-foreground">{$t('wizard.review')}</p>
        <pre
          class="max-h-48 overflow-auto rounded-lg border border-border bg-muted/40 p-3 font-mono text-[11px] leading-5 text-foreground">{preview}</pre>
      </div>
    {/if}

    <Dialog.Footer>
      <Button variant="ghost" onclick={dismiss}>{$t('wizard.dismiss')}</Button>
      {#if step > 1}
        <Button variant="outline" onclick={() => (step -= 1)}>{$t('common.back')}</Button>
      {/if}
      {#if step < STEPS}
        <Button disabled={!canContinue} onclick={() => (step += 1)}>{$t('common.next')}</Button>
      {:else}
        <Button disabled={!canContinue} onclick={finish}>
          <CheckIcon class="size-4" aria-hidden="true" />
          {$t('wizard.finish')}
        </Button>
      {/if}
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>
