<script lang="ts">
  /**
   * Logging and metrics (T308).
   *
   * Small section, one surprise in it: both the log level and the access log
   * are read when prx starts, so changing them here is a restart, not a reload.
   */
  import { Input } from '$lib/components/ui/input';
  import * as Select from '$lib/components/ui/select';
  import { Switch } from '$lib/components/ui/switch';
  import { draftConfig, editDraft } from '$lib/stores/configDraft';
  import SettingRow from './SettingRow.svelte';

  const observability = $derived($draftConfig?.observability ?? null);

  const LEVELS = [
    { id: 'error', hint: 'Only what broke' },
    { id: 'warn', hint: 'Plus what nearly broke' },
    { id: 'info', hint: 'Startup, reloads, certificates' },
    { id: 'debug', hint: 'Per-request decisions; noisy' },
    { id: 'trace', hint: 'Everything; slow' }
  ];

  const set = (path: string, value: unknown) => void editDraft([{ path, value }]);
</script>

{#if observability}
  <div class="max-w-4xl">
    <section class="rounded-2xl border border-border/80 bg-card/80 px-6">
      <SettingRow
        label="Log level"
        description="Applies to prx's own logs. `RUST_LOG` in the environment overrides it."
        path="observability.log_level"
        restart
        id="log-level"
      >
        <Select.Root
          type="single"
          value={observability.log_level}
          onValueChange={(value) => value && set('observability.log_level', value)}
        >
          <Select.Trigger id="log-level" class="w-full sm:w-64">
            {observability.log_level}
          </Select.Trigger>
          <Select.Content>
            {#each LEVELS as level (level.id)}
              <Select.Item value={level.id} label={level.id}>
                {level.id}
                <span class="ml-auto text-xs text-muted-foreground">{level.hint}</span>
              </Select.Item>
            {/each}
          </Select.Content>
        </Select.Root>
      </SettingRow>

      <SettingRow
        label="Access log"
        description="One line per request. Metrics are collected either way — turning this off does not blind the dashboard."
        path="observability.access_log"
        restart
        id="access-log"
      >
        <Switch
          id="access-log"
          checked={observability.access_log}
          onCheckedChange={(checked) => set('observability.access_log', checked)}
        />
      </SettingRow>

      <SettingRow
        label="Prometheus listener"
        description="host:port for /metrics. Blank turns the metrics endpoint off; the dashboard keeps working, since it reads the admin API."
        path="observability.prometheus_listen"
        restart
        id="prometheus-listen"
      >
        <Input
          id="prometheus-listen"
          placeholder="127.0.0.1:9100"
          value={observability.prometheus_listen}
          onchange={(event) => {
            const next = (event.currentTarget as HTMLInputElement).value.trim();
            set('observability.prometheus_listen', next === '' ? null : next);
          }}
        />
      </SettingRow>
    </section>

    <p class="mt-4 rounded-xl border border-border bg-muted/40 px-4 py-3 text-xs text-muted-foreground">
      Tracing (OpenTelemetry) and access-log formats are not in the config yet — they arrive
      with T402 and T403, and this page grows two more fields when they do.
    </p>
  </div>
{:else}
  <p class="text-sm text-muted-foreground">
    The draft has to parse before these fields can be shown. Fix the errors in the TOML tab.
  </p>
{/if}
