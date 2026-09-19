<script lang="ts">
  import CircleAlertIcon from '@lucide/svelte/icons/circle-alert';
  import InfoIcon from '@lucide/svelte/icons/info';

  import * as Alert from '$lib/components/ui/alert';
  import * as Form from '$lib/components/ui/form';
  import * as Select from '$lib/components/ui/select';
  import * as Sheet from '$lib/components/ui/sheet';
  import * as Tabs from '$lib/components/ui/tabs';
  import { Button } from '$lib/components/ui/button';
  import { Input } from '$lib/components/ui/input';
  import { Label } from '$lib/components/ui/label';
  import { Switch } from '$lib/components/ui/switch';

  import UpstreamEditor from './UpstreamEditor.svelte';
  import type { UpstreamStatus } from '$lib/api/admin';
  import {
    hasErrors,
    mapServerError,
    serviceWarnings,
    validateService
  } from '$lib/serviceValidation';
  import {
    LB_STRATEGIES,
    type LbStrategy,
    type RouteConfig,
    type ServiceConfig,
    type UpstreamH2
  } from '$lib/types/config';

  let {
    open = $bindable(false),
    service,
    mode = 'edit',
    services = [],
    routes = [],
    status = {},
    saving = false,
    serverError = null,
    onsave,
    oncancel,
    ondelete
  }: {
    open?: boolean;
    service: ServiceConfig | null;
    mode?: 'create' | 'edit';
    services?: ServiceConfig[];
    routes?: RouteConfig[];
    status?: Record<string, UpstreamStatus>;
    saving?: boolean;
    serverError?: string | null;
    onsave?: (service: ServiceConfig) => void;
    oncancel?: () => void;
    ondelete?: (service: ServiceConfig) => void;
  } = $props();

  const clone = (value: ServiceConfig): ServiceConfig =>
    JSON.parse(JSON.stringify(value)) as ServiceConfig;

  let draft = $state<ServiceConfig | null>(null);
  let originalName = $state<string | null>(null);
  let tab = $state('upstreams');
  let expectedStatusText = $state('');

  $effect(() => {
    const source = service;
    if (!source) {
      draft = null;
      return;
    }
    draft = clone(source);
    originalName = mode === 'edit' ? source.name : null;
    expectedStatusText = source.health_check.expected_status.join(', ');
    tab = 'upstreams';
  });

  const errors = $derived(
    draft
      ? validateService(draft, { services, editingName: originalName })
      : ({} as ReturnType<typeof validateService>)
  );
  const warnings = $derived(draft ? serviceWarnings(draft, routes) : []);
  const mappedServerError = $derived(serverError ? mapServerError(serverError) : null);

  const fieldErrors = $derived((field: string): string[] => {
    const own = errors[field] ?? [];
    if (mappedServerError?.field === field) return [...own, mappedServerError.message];
    return own;
  });

  const blocked = $derived(hasErrors(errors));

  const usedBy = $derived.by(() => {
    const current = draft;
    if (!current) return [];
    const name = originalName ?? current.name;
    return routes.filter((route) => route.service === name);
  });

  const tabErrors = $derived({
    upstreams: Object.keys(errors).some(
      (field) => field === 'upstreams' || field.startsWith('upstreams.')
    ),
    balancing: ['name', 'sticky.name'].some((field) => (errors[field] ?? []).length > 0),
    retries: ['retry_budget_ratio', 'retry_budget_window_ms'].some(
      (field) => (errors[field] ?? []).length > 0
    ),
    health: Object.keys(errors).some(
      (field) => field.startsWith('health_check.') || field.startsWith('circuit_breaker.')
    )
  });

  function commitExpectedStatus() {
    if (!draft) return;
    draft.health_check.expected_status = expectedStatusText
      .split(',')
      .map((code) => Number(code.trim()))
      .filter((code) => Number.isFinite(code) && code > 0);
  }

  function save() {
    if (!draft || blocked) return;
    onsave?.(clone(draft));
  }

  function cancel() {
    open = false;
    oncancel?.();
  }
</script>

<Sheet.Root bind:open onOpenChange={(next) => !next && oncancel?.()}>
  <Sheet.Content side="right" class="w-full gap-0 overflow-y-auto sm:max-w-2xl">
    <Sheet.Header class="border-b border-border">
      <Sheet.Title>{mode === 'create' ? 'New service' : `Edit ${originalName}`}</Sheet.Title>
      <Sheet.Description>
        Changes are applied to the running proxy as soon as you save.
      </Sheet.Description>
    </Sheet.Header>

    {#if draft}
      {@const form = draft}
      <div class="grid gap-4 p-4">
        {#if mappedServerError}
          <Alert.Root variant="destructive">
            <CircleAlertIcon />
            <Alert.Title>The proxy rejected this service</Alert.Title>
            <Alert.Description>{mappedServerError.message}</Alert.Description>
          </Alert.Root>
        {/if}

        {#each warnings as warning (warning)}
          <Alert.Root variant="warning">
            <InfoIcon />
            <Alert.Description>{warning}</Alert.Description>
          </Alert.Root>
        {/each}

        <Form.Field label="Name" errors={fieldErrors('name')} required>
          {#snippet children({ props })}
            <Input {...props} bind:value={form.name} placeholder="checkout-api" />
          {/snippet}
        </Form.Field>

        <Tabs.Root bind:value={tab}>
          <Tabs.List class="w-full">
            <Tabs.Trigger value="upstreams">
              Upstreams{tabErrors.upstreams ? ' •' : ''}
            </Tabs.Trigger>
            <Tabs.Trigger value="balancing">
              Balancing{tabErrors.balancing ? ' •' : ''}
            </Tabs.Trigger>
            <Tabs.Trigger value="retries">Retries{tabErrors.retries ? ' •' : ''}</Tabs.Trigger>
            <Tabs.Trigger value="health">Health{tabErrors.health ? ' •' : ''}</Tabs.Trigger>
          </Tabs.List>

          <!-- Upstreams ------------------------------------------------- -->
          <Tabs.Content value="upstreams" class="pt-4">
            <UpstreamEditor bind:upstreams={form.upstreams} {status} {errors} disabled={saving} />
          </Tabs.Content>

          <!-- Balancing ------------------------------------------------- -->
          <Tabs.Content value="balancing" class="grid gap-4 pt-4">
            <div class="grid gap-1.5">
              <Label for="service-lb">Load balancing</Label>
              <Select.Root type="single" bind:value={form.lb as string}>
                <Select.Trigger id="service-lb">
                  {LB_STRATEGIES.find((entry) => entry.id === form.lb)?.label ?? form.lb}
                </Select.Trigger>
                <Select.Content>
                  {#each LB_STRATEGIES as strategy (strategy.id)}
                    <Select.Item value={strategy.id} label={strategy.label}>
                      <span class="grid gap-0.5">
                        <span>{strategy.label}</span>
                        <span class="text-xs text-muted-foreground">{strategy.description}</span>
                      </span>
                    </Select.Item>
                  {/each}
                </Select.Content>
              </Select.Root>
              <p class="text-xs text-muted-foreground">
                {LB_STRATEGIES.find((entry) => entry.id === form.lb)?.description}
              </p>
            </div>

            <div class="grid gap-1.5">
              <Label for="service-h2">HTTP to upstreams</Label>
              <Select.Root type="single" bind:value={form.upstream_h2 as string}>
                <Select.Trigger id="service-h2">{form.upstream_h2}</Select.Trigger>
                <Select.Content>
                  <Select.Item value="never" label="never">never — HTTP/1.1 only</Select.Item>
                  <Select.Item value="always" label="always">always — HTTP/2 only</Select.Item>
                  <Select.Item value="auto" label="auto">auto — h2 over TLS, else 1.1</Select.Item>
                </Select.Content>
              </Select.Root>
            </div>

            <div class="flex items-center justify-between rounded-lg border border-border p-3">
              <div class="grid gap-0.5">
                <Label for="sticky-enabled">Session affinity</Label>
                <p class="text-xs text-muted-foreground">
                  Keep a client on the upstream it used last. Falls back to normal balancing when
                  that upstream is unavailable.
                </p>
              </div>
              <Switch id="sticky-enabled" bind:checked={form.sticky.enabled} />
            </div>

            {#if form.sticky.enabled}
              <div class="grid gap-4 sm:grid-cols-2">
                <div class="grid gap-1.5">
                  <Label for="sticky-mode">Pin by</Label>
                  <Select.Root type="single" bind:value={form.sticky.mode as string}>
                    <Select.Trigger id="sticky-mode">{form.sticky.mode}</Select.Trigger>
                    <Select.Content>
                      <Select.Item value="cookie" label="cookie">cookie — exact pin</Select.Item>
                      <Select.Item value="client_ip" label="client_ip">client IP — hashed</Select.Item>
                      <Select.Item value="header" label="header">header — hashed</Select.Item>
                    </Select.Content>
                  </Select.Root>
                </div>

                <Form.Field
                  label={form.sticky.mode === 'header' ? 'Header name' : 'Cookie name'}
                  errors={fieldErrors('sticky.name')}
                >
                  {#snippet children({ props })}
                    <Input {...props} class="font-mono text-xs" bind:value={form.sticky.name} />
                  {/snippet}
                </Form.Field>

                {#if form.sticky.mode === 'cookie'}
                  <Form.Field label="Cookie lifetime (s)">
                    {#snippet children({ props })}
                      <Input {...props} type="number" min="1" bind:value={form.sticky.ttl_s} />
                    {/snippet}
                  </Form.Field>
                {/if}
              </div>
            {/if}
          </Tabs.Content>

          <!-- Retries --------------------------------------------------- -->
          <Tabs.Content value="retries" class="grid gap-4 pt-4">
            <div class="grid gap-4 sm:grid-cols-2">
              <Form.Field label="Max retries" description="Per request, on top of the first try.">
                {#snippet children({ props })}
                  <Input {...props} type="number" min="0" bind:value={form.max_retries} />
                {/snippet}
              </Form.Field>

              <Form.Field label="Backoff (ms)">
                {#snippet children({ props })}
                  <Input {...props} type="number" min="0" bind:value={form.retry_backoff_ms} />
                {/snippet}
              </Form.Field>

              <Form.Field
                label="Retry budget"
                description="Share of recent successes that may be spent on retries. 0 turns the budget off."
                errors={fieldErrors('retry_budget_ratio')}
              >
                {#snippet children({ props })}
                  <Input
                    {...props}
                    type="number"
                    step="0.05"
                    min="0"
                    max="10"
                    bind:value={form.retry_budget_ratio}
                  />
                {/snippet}
              </Form.Field>

              <Form.Field
                label="Budget window (ms)"
                errors={fieldErrors('retry_budget_window_ms')}
              >
                {#snippet children({ props })}
                  <Input
                    {...props}
                    type="number"
                    min="1"
                    bind:value={form.retry_budget_window_ms}
                  />
                {/snippet}
              </Form.Field>

              <Form.Field
                label="Free retries per window"
                description="Below this many requests, retries are always allowed, so an idle service is not locked out by its own budget."
              >
                {#snippet children({ props })}
                  <Input
                    {...props}
                    type="number"
                    min="0"
                    bind:value={form.retry_budget_min_per_window}
                  />
                {/snippet}
              </Form.Field>

              <Form.Field
                label="Request timeout (ms)"
                description="Whole request including retries. 0 is off."
              >
                {#snippet children({ props })}
                  <Input {...props} type="number" min="0" bind:value={form.request_timeout_ms} />
                {/snippet}
              </Form.Field>
            </div>

            <div class="flex items-center justify-between rounded-lg border border-border p-3">
              <div class="grid gap-0.5">
                <Label for="retry-idempotent">Only retry idempotent methods</Label>
                <p class="text-xs text-muted-foreground">
                  Once a request may have reached the upstream, replaying a POST can double an
                  action. Connect failures are always retried — nothing was sent yet.
                </p>
              </div>
              <Switch id="retry-idempotent" bind:checked={form.retry_idempotent_only} />
            </div>
          </Tabs.Content>

          <!-- Health ---------------------------------------------------- -->
          <Tabs.Content value="health" class="grid gap-4 pt-4">
            <div class="flex items-center justify-between rounded-lg border border-border p-3">
              <div class="grid gap-0.5">
                <Label for="hc-enabled">Active health checks</Label>
                <p class="text-xs text-muted-foreground">
                  Probe upstreams in the background instead of finding out from a real request.
                </p>
              </div>
              <Switch id="hc-enabled" bind:checked={form.health_check.enabled} />
            </div>

            {#if form.health_check.enabled}
              <div class="grid gap-4 sm:grid-cols-2">
                <div class="grid gap-1.5">
                  <Label for="hc-kind">Probe</Label>
                  <Select.Root type="single" bind:value={form.health_check.kind as string}>
                    <Select.Trigger id="hc-kind">{form.health_check.kind}</Select.Trigger>
                    <Select.Content>
                      <Select.Item value="tcp" label="tcp">tcp — open a connection</Select.Item>
                      <Select.Item value="http" label="http">http — GET and check the status</Select.Item>
                    </Select.Content>
                  </Select.Root>
                </div>

                {#if form.health_check.kind === 'http'}
                  <Form.Field label="Path" errors={fieldErrors('health_check.path')}>
                    {#snippet children({ props })}
                      <Input {...props} class="font-mono text-xs" bind:value={form.health_check.path} />
                    {/snippet}
                  </Form.Field>

                  <Form.Field
                    label="Healthy statuses"
                    description="Comma separated."
                    errors={fieldErrors('health_check.expected_status')}
                  >
                    {#snippet children({ props })}
                      <Input
                        {...props}
                        class="font-mono text-xs"
                        bind:value={expectedStatusText}
                        oninput={commitExpectedStatus}
                      />
                    {/snippet}
                  </Form.Field>
                {/if}

                <Form.Field label="Interval (ms)" errors={fieldErrors('health_check.interval_ms')}>
                  {#snippet children({ props })}
                    <Input {...props} type="number" min="1" bind:value={form.health_check.interval_ms} />
                  {/snippet}
                </Form.Field>

                <Form.Field label="Timeout (ms)" errors={fieldErrors('health_check.timeout_ms')}>
                  {#snippet children({ props })}
                    <Input {...props} type="number" min="1" bind:value={form.health_check.timeout_ms} />
                  {/snippet}
                </Form.Field>

                <Form.Field
                  label="Probes to recover"
                  errors={fieldErrors('health_check.healthy_threshold')}
                >
                  {#snippet children({ props })}
                    <Input
                      {...props}
                      type="number"
                      min="1"
                      bind:value={form.health_check.healthy_threshold}
                    />
                  {/snippet}
                </Form.Field>

                <Form.Field
                  label="Probes to fail"
                  errors={fieldErrors('health_check.unhealthy_threshold')}
                >
                  {#snippet children({ props })}
                    <Input
                      {...props}
                      type="number"
                      min="1"
                      bind:value={form.health_check.unhealthy_threshold}
                    />
                  {/snippet}
                </Form.Field>
              </div>
            {/if}

            <div class="flex items-center justify-between rounded-lg border border-border p-3">
              <div class="grid gap-0.5">
                <Label for="cb-enabled">Circuit breaker</Label>
                <p class="text-xs text-muted-foreground">
                  Take an upstream out after consecutive failures, and let it back in after a
                  cool-off. prx has no half-open state: when the time is up the upstream is simply
                  used again.
                </p>
              </div>
              <Switch id="cb-enabled" bind:checked={form.circuit_breaker.enabled} />
            </div>

            {#if form.circuit_breaker.enabled}
              <div class="grid gap-4 sm:grid-cols-2">
                <Form.Field
                  label="Failures before opening"
                  errors={fieldErrors('circuit_breaker.consecutive_failures')}
                >
                  {#snippet children({ props })}
                    <Input
                      {...props}
                      type="number"
                      min="1"
                      bind:value={form.circuit_breaker.consecutive_failures}
                    />
                  {/snippet}
                </Form.Field>

                <Form.Field label="Stay open for (ms)" errors={fieldErrors('circuit_breaker.open_ms')}>
                  {#snippet children({ props })}
                    <Input
                      {...props}
                      type="number"
                      min="1"
                      bind:value={form.circuit_breaker.open_ms}
                    />
                  {/snippet}
                </Form.Field>
              </div>
            {/if}
          </Tabs.Content>
        </Tabs.Root>

        {#if mode === 'edit'}
          <div class="grid gap-2 rounded-lg border border-destructive/30 p-3">
            <div class="grid gap-0.5">
              <p class="text-sm font-medium">Delete this service</p>
              <p class="text-xs text-muted-foreground">
                {#if usedBy.length > 0}
                  Blocked: {usedBy.length} route{usedBy.length === 1 ? '' : 's'} still point here
                  — {usedBy.map((route) => route.name).join(', ')}. Move them first.
                {:else}
                  Nothing routes here, so it is safe to remove.
                {/if}
              </p>
            </div>
            <div>
              <Button
                variant="destructive"
                size="sm"
                disabled={saving || usedBy.length > 0}
                onclick={() => ondelete?.(form)}
              >
                Delete service
              </Button>
            </div>
          </div>
        {/if}
      </div>
    {/if}

    <Sheet.Footer class="flex-row justify-end gap-2 border-t border-border">
      <Button variant="outline" onclick={cancel} disabled={saving}>Cancel</Button>
      <Button onclick={save} disabled={saving || blocked}>
        {saving ? 'Saving...' : mode === 'create' ? 'Create service' : 'Save changes'}
      </Button>
    </Sheet.Footer>
  </Sheet.Content>
</Sheet.Root>
