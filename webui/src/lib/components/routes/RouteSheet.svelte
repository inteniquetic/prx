<script lang="ts">
  import CircleAlertIcon from '@lucide/svelte/icons/circle-alert';
  import InfoIcon from '@lucide/svelte/icons/info';
  import * as Alert from '$lib/components/ui/alert';
  import * as Form from '$lib/components/ui/form';
  import * as Select from '$lib/components/ui/select';
  import * as Sheet from '$lib/components/ui/sheet';
  import * as Tabs from '$lib/components/ui/tabs';
  import { Button } from '$lib/components/ui/button';
  import { Checkbox } from '$lib/components/ui/checkbox';
  import { Input } from '$lib/components/ui/input';
  import { Label } from '$lib/components/ui/label';
  import { Switch } from '$lib/components/ui/switch';
  import HeaderRulesEditor from './HeaderRulesEditor.svelte';
  import { hasErrors, mapServerError, routeWarnings, validateRoute } from '$lib/routeValidation';
  import { HTTP_METHODS, type RouteConfig, type ServiceConfig } from '$lib/types/config';
  import { plural, t } from '$lib/i18n';

  let {
    open = $bindable(false),
    /** The route being edited, or a fresh one for a create. */
    route,
    mode = 'edit',
    routes = [],
    services = [],
    saving = false,
    /** Whatever the admin API said when the last save was rejected. */
    serverError = null,
    onsave,
    oncancel,
    oncreateService
  }: {
    open?: boolean;
    route: RouteConfig | null;
    mode?: 'create' | 'edit';
    routes?: RouteConfig[];
    services?: ServiceConfig[];
    saving?: boolean;
    serverError?: string | null;
    onsave?: (route: RouteConfig) => void;
    oncancel?: () => void;
    oncreateService?: () => void;
  } = $props();

  const clone = (value: RouteConfig): RouteConfig =>
    JSON.parse(JSON.stringify(value)) as RouteConfig;

  let draft = $state<RouteConfig | null>(null);
  let tab = $state('matching');
  let statusCodesText = $state('');
  let varyHeadersText = $state('');
  let originalName = $state<string | null>(null);

  // A new route object means a new form; editing one route then another must
  // not carry the first one's half-typed values across.
  $effect(() => {
    const source = route;
    if (!source) {
      draft = null;
      return;
    }
    draft = clone(source);
    originalName = mode === 'edit' ? source.name : null;
    statusCodesText = source.cache.cache_status_codes.join(', ');
    varyHeadersText = source.cache.vary_headers.join(', ');
    tab = 'matching';
  });

  const serviceNames = $derived(services.map((service) => service.name));

  const errors = $derived(
    draft
      ? validateRoute(draft, { routes, serviceNames, editingName: originalName })
      : ({} as ReturnType<typeof validateRoute>)
  );

  const warnings = $derived(
    draft ? routeWarnings(draft, { routes, serviceNames, editingName: originalName }) : []
  );

  // The server's answer is attached to the field it names, so a rejection lands
  // next to the input that caused it rather than in a toast far from the form.
  const mappedServerError = $derived(serverError ? mapServerError(serverError) : null);

  const fieldErrors = $derived((field: string): string[] => {
    const own = errors[field] ?? [];
    if (mappedServerError?.field === field) return [...own, mappedServerError.message];
    return own;
  });

  const blocked = $derived(hasErrors(errors));

  const tabErrors = $derived({
    matching: ['name', 'service', 'host', 'path_prefix', 'methods'].some(
      (field) => (errors[field] ?? []).length > 0
    ),
    headers: ['request_headers', 'response_headers'].some(
      (field) => (errors[field] ?? []).length > 0
    ),
    limits: Object.keys(errors).some(
      (field) => field.startsWith('rate_limit.') || field.startsWith('concurrency_limit.')
    ),
    cache: Object.keys(errors).some((field) => field.startsWith('cache.'))
  });

  function toggleMethod(method: string, checked: boolean) {
    if (!draft) return;
    const next = new Set(draft.methods.map((entry) => entry.toUpperCase()));
    if (checked) next.add(method);
    else next.delete(method);
    // Keep the canonical order rather than the order they were clicked in.
    draft.methods = HTTP_METHODS.filter((entry) => next.has(entry));
  }

  function commitStatusCodes() {
    if (!draft) return;
    draft.cache.cache_status_codes = statusCodesText
      .split(',')
      .map((code) => Number(code.trim()))
      .filter((code) => Number.isFinite(code) && code > 0);
  }

  function commitVaryHeaders() {
    if (!draft) return;
    draft.cache.vary_headers = varyHeadersText
      .split(',')
      .map((name) => name.trim())
      .filter(Boolean);
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
  <Sheet.Content side="right" class="w-full gap-0 overflow-y-auto sm:max-w-xl">
    <Sheet.Header class="border-b border-border">
      <Sheet.Title>{mode === 'create' ? 'New route' : `Edit ${originalName}`}</Sheet.Title>
      <Sheet.Description>
        {$t('routeSheet.appliesImmediately')}
      </Sheet.Description>
    </Sheet.Header>

    {#if draft}
      <!-- One narrowed reference for the whole form: the `{#if}` above does not
           reach inside the snippets each field is built from. -->
      {@const form = draft}
      <div class="grid gap-4 p-4">
        {#if mappedServerError}
          <!-- Always shown, even when the field is known: the field it names
               may be behind a collapsed section or another tab, and an error
               nobody can see is an error nobody can fix. -->
          <Alert.Root variant="destructive">
            <CircleAlertIcon />
            <Alert.Title>{$t('routeSheet.rejected')}</Alert.Title>
            <Alert.Description>{mappedServerError.message}</Alert.Description>
          </Alert.Root>
        {/if}

        {#each warnings as warning (warning)}
          <Alert.Root variant="warning">
            <InfoIcon />
            <Alert.Description>{warning}</Alert.Description>
          </Alert.Root>
        {/each}

        <Tabs.Root bind:value={tab}>
          <Tabs.List class="w-full">
            <Tabs.Trigger value="matching">
              {$t('routeSheet.tab.matching')}{tabErrors.matching ? ' •' : ''}
            </Tabs.Trigger>
            <Tabs.Trigger value="headers">
              {$t('routeSheet.tab.headers')}{tabErrors.headers ? ' •' : ''}
            </Tabs.Trigger>
            <Tabs.Trigger value="limits">
              {$t('routeSheet.tab.limits')}{tabErrors.limits ? ' •' : ''}
            </Tabs.Trigger>
            <Tabs.Trigger value="cache">
              {$t('routeSheet.tab.cache')}{tabErrors.cache ? ' •' : ''}
            </Tabs.Trigger>
          </Tabs.List>

          <!-- Matching -------------------------------------------------- -->
          <Tabs.Content value="matching" class="grid gap-4 pt-4">
            <Form.Field label={$t('routeSheet.name')} errors={fieldErrors('name')} required>
              {#snippet children({ props })}
                <Input {...props} bind:value={form.name} placeholder={$t('routeSheet.apiPublic')} />
              {/snippet}
            </Form.Field>

            <Form.Field
              label={$t('routeSheet.service')}
              description={$t('routeSheet.whereMatchingRequestsAre')}
              errors={fieldErrors('service')}
              required
            >
              {#snippet children({ props })}
                <div class="flex items-center gap-2">
                  <Select.Root type="single" bind:value={form.service}>
                    <Select.Trigger {...props} class="flex-1">
                      {form.service || 'Pick a service'}
                    </Select.Trigger>
                    <Select.Content>
                      {#each services as service (service.name)}
                        <Select.Item value={service.name} label={service.name}>
                          {service.name}
                          <span class="ml-auto text-xs text-muted-foreground">
                            {$plural('routeSheet.upstreams', service.upstreams.length)}
                          </span>
                        </Select.Item>
                      {/each}
                    </Select.Content>
                  </Select.Root>
                  <Button variant="outline" size="sm" onclick={() => oncreateService?.()}>
                    {$t('routeSheet.newService')}
                  </Button>
                </div>
              {/snippet}
            </Form.Field>

            <Form.Field
              label={$t('routeSheet.host')}
              description={$t('routeSheet.emptyMatchesAnyHost')}
              errors={fieldErrors('host')}
            >
              {#snippet children({ props })}
                <Input {...props} bind:value={form.host} placeholder={$t('routeSheet.apiExampleCom')} />
              {/snippet}
            </Form.Field>

            <Form.Field
              label={$t('routeSheet.pathPrefix')}
              description={$t('routeSheet.theLongestMatchingPrefix')}
              errors={fieldErrors('path_prefix')}
              required
            >
              {#snippet children({ props })}
                <Input {...props} bind:value={form.path_prefix} placeholder={$t('routeSheet.api')} />
              {/snippet}
            </Form.Field>

            <Form.Field
              label={$t('routeSheet.methods')}
              description={$t('routeSheet.noneSelectedMeansEvery')}
              errors={fieldErrors('methods')}
            >
              {#snippet children({ props })}
                <div {...props} class="flex flex-wrap gap-3">
                  {#each HTTP_METHODS as method (method)}
                    {@const checked = form.methods.some(
                      (entry) => entry.toUpperCase() === method
                    )}
                    <div class="flex items-center gap-1.5">
                      <Checkbox
                        id={`method-${method}`}
                        {checked}
                        onCheckedChange={(next) => toggleMethod(method, next)}
                      />
                      <Label for={`method-${method}`} class="font-mono text-xs">{method}</Label>
                    </div>
                  {/each}
                </div>
              {/snippet}
            </Form.Field>

            <div class="flex items-center justify-between rounded-lg border border-border p-3">
              <div class="grid gap-0.5">
                <Label for="route-default">{$t('routeSheet.fallback')}</Label>
                <p class="text-xs text-muted-foreground">
                  {$t('routeSheet.fallbackHelp')}
                </p>
              </div>
              <Switch id="route-default" bind:checked={form.is_default} />
            </div>

            <div class="flex items-center justify-between rounded-lg border border-border p-3">
              <div class="grid gap-0.5">
                <Label for="route-enabled">{$t('common.enabled')}</Label>
                <p class="text-xs text-muted-foreground">
                  {$t('routeSheet.enabledHelp')}
                </p>
              </div>
              <Switch id="route-enabled" bind:checked={form.enabled} />
            </div>
          </Tabs.Content>

          <!-- Headers -------------------------------------------------- -->
          <Tabs.Content value="headers" class="grid gap-6 pt-4">
            <div class="grid gap-3">
              <h3 class="text-sm font-semibold">{$t('routeSheet.requestHeaders')}</h3>
              <p class="text-xs text-muted-foreground">{$t('routeSheet.requestHeadersHelp')}</p>
              {#each fieldErrors('request_headers') as message (message)}
                <p class="text-xs font-medium text-destructive-emphasis">{message}</p>
              {/each}
              <HeaderRulesEditor bind:rules={form.request_headers} direction="request" />
            </div>

            <div class="grid gap-3">
              <h3 class="text-sm font-semibold">{$t('routeSheet.responseHeaders')}</h3>
              <p class="text-xs text-muted-foreground">{$t('routeSheet.responseHeadersHelp')}</p>
              {#each fieldErrors('response_headers') as message (message)}
                <p class="text-xs font-medium text-destructive-emphasis">{message}</p>
              {/each}
              <HeaderRulesEditor bind:rules={form.response_headers} direction="response" />
            </div>
          </Tabs.Content>

          <!-- Limits --------------------------------------------------- -->
          <Tabs.Content value="limits" class="grid gap-4 pt-4">
            <div class="flex items-center justify-between rounded-lg border border-border p-3">
              <div class="grid gap-0.5">
                <Label for="rate-enabled">{$t('routeSheet.rateLimit')}</Label>
                <p class="text-xs text-muted-foreground">{$t('routeSheet.rateLimitHelp')}</p>
              </div>
              <Switch id="rate-enabled" bind:checked={form.rate_limit.enabled} />
            </div>

            {#if form.rate_limit.enabled}
              <Form.Field
                label={$t('routeSheet.countPer')}
                description={$t('routeSheet.clientIpRouteOr')}
                errors={fieldErrors('rate_limit.key')}
              >
                {#snippet children({ props })}
                  <Input {...props} bind:value={form.rate_limit.key} placeholder={$t('routeSheet.clientIp')} />
                {/snippet}
              </Form.Field>

              <div class="grid gap-4 sm:grid-cols-2">
                <Form.Field
                  label={$t('routeSheet.requestsPerSecond')}
                  errors={fieldErrors('rate_limit.requests_per_second')}
                >
                  {#snippet children({ props })}
                    <Input
                      {...props}
                      type="number"
                      min="1"
                      bind:value={form.rate_limit.requests_per_second}
                    />
                  {/snippet}
                </Form.Field>

                <Form.Field label={$t('routeSheet.burst')} description={$t('routeSheet.0UsesTheSustained')}>
                  {#snippet children({ props })}
                    <Input {...props} type="number" min="0" bind:value={form.rate_limit.burst} />
                  {/snippet}
                </Form.Field>

                <Form.Field
                  label={$t('routeSheet.rejectionStatus')}
                  errors={fieldErrors('rate_limit.response_status')}
                >
                  {#snippet children({ props })}
                    <Input
                      {...props}
                      type="number"
                      min="400"
                      max="599"
                      bind:value={form.rate_limit.response_status}
                    />
                  {/snippet}
                </Form.Field>

                <Form.Field label={$t('routeSheet.trackedKeys')} errors={fieldErrors('rate_limit.max_entries')}>
                  {#snippet children({ props })}
                    <Input
                      {...props}
                      type="number"
                      min="1"
                      bind:value={form.rate_limit.max_entries}
                    />
                  {/snippet}
                </Form.Field>
              </div>

              <div class="flex items-center justify-between rounded-lg border border-border p-3">
                <Label for="retry-after">{$t('routeSheet.retryAfter')}</Label>
                <Switch id="retry-after" bind:checked={form.rate_limit.retry_after} />
              </div>
            {/if}

            <div class="grid gap-4 sm:grid-cols-2">
              <Form.Field
                label={$t('routeSheet.maxConcurrentRequests')}
                description={$t('routeSheet.0IsUnlimited')}
              >
                {#snippet children({ props })}
                  <Input
                    {...props}
                    type="number"
                    min="0"
                    bind:value={form.concurrency_limit.max_concurrent}
                  />
                {/snippet}
              </Form.Field>

              <Form.Field
                label={$t('routeSheet.rejectionStatus')}
                errors={fieldErrors('concurrency_limit.response_status')}
              >
                {#snippet children({ props })}
                  <Input
                    {...props}
                    type="number"
                    min="400"
                    max="599"
                    bind:value={form.concurrency_limit.response_status}
                  />
                {/snippet}
              </Form.Field>
            </div>
          </Tabs.Content>

          <!-- Cache ---------------------------------------------------- -->
          <Tabs.Content value="cache" class="grid gap-4 pt-4">
            <div class="flex items-center justify-between rounded-lg border border-border p-3">
              <div class="grid gap-0.5">
                <Label for="cache-enabled">{$t('routeSheet.cache')}</Label>
                <p class="text-xs text-muted-foreground">
                  {$t('routeSheet.cacheHelp')}
                </p>
              </div>
              <Switch id="cache-enabled" bind:checked={form.cache.enabled} />
            </div>

            {#if form.cache.enabled}
              <div class="grid gap-4 sm:grid-cols-2">
                <Form.Field label={$t('routeSheet.ttlMs')} errors={fieldErrors('cache.ttl_ms')}>
                  {#snippet children({ props })}
                    <Input {...props} type="number" min="1" bind:value={form.cache.ttl_ms} />
                  {/snippet}
                </Form.Field>

                <Form.Field
                  label={$t('routeSheet.maxBodyBytes')}
                  description={$t('routeSheet.biggerResponsesStreamThrough')}
                  errors={fieldErrors('cache.max_body_bytes')}
                >
                  {#snippet children({ props })}
                    <Input
                      {...props}
                      type="number"
                      min="1"
                      bind:value={form.cache.max_body_bytes}
                    />
                  {/snippet}
                </Form.Field>

                <Form.Field label={$t('routeSheet.maxEntries')}>
                  {#snippet children({ props })}
                    <Input {...props} type="number" min="1" bind:value={form.cache.max_entries} />
                  {/snippet}
                </Form.Field>

                <Form.Field label={$t('routeSheet.coalesceWaitMs')}>
                  {#snippet children({ props })}
                    <Input
                      {...props}
                      type="number"
                      min="0"
                      bind:value={form.cache.coalesce_wait_ms}
                    />
                  {/snippet}
                </Form.Field>
              </div>

              <Form.Field
                label={$t('routeSheet.cachedStatusCodes')}
                description={$t('routeSheet.commaSeparated')}
                errors={fieldErrors('cache.cache_status_codes')}
              >
                {#snippet children({ props })}
                  <Input
                    {...props}
                    class="font-mono text-xs"
                    bind:value={statusCodesText}
                    oninput={commitStatusCodes}
                  />
                {/snippet}
              </Form.Field>

              <Form.Field
                label={$t('routeSheet.varyOnHeaders')}
                description={$t('routeSheet.requestsThatDifferBy')}
                errors={fieldErrors('cache.vary_headers')}
              >
                {#snippet children({ props })}
                  <Input
                    {...props}
                    class="font-mono text-xs"
                    bind:value={varyHeadersText}
                    oninput={commitVaryHeaders}
                  />
                {/snippet}
              </Form.Field>

              <div class="flex items-center justify-between rounded-lg border border-border p-3">
                <Label for="cache-query">{$t('routeSheet.cacheKeyQuery')}</Label>
                <Switch id="cache-query" bind:checked={form.cache.key_query} />
              </div>

              <div class="flex items-center justify-between rounded-lg border border-border p-3">
                <Label for="cache-header">{$t('routeSheet.cacheHeader')}</Label>
                <Switch id="cache-header" bind:checked={form.cache.add_status_header} />
              </div>
            {/if}
          </Tabs.Content>
        </Tabs.Root>
      </div>
    {/if}

    <Sheet.Footer class="flex-row justify-end gap-2 border-t border-border">
      <Button variant="outline" onclick={cancel} disabled={saving}>{$t('common.cancel')}</Button>
      <Button onclick={save} disabled={saving || blocked}>
        {saving ? 'Saving...' : mode === 'create' ? 'Create route' : 'Save changes'}
      </Button>
    </Sheet.Footer>
  </Sheet.Content>
</Sheet.Root>
