<script lang="ts">
  import CloudOffIcon from '@lucide/svelte/icons/cloud-off';
  import PlugZapIcon from '@lucide/svelte/icons/plug-zap';
  import RefreshCwIcon from '@lucide/svelte/icons/refresh-cw';
  import WifiIcon from '@lucide/svelte/icons/wifi';
  import * as Tooltip from '$lib/components/ui/tooltip';
  import { connection, type ConnectionStatus } from '$lib/stores/connection';
  import { cn } from '$lib/utils';

  let { collapsed = false }: { collapsed?: boolean } = $props();

  // Shape and word first, colour second — same rule as StatusDot.
  const META: Record<ConnectionStatus, { label: string; icon: typeof WifiIcon; class: string }> = {
    connecting: { label: 'Connecting', icon: PlugZapIcon, class: 'text-muted-foreground' },
    online: { label: 'Online', icon: WifiIcon, class: 'text-success-emphasis' },
    reconnecting: { label: 'Reconnecting', icon: RefreshCwIcon, class: 'text-warning-emphasis' },
    offline: { label: 'Offline', icon: CloudOffIcon, class: 'text-destructive-emphasis' }
  };

  const state = $derived($connection);
  const meta = $derived(META[state.status]);
  const detail = $derived(
    state.status === 'online'
      ? 'The admin API answered the last request.'
      : state.lastError
        ? `Admin API: ${state.lastError}`
        : 'Waiting for the admin API.'
  );
</script>

<Tooltip.Root>
  <Tooltip.Trigger
    class={cn(
      'flex items-center gap-2 rounded-md px-2 py-1.5 text-xs font-medium focus-visible:ring-[3px] focus-visible:ring-ring/50 focus-visible:outline-none',
      meta.class
    )}
  >
    <meta.icon class={cn('size-4 shrink-0', state.status === 'reconnecting' && 'animate-spin')} aria-hidden="true" />
    <span class={cn(collapsed && 'sr-only')}>{meta.label}</span>
    <span class="sr-only">— admin API</span>
  </Tooltip.Trigger>
  <Tooltip.Content side="right">{detail}</Tooltip.Content>
</Tooltip.Root>
