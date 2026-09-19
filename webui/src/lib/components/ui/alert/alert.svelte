<script lang="ts" module>
  import type { WithElementRef } from 'bits-ui';
  import type { HTMLAttributes } from 'svelte/elements';
  import { type VariantProps, tv } from 'tailwind-variants';

  export const alertVariants = tv({
    base: 'relative grid w-full grid-cols-[0_1fr] items-start gap-y-0.5 rounded-lg border px-4 py-3 text-sm has-[>svg]:grid-cols-[calc(var(--spacing)*4)_1fr] has-[>svg]:gap-x-3 [&>svg]:size-4 [&>svg]:translate-y-0.5',
    variants: {
      variant: {
        default: 'border-border bg-card text-card-foreground',
        info: 'border-primary/30 bg-primary/10 text-foreground [&>svg]:text-primary',
        success: 'border-success/30 bg-success/10 text-foreground [&>svg]:text-success-emphasis',
        warning: 'border-warning/40 bg-warning/10 text-foreground [&>svg]:text-warning-emphasis',
        destructive:
          'border-destructive/30 bg-destructive/10 text-foreground [&>svg]:text-destructive-emphasis'
      }
    },
    defaultVariants: { variant: 'default' }
  });

  export type AlertVariant = VariantProps<typeof alertVariants>['variant'];
</script>

<script lang="ts">
  import { cn } from '$lib/utils';

  let {
    ref = $bindable(null),
    class: className,
    variant = 'default',
    children,
    ...restProps
  }: WithElementRef<HTMLAttributes<HTMLDivElement>> & { variant?: AlertVariant } = $props();
</script>

<div
  bind:this={ref}
  data-slot="alert"
  class={cn(alertVariants({ variant }), className)}
  role="alert"
  {...restProps}
>
  {@render children?.()}
</div>
