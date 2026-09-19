<script lang="ts" module>
  export type FormFieldControlProps = {
    id: string;
    'aria-describedby': string | undefined;
    'aria-invalid': 'true' | undefined;
  };
</script>

<script lang="ts">
  import { useId } from 'bits-ui';
  import type { Snippet } from 'svelte';
  import CircleAlertIcon from '@lucide/svelte/icons/circle-alert';
  import { Label } from '../label';
  import { cn } from '$lib/utils';

  let {
    id = useId(),
    label,
    description,
    errors = [],
    required = false,
    class: className,
    children
  }: {
    id?: string;
    label?: string;
    description?: string;
    /** Validation messages for the control; a non-empty list marks it invalid. */
    errors?: string[];
    required?: boolean;
    class?: string;
    children: Snippet<[{ props: FormFieldControlProps }]>;
  } = $props();

  const descriptionId = $derived(`${id}-description`);
  const errorId = $derived(`${id}-error`);

  const invalid = $derived(errors.length > 0);
  const describedBy = $derived(
    [description ? descriptionId : null, invalid ? errorId : null].filter(Boolean).join(' ') ||
      undefined
  );
</script>

<div data-slot="form-field" class={cn('grid gap-2', className)}>
  {#if label}
    <Label for={id}>
      {label}
      {#if required}
        <span aria-hidden="true" class="text-destructive-emphasis">*</span>
        <span class="sr-only">(required)</span>
      {/if}
    </Label>
  {/if}

  {@render children({
    props: {
      id,
      'aria-describedby': describedBy,
      'aria-invalid': invalid ? 'true' : undefined
    }
  })}

  {#if description}
    <p id={descriptionId} data-slot="form-description" class="text-xs text-muted-foreground">
      {description}
    </p>
  {/if}

  {#if invalid}
    <!-- The icon and the text carry the error on their own: a red outline alone
         is invisible to anyone who cannot tell red from grey. -->
    <p
      id={errorId}
      data-slot="form-message"
      class="flex items-start gap-1.5 text-xs font-medium text-destructive-emphasis"
    >
      <CircleAlertIcon class="mt-px size-3.5 shrink-0" aria-hidden="true" />
      <span>{errors.join(' · ')}</span>
    </p>
  {/if}
</div>
