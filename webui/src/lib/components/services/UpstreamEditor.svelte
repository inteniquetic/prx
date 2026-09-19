<script lang="ts">
  import CircleCheckIcon from '@lucide/svelte/icons/circle-check';
  import CircleXIcon from '@lucide/svelte/icons/circle-x';
  import PlugIcon from '@lucide/svelte/icons/plug';
  import PlusIcon from '@lucide/svelte/icons/plus';
  import Trash2Icon from '@lucide/svelte/icons/trash-2';

  import * as Table from '$lib/components/ui/table';
  import { Badge } from '$lib/components/ui/badge';
  import { Button } from '$lib/components/ui/button';
  import { Input } from '$lib/components/ui/input';
  import { Label } from '$lib/components/ui/label';
  import { Slider } from '$lib/components/ui/slider';
  import { StatusDot, type UpstreamStatus as DotStatus } from '$lib/components/ui/status-dot';
  import { Switch } from '$lib/components/ui/switch';

  import { testUpstream, type UpstreamStatus } from '$lib/api/admin';
  import { formatLatency } from '$lib/format';
  import { createDefaultUpstream, type UpstreamConfig } from '$lib/types/config';
  import type { FieldErrors } from '$lib/serviceValidation';
  import { t } from '$lib/i18n';

  let {
    upstreams = $bindable(),
    /** Live state from `/web/services/status`, keyed by address. */
    status = {},
    errors = {},
    disabled = false
  }: {
    upstreams: UpstreamConfig[];
    status?: Record<string, UpstreamStatus>;
    errors?: FieldErrors;
    disabled?: boolean;
  } = $props();

  type TestResult = { healthy: boolean; detail: string };
  let tests = $state<Record<string, TestResult>>({});
  let testing = $state<string | null>(null);

  // What each upstream would get of the next requests. Computed the way the
  // balancer builds its ring — weight per enabled upstream over the total —
  // and replaced by the server's own number as soon as the config is applied.
  const shares = $derived.by(() => {
    const total = upstreams
      .filter((upstream) => upstream.enabled)
      .reduce((sum, upstream) => sum + Math.min(256, Math.max(1, upstream.weight)), 0);
    return upstreams.map((upstream) =>
      !upstream.enabled || total === 0 ? 0 : Math.min(256, Math.max(1, upstream.weight)) / total
    );
  });

  const dotStatus = (upstream: UpstreamConfig): DotStatus => {
    if (!upstream.enabled) return 'disabled';
    const live = status[upstream.addr];
    if (!live) return 'unknown';
    if (live.circuit_open) return 'circuit-open';
    if (!live.probe_healthy) return 'down';
    return 'healthy';
  };

  const dotReason = (upstream: UpstreamConfig): string => {
    if (!upstream.enabled) return $t('upstreamEditor.reason.drained');
    const live = status[upstream.addr];
    if (!live) return $t('upstreamEditor.reason.unknown');
    if (live.circuit_open) {
      const left = live.circuit_reopens_in_ms;
      return left
        ? $t('upstreamEditor.reason.circuitRetry', {
            failures: live.consecutive_failures,
            seconds: Math.ceil(left / 1000)
          })
        : $t('upstreamEditor.reason.circuit', { failures: live.consecutive_failures });
    }
    if (!live.probe_healthy) return $t('upstreamEditor.reason.down');
    return $t('upstreamEditor.reason.healthy');
  };

  async function runTest(addr: string) {
    if (!addr.trim() || testing) return;
    testing = addr;
    try {
      const result = await testUpstream(addr);
      tests = {
        ...tests,
        [addr]: {
          healthy: result.healthy,
          detail: result.healthy
            ? $t('upstreamEditor.test.connected', {
                latency: formatLatency(result.latency_ms ?? 0)
              })
            : (result.error ?? $t('upstreamEditor.test.failed'))
        }
      };
    } catch (err) {
      tests = {
        ...tests,
        [addr]: { healthy: false, detail: err instanceof Error ? err.message : String(err) }
      };
    } finally {
      testing = null;
    }
  }

  function addUpstream() {
    upstreams = [...upstreams, createDefaultUpstream()];
  }

  function removeUpstream(index: number) {
    upstreams = upstreams.filter((_, i) => i !== index);
  }
</script>

<div class="grid gap-3">
  {#each errors.upstreams ?? [] as message (message)}
    <p class="text-xs font-medium text-destructive-emphasis">{message}</p>
  {/each}

  <div class="rounded-lg border border-border">
    <Table.Root>
      <Table.Header>
        <Table.Row>
          <Table.Head>{$t('upstreamEditor.address')}</Table.Head>
          <Table.Head class="w-56">{$t('upstreamEditor.weight')}</Table.Head>
          <Table.Head>{$t('upstreamEditor.state')}</Table.Head>
          <Table.Head class="text-right">{$t('upstreamEditor.inflight')}</Table.Head>
          <Table.Head class="w-10">
            <span class="sr-only">{$t('upstreamEditor.actions')}</span>
          </Table.Head>
        </Table.Row>
      </Table.Header>
      <Table.Body>
        {#each upstreams as upstream, index (index)}
          {@const live = status[upstream.addr]}
          {@const test = tests[upstream.addr]}
          <Table.Row class={upstream.enabled ? '' : 'opacity-60'}>
            <Table.Cell class="align-top">
              <div class="grid gap-1.5">
                <Input
                  {disabled}
                  class="font-mono text-xs"
                  aria-label={$t('upstreamEditor.addressAria', { index: index + 1 })}
                  aria-invalid={(errors[`upstreams.${index}.addr`] ?? []).length > 0
                    ? 'true'
                    : undefined}
                  bind:value={upstream.addr}
                />
                {#each errors[`upstreams.${index}.addr`] ?? [] as message (message)}
                  <p class="text-xs font-medium text-destructive-emphasis">{message}</p>
                {/each}
                <div class="flex items-center gap-3 text-xs text-muted-foreground">
                  <label class="flex items-center gap-1.5">
                    <Switch
                      {disabled}
                      checked={upstream.tls}
                      onCheckedChange={(checked) => (upstream.tls = checked)}
                      aria-label={$t('upstreamEditor.tlsAria', { index: index + 1 })}
                    />
                    TLS
                  </label>
                  {#if upstream.tls}
                    <Input
                      {disabled}
                      class="h-7 max-w-40 font-mono text-xs"
                      placeholder={$t('upstreamEditor.sni')}
                      aria-label={$t('upstreamEditor.sniAria', { index: index + 1 })}
                      bind:value={upstream.sni}
                    />
                  {/if}
                </div>
                {#if test}
                  <p
                    class={`flex items-center gap-1.5 text-xs font-medium ${
                      test.healthy ? 'text-success-emphasis' : 'text-destructive-emphasis'
                    }`}
                  >
                    {#if test.healthy}
                      <CircleCheckIcon class="size-3.5" aria-hidden="true" />
                    {:else}
                      <CircleXIcon class="size-3.5" aria-hidden="true" />
                    {/if}
                    {test.detail}
                  </p>
                {/if}
              </div>
            </Table.Cell>

            <Table.Cell class="align-top">
              <div class="grid gap-1.5">
                <div class="flex items-center gap-2">
                  <Slider
                    type="single"
                    min={1}
                    max={20}
                    step={1}
                    {disabled}
                    value={Math.min(20, Math.max(1, upstream.weight))}
                    onValueChange={(value: number) => (upstream.weight = value)}
                    aria-label={$t('upstreamEditor.weightAria', { index: index + 1 })}
                    class="w-28"
                  />
                  <span class="w-8 text-right text-xs tabular-nums">{upstream.weight}</span>
                </div>
                <p class="text-xs text-muted-foreground tabular-nums">
                  {#if upstream.enabled}
                    {$t('upstreamEditor.share', { percent: (shares[index] * 100).toFixed(1) })}
                    {#if live && Math.abs(live.share - shares[index]) > 0.005}
                      <span class="text-warning-emphasis">
                        {$t('upstreamEditor.shareUntilApplied', {
                          percent: (live.share * 100).toFixed(1)
                        })}
                      </span>
                    {/if}
                  {:else}
                    {$t('upstreamEditor.drained')}
                  {/if}
                </p>
                <label class="flex items-center gap-1.5 text-xs text-muted-foreground">
                  <Switch
                    {disabled}
                    checked={!upstream.enabled}
                    onCheckedChange={(checked) => (upstream.enabled = !checked)}
                    aria-label={$t('upstreamEditor.drainAria', { index: index + 1 })}
                  />
                  {$t('upstreamEditor.drain')}
                </label>
              </div>
            </Table.Cell>

            <Table.Cell class="align-top">
              <div class="grid gap-1">
                <StatusDot
                  status={dotStatus(upstream)}
                  reason={dotReason(upstream)}
                  showLabel
                  tooltip={false}
                />
                {#if live?.circuit_open && live.circuit_reopens_in_ms}
                  <Badge variant="warning" class="w-fit tabular-nums">
                    {$t('upstreamEditor.retryIn', {
                      seconds: Math.ceil(live.circuit_reopens_in_ms / 1000)
                    })}
                  </Badge>
                {/if}
                {#if live && live.ewma_us > 0}
                  <span class="text-xs text-muted-foreground tabular-nums">
                    {formatLatency(live.ewma_us / 1000)}
                  </span>
                {/if}
              </div>
            </Table.Cell>

            <Table.Cell class="text-right align-top tabular-nums">
              {live?.inflight ?? '—'}
            </Table.Cell>

            <Table.Cell class="align-top">
              <div class="flex items-center gap-1">
                <Button
                  variant="ghost"
                  size="icon-sm"
                  disabled={disabled || testing === upstream.addr}
                  aria-label={$t('upstreamEditor.testAria', { index: index + 1 })}
                  onclick={() => runTest(upstream.addr)}
                >
                  <PlugIcon aria-hidden="true" />
                </Button>
                <Button
                  variant="ghost"
                  size="icon-sm"
                  disabled={disabled || upstreams.length <= 1}
                  aria-label={$t('upstreamEditor.removeAria', { index: index + 1 })}
                  onclick={() => removeUpstream(index)}
                >
                  <Trash2Icon aria-hidden="true" />
                </Button>
              </div>
            </Table.Cell>
          </Table.Row>
        {/each}
      </Table.Body>
    </Table.Root>
  </div>

  <div>
    <Button variant="outline" size="sm" {disabled} onclick={addUpstream}>
      <PlusIcon aria-hidden="true" />
      {$t('upstreamEditor.add')}
    </Button>
  </div>

  <p class="text-xs text-muted-foreground">{$t('upstreamEditor.note')}</p>
  <Label class="sr-only">{$t('upstreamEditor.label')}</Label>
</div>
