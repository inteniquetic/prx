<script lang="ts">
  import ActivityIcon from '@lucide/svelte/icons/activity';
  import GaugeIcon from '@lucide/svelte/icons/gauge';
  import HardDriveIcon from '@lucide/svelte/icons/hard-drive';
  import InfoIcon from '@lucide/svelte/icons/info';
  import PlusIcon from '@lucide/svelte/icons/plus';
  import RouteIcon from '@lucide/svelte/icons/route';
  import SettingsIcon from '@lucide/svelte/icons/settings';
  import TriangleAlertIcon from '@lucide/svelte/icons/triangle-alert';

  import * as Alert from '$lib/components/ui/alert';
  import * as Card from '$lib/components/ui/card';
  import * as Command from '$lib/components/ui/command';
  import * as Dialog from '$lib/components/ui/dialog';
  import * as DropdownMenu from '$lib/components/ui/dropdown-menu';
  import * as Form from '$lib/components/ui/form';
  import * as Popover from '$lib/components/ui/popover';
  import * as Select from '$lib/components/ui/select';
  import * as Sheet from '$lib/components/ui/sheet';
  import * as Table from '$lib/components/ui/table';
  import * as Tabs from '$lib/components/ui/tabs';
  import * as Tooltip from '$lib/components/ui/tooltip';
  import { Badge } from '$lib/components/ui/badge';
  import { Button } from '$lib/components/ui/button';
  import { Checkbox } from '$lib/components/ui/checkbox';
  import { ConfirmDialog } from '$lib/components/ui/confirm-dialog';
  import { CopyButton } from '$lib/components/ui/copy-button';
  import { EmptyState } from '$lib/components/ui/empty-state';
  import { Input } from '$lib/components/ui/input';
  import { KeyValueList } from '$lib/components/ui/key-value-list';
  import { Label } from '$lib/components/ui/label';
  import { MetricTile } from '$lib/components/ui/metric-tile';
  import { Separator } from '$lib/components/ui/separator';
  import { Skeleton } from '$lib/components/ui/skeleton';
  import { StatusDot, STATUS_META, type UpstreamStatus } from '$lib/components/ui/status-dot';
  import { Switch } from '$lib/components/ui/switch';
  import { Textarea } from '$lib/components/ui/textarea';
  import { Toaster, toast } from '$lib/components/ui/sonner';

  import { formatBytes, formatLatency, formatPercent, formatThroughput } from '$lib/format';
  import { cycleTheme, initTheme, themeChoice } from '$lib/stores/theme';
  import type { ButtonVariant } from '$lib/components/ui/button';
  import type { BadgeVariant } from '$lib/components/ui/badge';
  import { onMount } from 'svelte';

  const buttonVariants: ButtonVariant[] = [
    'default',
    'secondary',
    'outline',
    'ghost',
    'destructive',
    'link'
  ];
  const badgeVariants: BadgeVariant[] = [
    'default',
    'secondary',
    'outline',
    'success',
    'warning',
    'destructive'
  ];
  const statuses = Object.keys(STATUS_META) as UpstreamStatus[];
  const reasons: Record<UpstreamStatus, string> = {
    healthy: 'last 20 probes passed, p95 8 ms',
    degraded: '3 of 20 probes timed out',
    down: 'connection refused on 10.0.0.4:8080',
    'circuit-open': 'opened 12s ago after 5 consecutive 502s',
    disabled: 'taken out of rotation in the config',
    unknown: 'no probe has run since the last reload'
  };

  const tokens = [
    ['background / foreground', 'bg-background text-foreground'],
    ['card / card-foreground', 'bg-card text-card-foreground'],
    ['muted / muted-foreground', 'bg-muted text-muted-foreground'],
    ['primary / primary-foreground', 'bg-primary text-primary-foreground'],
    ['secondary / secondary-foreground', 'bg-secondary text-secondary-foreground'],
    ['accent / accent-foreground', 'bg-accent text-accent-foreground'],
    ['success / success-foreground', 'bg-success text-success-foreground'],
    ['warning / warning-foreground', 'bg-warning text-warning-foreground'],
    ['circuit / circuit-foreground', 'bg-circuit text-circuit-foreground'],
    ['destructive / destructive-foreground', 'bg-destructive text-destructive-foreground'],
    ['tooltip / tooltip-foreground', 'bg-tooltip text-tooltip-foreground']
  ];

  const spacing = [1, 2, 3, 4, 6, 8, 12];
  const radii = [
    ['sm', 'rounded-sm'],
    ['md', 'rounded-md'],
    ['lg', 'rounded-lg'],
    ['xl', 'rounded-xl'],
    ['full', 'rounded-full']
  ];
  const elevations = [
    ['xs — resting control', 'shadow-xs'],
    ['sm — card', 'shadow-sm'],
    ['md — popover, dropdown', 'shadow-md'],
    ['lg — dialog, sheet, toast', 'shadow-lg']
  ];

  let dialogOpen = $state(false);
  let sheetOpen = $state(false);
  let confirmOpen = $state(false);
  let paletteOpen = $state(false);
  let checked = $state(true);
  let switched = $state(true);
  let selected = $state('round-robin');
  let routeName = $state('');
  let tab = $state('overview');

  const nameErrors = $derived(
    routeName.includes(' ') ? ['Route names cannot contain spaces.'] : []
  );

  const kv = [
    { key: 'Listen', value: '0.0.0.0:8080', mono: true, copyable: true },
    { key: 'Upstream', value: '10.0.0.4:8080', mono: true, copyable: true },
    { key: 'Strategy', value: 'least-latency', hint: 'session affinity by cookie' },
    { key: 'Timeout', value: '2,000 ms' }
  ];

  onMount(() => initTheme());

  function onPaletteKey(event: KeyboardEvent) {
    if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === 'k') {
      event.preventDefault();
      paletteOpen = !paletteOpen;
    }
  }
</script>

<svelte:window onkeydown={onPaletteKey} />

<Toaster />

<div class="h-full overflow-y-auto bg-background text-foreground">
  <div class="mx-auto flex max-w-5xl flex-col gap-10 px-6 py-10">
    <header class="flex flex-wrap items-center justify-between gap-4">
      <div>
        <h1 class="text-2xl font-semibold tracking-tight">prx design system</h1>
        <p class="text-sm text-muted-foreground">
          Every component on one page, in both themes. Dev build only.
        </p>
      </div>
      <div class="flex items-center gap-2">
        <Button variant="outline" onclick={() => cycleTheme($themeChoice)}>
          Theme: {$themeChoice}
        </Button>
        <Button variant="outline" onclick={() => (paletteOpen = true)}>
          Command palette <kbd class="ml-1 font-mono text-xs">⌘K</kbd>
        </Button>
      </div>
    </header>

    <section class="grid gap-3">
      <h2 class="text-lg font-semibold">Colour tokens</h2>
      <div class="grid gap-2 sm:grid-cols-2">
        {#each tokens as [name, classes] (name)}
          <div class="flex items-center justify-between gap-3 rounded-lg border border-border p-3 {classes}">
            <span class="text-sm font-medium">{name}</span>
            <span class="text-xs opacity-80">Aa 123</span>
          </div>
        {/each}
      </div>
      <p class="text-xs text-muted-foreground">
        Pairs are verified by <code class="font-mono">npm run contrast</code>; nothing here is a
        hardcoded colour.
      </p>
    </section>

    <section class="grid gap-3">
      <h2 class="text-lg font-semibold">Spacing, radius, elevation</h2>
      <div class="flex flex-wrap items-end gap-3">
        {#each spacing as step (step)}
          <div class="grid gap-1 text-center">
            <div class="bg-primary" style="width: calc(var(--spacing) * {step}); height: calc(var(--spacing) * {step});"></div>
            <span class="text-xs text-muted-foreground">{step}</span>
          </div>
        {/each}
      </div>
      <div class="flex flex-wrap gap-3">
        {#each radii as [name, cls] (name)}
          <div class="grid gap-1 text-center">
            <div class="size-12 border border-border bg-muted {cls}"></div>
            <span class="text-xs text-muted-foreground">{name}</span>
          </div>
        {/each}
      </div>
      <div class="flex flex-wrap gap-3">
        {#each elevations as [name, cls] (name)}
          <div class="rounded-lg border border-border bg-card px-4 py-3 text-xs text-card-foreground {cls}">
            {name}
          </div>
        {/each}
      </div>
    </section>

    <section class="grid gap-3">
      <h2 class="text-lg font-semibold">Number formats</h2>
      <KeyValueList
        items={[
          { key: 'Latency', value: `${formatLatency(0.42)} · ${formatLatency(7.3)} · ${formatLatency(1240)}`, mono: true },
          { key: 'Throughput', value: `${formatThroughput(8.4)} · ${formatThroughput(2430)}`, mono: true },
          { key: 'Bytes (IEC)', value: `${formatBytes(900)} · ${formatBytes(1536)} · ${formatBytes(5.4 * 1024 ** 3)}`, mono: true },
          { key: 'Ratio', value: formatPercent(0.9976, 2), mono: true }
        ]}
      />
    </section>

    <section class="grid gap-3">
      <h2 class="text-lg font-semibold">Buttons</h2>
      <div class="flex flex-wrap items-center gap-2">
        {#each buttonVariants as variant (variant)}
          <Button {variant}>{variant}</Button>
        {/each}
      </div>
      <div class="flex flex-wrap items-center gap-2">
        <Button size="sm">small</Button>
        <Button>default</Button>
        <Button size="lg">large</Button>
        <Button size="icon" aria-label="Add route"><PlusIcon /></Button>
        <Button disabled>disabled</Button>
        <CopyButton text="prx --config /etc/prx/Prx.toml" showLabel size="sm" variant="outline" />
      </div>
    </section>

    <section class="grid gap-3">
      <h2 class="text-lg font-semibold">Badges &amp; status</h2>
      <div class="flex flex-wrap items-center gap-2">
        {#each badgeVariants as variant (variant)}
          <Badge {variant}>{variant}</Badge>
        {/each}
      </div>
      <div class="flex flex-wrap items-center gap-4 rounded-lg border border-border bg-card p-4">
        {#each statuses as status (status)}
          <StatusDot {status} reason={reasons[status]} showLabel />
        {/each}
      </div>
      <p class="text-xs text-muted-foreground">
        Each status has its own glyph and word, so none of them depends on colour alone.
      </p>
    </section>

    <section class="grid gap-3">
      <h2 class="text-lg font-semibold">Metrics</h2>
      <div class="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
        <MetricTile
          label="p95 latency"
          value={formatLatency(18)}
          delta={-3.4}
          unit="ms"
          betterWhen="lower"
          icon={GaugeIcon}
          history={[26, 24, 25, 21, 22, 19, 18]}
        />
        <MetricTile
          label="Throughput"
          value={formatThroughput(2430)}
          delta={180}
          betterWhen="higher"
          icon={ActivityIcon}
          history={[1800, 1950, 2100, 2050, 2280, 2400, 2430]}
        />
        <MetricTile
          label="Cache size"
          value={formatBytes(5.4 * 1024 ** 3)}
          icon={HardDriveIcon}
          betterWhen="neutral"
        />
      </div>
    </section>

    <section class="grid gap-3">
      <h2 class="text-lg font-semibold">Alerts</h2>
      <Alert.Root variant="info">
        <InfoIcon />
        <Alert.Title>Config reloaded</Alert.Title>
        <Alert.Description>The running proxy picked up the new routes.</Alert.Description>
      </Alert.Root>
      <Alert.Root variant="warning">
        <TriangleAlertIcon />
        <Alert.Title>One upstream is degraded</Alert.Title>
        <Alert.Description>api-2 answered 3 of the last 20 probes slowly.</Alert.Description>
      </Alert.Root>
      <Alert.Root variant="destructive">
        <TriangleAlertIcon />
        <Alert.Title>Save blocked</Alert.Title>
        <Alert.Description>2 validation issues have to be fixed first.</Alert.Description>
      </Alert.Root>
    </section>

    <section class="grid gap-3">
      <h2 class="text-lg font-semibold">Forms</h2>
      <Card.Root>
        <Card.Header>
          <Card.Title>Route</Card.Title>
          <Card.Description>Fields wire their own label, hint and error together.</Card.Description>
        </Card.Header>
        <Card.Content class="grid gap-4">
          <Form.Field
            label="Name"
            description="Used in logs and metrics."
            errors={nameErrors}
            required
          >
            {#snippet children({ props })}
              <Input {...props} bind:value={routeName} placeholder="api-public" />
            {/snippet}
          </Form.Field>

          <Form.Field label="Notes" description="Free text, kept in the config comment.">
            {#snippet children({ props })}
              <Textarea {...props} placeholder="Owned by the platform team" />
            {/snippet}
          </Form.Field>

          <div class="grid gap-2">
            <Label for="sg-strategy">Load balancing</Label>
            <Select.Root type="single" bind:value={selected}>
              <Select.Trigger id="sg-strategy" class="w-60">
                {selected}
              </Select.Trigger>
              <Select.Content>
                <Select.Item value="round-robin" label="round-robin" />
                <Select.Item value="least-latency" label="least-latency" />
                <Select.Item value="random" label="random" />
              </Select.Content>
            </Select.Root>
          </div>

          <div class="flex items-center gap-2">
            <Checkbox id="sg-retry" bind:checked />
            <Label for="sg-retry">Retry idempotent requests</Label>
          </div>

          <div class="flex items-center gap-2">
            <Switch id="sg-compress" bind:checked={switched} />
            <Label for="sg-compress">Compress responses</Label>
          </div>
        </Card.Content>
        <Card.Footer class="gap-2">
          <Button onclick={() => toast.success('Route saved')}>Save</Button>
          <Button variant="outline" onclick={() => toast.error('Could not reach the admin API')}>
            Fail
          </Button>
        </Card.Footer>
      </Card.Root>
    </section>

    <section class="grid gap-3">
      <h2 class="text-lg font-semibold">Overlays</h2>
      <div class="flex flex-wrap items-center gap-2">
        <Button variant="outline" onclick={() => (dialogOpen = true)}>Dialog</Button>
        <Button variant="outline" onclick={() => (sheetOpen = true)}>Sheet</Button>
        <Button variant="outline" onclick={() => (confirmOpen = true)}>Confirm</Button>

        <Popover.Root>
          <Popover.Trigger class="inline-flex h-9 items-center rounded-md border border-border px-4 text-sm font-medium hover:bg-accent hover:text-accent-foreground">
            Popover
          </Popover.Trigger>
          <Popover.Content class="grid gap-2">
            <p class="text-sm font-medium">Probe settings</p>
            <p class="text-sm text-muted-foreground">Interval 2s, timeout 500 ms, 3 failures.</p>
          </Popover.Content>
        </Popover.Root>

        <DropdownMenu.Root>
          <DropdownMenu.Trigger class="inline-flex h-9 items-center rounded-md border border-border px-4 text-sm font-medium hover:bg-accent hover:text-accent-foreground">
            Menu
          </DropdownMenu.Trigger>
          <DropdownMenu.Content class="w-48">
            <DropdownMenu.Group>
              <!-- bits-ui only accepts a group heading inside its group. -->
              <DropdownMenu.Label>Route</DropdownMenu.Label>
              <DropdownMenu.Item>Duplicate<DropdownMenu.Shortcut>⌘D</DropdownMenu.Shortcut></DropdownMenu.Item>
              <DropdownMenu.Item>Export</DropdownMenu.Item>
            </DropdownMenu.Group>
            <DropdownMenu.Separator />
            <DropdownMenu.Item variant="destructive">Delete</DropdownMenu.Item>
          </DropdownMenu.Content>
        </DropdownMenu.Root>

        <Tooltip.Root>
          <Tooltip.Trigger class="inline-flex h-9 items-center rounded-md border border-border px-4 text-sm font-medium hover:bg-accent hover:text-accent-foreground">
            Tooltip
          </Tooltip.Trigger>
          <Tooltip.Content>Probed 2 seconds ago</Tooltip.Content>
        </Tooltip.Root>
      </div>
    </section>

    <section class="grid gap-3">
      <h2 class="text-lg font-semibold">Tabs, table, list</h2>
      <Tabs.Root bind:value={tab}>
        <Tabs.List>
          <Tabs.Trigger value="overview"><RouteIcon />Overview</Tabs.Trigger>
          <Tabs.Trigger value="upstreams">Upstreams</Tabs.Trigger>
          <Tabs.Trigger value="settings"><SettingsIcon />Settings</Tabs.Trigger>
        </Tabs.List>
        <Tabs.Content value="overview">
          <Card.Root class="py-4">
            <Card.Content>
              <KeyValueList items={kv} />
            </Card.Content>
          </Card.Root>
        </Tabs.Content>
        <Tabs.Content value="upstreams">
          <Card.Root class="py-0">
            <Card.Content class="px-0">
              <Table.Root>
                <Table.Caption>Upstreams behind <code class="font-mono">api-public</code></Table.Caption>
                <Table.Header>
                  <Table.Row>
                    <Table.Head>Address</Table.Head>
                    <Table.Head>Status</Table.Head>
                    <Table.Head class="text-right">p95</Table.Head>
                    <Table.Head class="text-right">In flight</Table.Head>
                  </Table.Row>
                </Table.Header>
                <Table.Body>
                  {#each [['10.0.0.4:8080', 'healthy', 8], ['10.0.0.5:8080', 'degraded', 240], ['10.0.0.6:8080', 'circuit-open', 0]] as [address, status, p95] (address)}
                    <Table.Row>
                      <Table.Cell class="font-mono text-xs">{address}</Table.Cell>
                      <Table.Cell>
                        <StatusDot status={status as UpstreamStatus} reason={reasons[status as UpstreamStatus]} showLabel />
                      </Table.Cell>
                      <Table.Cell class="text-right tabular-nums">{formatLatency(p95 as number)}</Table.Cell>
                      <Table.Cell class="text-right tabular-nums">12</Table.Cell>
                    </Table.Row>
                  {/each}
                </Table.Body>
              </Table.Root>
            </Card.Content>
          </Card.Root>
        </Tabs.Content>
        <Tabs.Content value="settings">
          <EmptyState
            icon={SettingsIcon}
            title="No overrides"
            description="This route uses the server defaults for timeouts and retries."
          >
            {#snippet action()}
              <Button size="sm"><PlusIcon />Add an override</Button>
            {/snippet}
          </EmptyState>
        </Tabs.Content>
      </Tabs.Root>
    </section>

    <section class="grid gap-3">
      <h2 class="text-lg font-semibold">Loading</h2>
      <div class="grid gap-2 rounded-xl border border-border bg-card p-4" aria-busy="true">
        <Skeleton class="h-4 w-40" />
        <Skeleton class="h-4 w-full" />
        <Skeleton class="h-4 w-2/3" />
      </div>
      <Separator />
      <p class="text-xs text-muted-foreground">
        Skeletons are decoration; the region says <code class="font-mono">aria-busy</code> so the
        state is announced once rather than as a row of empty boxes.
      </p>
    </section>
  </div>
</div>

<Dialog.Root bind:open={dialogOpen}>
  <Dialog.Content>
    <Dialog.Header>
      <Dialog.Title>Reload config</Dialog.Title>
      <Dialog.Description>The proxy applies the change without dropping connections.</Dialog.Description>
    </Dialog.Header>
    <Dialog.Footer>
      <Button variant="outline" onclick={() => (dialogOpen = false)}>Cancel</Button>
      <Button onclick={() => (dialogOpen = false)}>Reload</Button>
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>

<Sheet.Root bind:open={sheetOpen}>
  <Sheet.Content side="right">
    <Sheet.Header>
      <Sheet.Title>Route detail</Sheet.Title>
      <Sheet.Description>Everything about api-public in one panel.</Sheet.Description>
    </Sheet.Header>
    <div class="px-4">
      <KeyValueList items={kv} layout="stacked" />
    </div>
  </Sheet.Content>
</Sheet.Root>

<ConfirmDialog
  bind:open={confirmOpen}
  variant="destructive"
  title="Delete this route?"
  description="Traffic matching it will start falling through to the next route."
  confirmLabel="Delete route"
  onconfirm={() => {
    confirmOpen = false;
    toast.success('Route deleted');
  }}
/>

<Command.Dialog bind:open={paletteOpen}>
  <Command.Input placeholder="Search routes, services, settings..." />
  <Command.List>
    <Command.Empty>Nothing matches that.</Command.Empty>
    <Command.Group heading="Go to">
      <Command.Item onSelect={() => (paletteOpen = false)}>
        <RouteIcon />Routes<Command.Shortcut>G R</Command.Shortcut>
      </Command.Item>
      <Command.Item onSelect={() => (paletteOpen = false)}>
        <HardDriveIcon />Services<Command.Shortcut>G S</Command.Shortcut>
      </Command.Item>
    </Command.Group>
    <Command.Separator />
    <Command.Group heading="Actions">
      <Command.Item onSelect={() => (paletteOpen = false)}><PlusIcon />Add route</Command.Item>
      <Command.Item onSelect={() => (paletteOpen = false)}><ActivityIcon />Check health</Command.Item>
    </Command.Group>
  </Command.List>
</Command.Dialog>
