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

  const MATCHED_BY: Record<string, string> = {
    exact_host: 'Exact host match',
    wildcard_host: 'Wildcard host match',
    any_host: 'Route with no host',
    default_route: 'Fallback route — nothing else covered this request'
  };

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
      <Label for="tester-method">Method</Label>
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
      <Label for="tester-host">Host</Label>
      <Input id="tester-host" bind:value={host} placeholder="api.example.com" />
    </div>

    <div class="grid gap-1.5">
      <Label for="tester-path">Path</Label>
      <Input id="tester-path" bind:value={path} placeholder="/v1/users" />
    </div>

    <Button onclick={run} disabled={running}>
      <PlayIcon aria-hidden="true" />
      {running ? 'Testing...' : 'Test'}
    </Button>
  </div>

  {#if error}
    <Alert.Root variant="destructive">
      <CircleXIcon />
      <Alert.Title>The test could not run</Alert.Title>
      <Alert.Description>{error}</Alert.Description>
    </Alert.Root>
  {/if}

  {#if result}
    {#if result.outcome === 'not_found'}
      <Alert.Root variant="warning">
        <CircleXIcon />
        <Alert.Title>No route matches</Alert.Title>
        <Alert.Description>
          {result.request.method}
          {result.request.normalized_host || '(any host)'}{result.request.path} would get a 404.
          Add a fallback route to catch requests like this.
        </Alert.Description>
      </Alert.Root>
    {:else if result.outcome === 'method_not_allowed'}
      <Alert.Root variant="warning">
        <ShieldAlertIcon />
        <Alert.Title>Matched a route, but not the method</Alert.Title>
        <Alert.Description>
          A route covers this host and path but does not accept {result.request.method}, so the
          request gets a 405. prx does not fall through to a broader route here — that would take
          the request past the restriction on purpose.
        </Alert.Description>
      </Alert.Root>
    {:else if result.route}
      <Alert.Root variant="success">
        <TargetIcon />
        <Alert.Title>
          {result.request.method}
          {result.request.normalized_host || '(any host)'}{result.request.path}
          → {result.route.name}
        </Alert.Title>
        <Alert.Description>{MATCHED_BY[result.route.matched_by]}</Alert.Description>
      </Alert.Root>

      <div class="grid gap-4 lg:grid-cols-2">
        <div class="rounded-lg border border-border p-3">
          <div class="mb-2 flex items-center justify-between gap-2">
            <h3 class="text-sm font-semibold">Route</h3>
            <Button variant="link" size="sm" onclick={() => onopenRoute?.(result!.route!.name)}>
              Open
            </Button>
          </div>
          <KeyValueList
            items={[
              { key: 'Name', value: result.route.name, mono: true },
              { key: 'Host', value: result.route.host || 'any', mono: true },
              { key: 'Path prefix', value: result.route.path_prefix, mono: true },
              {
                key: 'Methods',
                value: result.route.methods.length ? result.route.methods.join(', ') : 'any',
                mono: true
              },
              { key: 'Position', value: `#${result.route.index + 1} in the config` }
            ]}
          />
        </div>

        {#if result.service}
          <div class="rounded-lg border border-border p-3">
            <div class="mb-2 flex items-center justify-between gap-2">
              <h3 class="text-sm font-semibold">Service · {result.service.name}</h3>
              <Badge variant="outline">{result.service.lb}</Badge>
            </div>

            <p class="mb-2 text-xs text-muted-foreground">{result.service.selection.note}</p>

            <Table.Root>
              <Table.Header>
                <Table.Row>
                  <Table.Head>Upstream</Table.Head>
                  <Table.Head>State</Table.Head>
                  <Table.Head class="text-right">In flight</Table.Head>
                  <Table.Head class="text-right">p50-ish</Table.Head>
                </Table.Row>
              </Table.Header>
              <Table.Body>
                {#each result.service.upstreams as upstream (upstream.addr)}
                  <Table.Row>
                    <Table.Cell class="font-mono text-xs">
                      {upstream.addr}
                      {#if result.service.selection.would_pick === upstream.addr}
                        <Badge variant="success" class="ml-1.5">next</Badge>
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
                          ? 'The circuit breaker is open after repeated failures.'
                          : upstream.available
                            ? 'Probes are passing and the circuit is closed.'
                            : 'The last health probe failed.'}
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
                This strategy picks at request time, so there is no single upstream to name in
                advance.
              </p>
            {/if}
          </div>
        {/if}
      </div>
    {/if}
  {/if}
</div>
