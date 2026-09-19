<script lang="ts">
  import { Toaster as Sonner, type ToasterProps } from 'svelte-sonner';
  import { resolvedTheme } from '$lib/stores/theme';

  let { ...restProps }: ToasterProps = $props();
</script>

<!-- Toasts are portalled outside the themed tree, so they get the resolved
     theme handed to them rather than inheriting the `dark` class. -->
<Sonner
  theme={$resolvedTheme}
  class="toaster group"
  toastOptions={{
    classes: {
      toast:
        'group toast group-[.toaster]:bg-popover group-[.toaster]:text-popover-foreground group-[.toaster]:border group-[.toaster]:border-border group-[.toaster]:shadow-lg group-[.toaster]:rounded-lg',
      description: 'group-[.toast]:text-muted-foreground',
      actionButton: 'group-[.toast]:bg-primary group-[.toast]:text-primary-foreground',
      cancelButton: 'group-[.toast]:bg-muted group-[.toast]:text-muted-foreground',
      error: 'group-[.toaster]:text-destructive-emphasis',
      success: 'group-[.toaster]:text-success-emphasis',
      warning: 'group-[.toaster]:text-warning-emphasis',
      info: 'group-[.toaster]:text-primary'
    }
  }}
  {...restProps}
/>
