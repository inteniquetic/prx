<script lang="ts">
  import CheckIcon from '@lucide/svelte/icons/check';
  import CopyIcon from '@lucide/svelte/icons/copy';
  import { Button, type ButtonSize, type ButtonVariant } from '../button';
  import { cn } from '$lib/utils';

  let {
    /** The text to put on the clipboard. */
    text,
    label = 'Copy',
    copiedLabel = 'Copied',
    showLabel = false,
    variant = 'ghost',
    size = 'icon-sm',
    class: className
  }: {
    text: string;
    label?: string;
    copiedLabel?: string;
    showLabel?: boolean;
    variant?: ButtonVariant;
    size?: ButtonSize;
    class?: string;
  } = $props();

  let copied = $state(false);
  let failed = $state(false);
  let timer: ReturnType<typeof setTimeout> | undefined;

  /**
   * navigator.clipboard is only available on a secure origin, and prx is often
   * reached over plain HTTP on a LAN address. The textarea route is the
   * fallback that keeps the button working there.
   */
  async function writeToClipboard(value: string): Promise<boolean> {
    try {
      if (navigator.clipboard?.writeText) {
        await navigator.clipboard.writeText(value);
        return true;
      }
    } catch {
      // Permission denied or an insecure context: try the old way.
    }

    try {
      const scratch = document.createElement('textarea');
      scratch.value = value;
      scratch.setAttribute('readonly', '');
      scratch.style.position = 'fixed';
      scratch.style.opacity = '0';
      document.body.appendChild(scratch);
      scratch.select();
      const ok = document.execCommand('copy');
      document.body.removeChild(scratch);
      return ok;
    } catch {
      return false;
    }
  }

  async function copy() {
    const ok = await writeToClipboard(text);
    copied = ok;
    failed = !ok;
    clearTimeout(timer);
    timer = setTimeout(() => {
      copied = false;
      failed = false;
    }, 2000);
  }

  $effect(() => () => clearTimeout(timer));
</script>

<Button
  {variant}
  {size}
  data-slot="copy-button"
  class={cn(copied && 'text-success-emphasis', className)}
  onclick={copy}
  aria-label={showLabel ? undefined : copied ? copiedLabel : label}
>
  {#if copied}
    <CheckIcon aria-hidden="true" />
  {:else}
    <CopyIcon aria-hidden="true" />
  {/if}
  {#if showLabel}
    <span>{copied ? copiedLabel : label}</span>
  {/if}
</Button>

<!-- Announced rather than only drawn, so the copy is confirmed without sight. -->
<span aria-live="polite" class="sr-only">
  {#if copied}{copiedLabel}{:else if failed}Copy failed{/if}
</span>
