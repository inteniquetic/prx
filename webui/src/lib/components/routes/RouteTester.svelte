<script lang="ts">
  import CircleXIcon from '@lucide/svelte/icons/circle-x';
  import PlayIcon from '@lucide/svelte/icons/play';
  import ShieldAlertIcon from '@lucide/svelte/icons/shield-alert';
  import TargetIcon from '@lucide/svelte/icons/target';
  import * as Alert from '$lib/components/ui/alert';
  import * as Select from '$lib/components/ui/select';
  import * as Table from '$lib/components/ui/table';
  import { Badge } from '$lib/components/ui/badge';
  import { Button } from '$lib/components/ui/button';
  import { Input } from '$lib/components/ui/input';
  import { Label } from '$lib/components/ui/label';
  import { KeyValueList } from '$lib/components/ui/key-value-list';
  import { StatusDot } from '$lib/components/ui/status-dot';
  import { testRoute, type RouteTestResponse } from '$lib/api/admin';
  import { formatLatency } from '$lib/format';
  import { HTTP_METHODS } from '$lib/types/config';
  import { t } from '$lib/i18n';

  let {
    /** Prefilled from the route the table is showing, when there is one. */
    host = $bindable(''),
    path = $bindable('/'),
    onopenRoute
  }: { host?: string; path?: string; onopenRoute?: (name: string) => void } = $props();

  let method = $state('GET');
  let running = $state(false);
  let error = $state('');
  let result = $state<RouteTestResponse | null>(null);

  const matchedBy = (kind: string): string => $t(`routeTester.matched.${kind}`);

  async function run() {
    if (running) return;
    running = true;
    error = '';
    try {
      result = await testRoute({ method, host, path });
    } catch (err) {
      result = null;
      error = err instanceof Error ? err.message : String(err);
    } finally {
      running = false;
    }
  }
</script>

<div class="grid gap-4">
  <div class="grid gap-3 sm:grid-cols-[8rem_1fr_1fr_auto] sm:items-end">
    <div class="grid gap-1.5">
      <Label for="tester-method">{$t('routeTester.method')}</Label>
      <Select.Root type="single" bind:value={method}>
        <Select.Trigger id="tester-method">{method}</Select.Trigger>
        <Select.Content>
          {#each HTTP_METHODS as entry (entry)}
            <Select.Item value={entry} label={entry} />
          {/each}
        </Select.Content>
      </Select.Root>
    </div>

    <div class="grid gap-1.5">
      <Label for="tester-host">{$t('routeTester.host')}</Label>
      <Input id="tester-host" bind:value={host} placeholder={$t('routeTester.apiExampleCom')} />
    </div>

    <div class="grid gap-1.5">
      <Label for="tester-path">{$t('routeTester.path')}</Label>
      <Input id="tester-path" bind:value={path} placeholder={$t('routeTester.v1Users')} />
    </div>

    <Button onclick={run} disabled={running}>
      <PlayIcon aria-hidden="true" />
      {running ? 'Testing...' : 'Test'}
    </Button>
  </div>

  {#if error}
    <Alert.Root variant="destructive">
      <CircleXIcon />
      <Alert.Title>{$t('routeTester.failed')}</Alert.Title>
      <Alert.Description>{error}</Alert.Description>
    </Alert.Root>
  {/if}

  {#if result}
    {#if result.outcome === 'not_found'}
      <Alert.Root variant="warning">
        <CircleXIcon />
        <Alert.Title>{$t('routeTester.noMatch')}</Alert.Title>
        <Alert.Description>
          {$t('routeTester.noMatchBody', {
            method: result.request.method,
            host: result.request.normalized_host || $t('routeTester.anyHost'),
            path: result.request.path
          })}
        </Alert.Description>
      </Alert.Root>
    {:else if result.outcome === 'method_not_allowed'}
      <Alert.Root variant="warning">
        <ShieldAlertIcon />
        <Alert.Title>{$t('routeTester.methodBlocked')}</Alert.Title>
        <Alert.Description>
          {$t('routeTester.methodBlockedBody', { method: result.request.method })}
        </Alert.Description>
      </Alert.Root>
    {:else if result.route}
      <Alert.Root variant="success">
        <TargetIcon />
        <Alert.Title>
          {result.request.method}
          {result.request.normalized_host || $t('routeTester.anyHost')}{result.request.path}
          → {result.route.name}
        </Alert.Title>
        <Alert.Description>{matchedBy(result.route.matched_by)}</Alert.Description>
      </Alert.Root>

      <div class="grid gap-4 lg:grid-cols-2">
        <div class="rounded-lg border border-border p-3">
          <div class="mb-2 flex items-center justify-between gap-2">
            <h3 class="text-sm font-semibold">{$t('routeTester.route')}</h3>
            <Button variant="link" size="sm" onclick={() => onopenRoute?.(result!.route!.name)}>
              {$t('routeTester.open')}
            </Button>
          </div>
          <KeyValueList
            items={[
              { key: $t('routeTester.key.name'), value: result.route.name, mono: true },
              {
                key: $t('routeTester.key.host'),
                value: result.route.host || $t('routeTester.value.any'),
                mono: true
              },
              { key: $t('routeTester.key.path'), value: result.route.path_prefix, mono: true },
              {
                key: $t('routeTester.key.methods'),
                value: result.route.methods.length
                  ? result.route.methods.join(', ')
                  : $t('routeTester.value.any'),
                mono: true
              },
              {
                key: $t('routeTester.key.position'),
                value: $t('routeTester.value.position', { index: result.route.index + 1 })
              }
            ]}
          />
        </div>

        {#if result.service}
          <div class="rounded-lg border border-border p-3">
            <div class="mb-2 flex items-center justify-between gap-2">
              <h3 class="text-sm font-semibold">
                {$t('routeTester.service', { name: result.service.name })}
              </h3>
              <Badge variant="outline">{result.service.lb}</Badge>
            </div>

            <p class="mb-2 text-xs text-muted-foreground">{result.service.selection.note}</p>

            <Table.Root>
              <Table.Header>
                <Table.Row>
                  <Table.Head>{$t('routeTester.upstream')}</Table.Head>
                  <Table.Head>{$t('routeTester.state')}</Table.Head>
                  <Table.Head class="text-right">{$t('routeTester.inflight')}</Table.Head>
                  <Table.Head class="text-right">{$t('routeTester.p50')}</Table.Head>
                </Table.Row>
              </Table.Header>
              <Table.Body>
                {#each result.service.upstreams as upstream (upstream.addr)}
                  <Table.Row>
                    <Table.Cell class="font-mono text-xs">
                      {upstream.addr}
                      {#if result.service.selection.would_pick === upstream.addr}
                        <Badge variant="success" class="ml-1.5">{$t('routeTester.next')}</Badge>
                      {/if}
                    </Table.Cell>
                    <Table.Cell>
                      <StatusDot
                        status={upstream.circuit_open
                          ? 'circuit-open'
                          : upstream.available
                            ? 'healthy'
                            : 'down'}
                        reason={upstream.circuit_open
                          ? $t('routeTester.upstream.circuitOpen')
                          : upstream.available
                            ? $t('routeTester.upstream.healthy')
                            : $t('routeTester.upstream.down')}
                        showLabel
                      />
                    </Table.Cell>
                    <Table.Cell class="text-right tabular-nums">{upstream.inflight}</Table.Cell>
                    <Table.Cell class="text-right tabular-nums">
                      {upstream.ewma_us ? formatLatency(upstream.ewma_us / 1000) : '—'}
                    </Table.Cell>
                  </Table.Row>
                {/each}
              </Table.Body>
            </Table.Root>

            {#if !result.service.selection.deterministic}
              <p class="mt-2 text-xs text-muted-foreground">
                {$t('routeTester.nondeterministic')}
              </p>
            {/if}
          </div>
        {/if}
      </div>
    {/if}
  {/if}
</div>
