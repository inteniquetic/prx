/**
 * The number formats the UI agrees on.
 *
 * Every screen shows the same quantity the same way: latency in milliseconds,
 * throughput in requests per second, sizes in IEC units. Anything that formats
 * a number for display goes through here rather than calling toFixed() inline,
 * so a change of mind is one edit instead of a grep.
 */

const nf = (min: number, max: number) =>
  new Intl.NumberFormat('en-US', { minimumFractionDigits: min, maximumFractionDigits: max });

/**
 * Latency, always in ms so two numbers can be compared at a glance.
 *
 * Precision shrinks as the number grows: sub-millisecond values need decimals
 * to say anything at all, a 1,240 ms value does not.
 */
export function formatLatency(ms: number | null | undefined): string {
  if (ms === null || ms === undefined || !Number.isFinite(ms)) return '—';
  if (ms < 1) return `${nf(0, 2).format(ms)} ms`;
  if (ms < 10) return `${nf(0, 1).format(ms)} ms`;
  return `${nf(0, 0).format(Math.round(ms))} ms`;
}

/** Throughput in requests per second, thinned out with k/M above a thousand. */
export function formatThroughput(reqPerSec: number | null | undefined): string {
  if (reqPerSec === null || reqPerSec === undefined || !Number.isFinite(reqPerSec)) return '—';
  if (reqPerSec < 1000) return `${nf(0, reqPerSec < 10 ? 1 : 0).format(reqPerSec)} req/s`;
  if (reqPerSec < 1_000_000) return `${nf(0, 1).format(reqPerSec / 1000)}k req/s`;
  return `${nf(0, 1).format(reqPerSec / 1_000_000)}M req/s`;
}

const IEC_UNITS = ['B', 'KiB', 'MiB', 'GiB', 'TiB', 'PiB'] as const;

/**
 * Bytes in IEC units (KiB = 1024 B), because that is what the proxy counts.
 * Mixing SI and IEC is how a 1000-vs-1024 bug reaches production.
 */
export function formatBytes(bytes: number | null | undefined): string {
  if (bytes === null || bytes === undefined || !Number.isFinite(bytes)) return '—';
  const sign = bytes < 0 ? '-' : '';
  let value = Math.abs(bytes);
  let unit = 0;
  while (value >= 1024 && unit < IEC_UNITS.length - 1) {
    value /= 1024;
    unit += 1;
  }
  const digits = unit === 0 ? 0 : value < 10 ? 1 : 0;
  return `${sign}${nf(0, digits).format(value)} ${IEC_UNITS[unit]}`;
}

/** Plain counts: grouped thousands, no unit. */
export function formatCount(value: number | null | undefined): string {
  if (value === null || value === undefined || !Number.isFinite(value)) return '—';
  return nf(0, 0).format(value);
}

/** A ratio in 0..1 as a percentage. */
export function formatPercent(ratio: number | null | undefined, digits = 1): string {
  if (ratio === null || ratio === undefined || !Number.isFinite(ratio)) return '—';
  return `${nf(0, digits).format(ratio * 100)}%`;
}

/** A signed change, for deltas next to a metric. */
export function formatDelta(value: number | null | undefined, digits = 1): string {
  if (value === null || value === undefined || !Number.isFinite(value)) return '—';
  const sign = value > 0 ? '+' : '';
  return `${sign}${nf(0, digits).format(value)}`;
}
