<script lang="ts">
  /**
   * The three shapes most proxies start as (T309).
   *
   * Picking one does not apply anything: it fills the draft, so the first thing
   * a new operator sees is the same diff every other change goes through. That
   * is also the fastest way to learn the file — the template is the explanation.
   */
  import ArrowRightIcon from '@lucide/svelte/icons/arrow-right';

  import { Button } from '$lib/components/ui/button';
  import { toast } from '$lib/components/ui/sonner';
  import { TEMPLATES, type ConfigTemplate } from '$lib/configTemplates';
  import { plural, t } from '$lib/i18n';
  import { setDraft } from '$lib/stores/configDraft';

  let {
    /** Called after a template lands in the draft, e.g. to close a dialog. */
    onpicked,
    class: className = ''
  }: {
    onpicked?: (template: ConfigTemplate) => void;
    class?: string;
  } = $props();

  function pick(template: ConfigTemplate) {
    setDraft(template.toml());
    toast.message($t('template.loaded'));
    onpicked?.(template);
  }
</script>

<section class={className} data-slot="template-picker">
  <h3 class="text-sm font-semibold text-foreground">{$t('template.heading')}</h3>
  <p class="mt-1 text-xs text-muted-foreground">{$t('template.help')}</p>

  <ul class="mt-3 grid gap-3 sm:grid-cols-3">
    {#each TEMPLATES as template (template.id)}
      <li class="flex flex-col rounded-xl border border-border bg-card p-4 text-left">
        <h4 class="text-sm font-medium text-foreground">{$t(template.titleKey)}</h4>
        <p class="mt-1 flex-1 text-xs text-muted-foreground">{$t(template.descriptionKey)}</p>
        <p class="mt-2 font-mono text-[11px] text-muted-foreground">
          {$t('template.summary', {
            services: $plural('template.services', template.services),
            routes: $plural('template.routes', template.routes)
          })}
        </p>
        <Button
          variant="outline"
          size="sm"
          class="mt-3 w-full"
          onclick={() => pick(template)}
          data-template={template.id}
        >
          {$t('template.use')}
          <ArrowRightIcon class="size-4" aria-hidden="true" />
        </Button>
      </li>
    {/each}
  </ul>
</section>
