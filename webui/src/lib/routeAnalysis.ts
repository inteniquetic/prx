import type { RouteConfig } from './types/config';

/**
 * What the route table needs to say about a config beyond the fields
 * themselves: which rule each route wins on, and which routes can never win at
 * all.
 *
 * The rules mirrored here are the ones `src/router.rs` documents and enforces:
 *
 * 1. exact host beats wildcard host beats no host;
 * 2. among wildcards, the longest suffix wins;
 * 3. inside one host, the longest path prefix wins;
 * 4. a tie on host and prefix is settled by config order — first wins;
 * 5. methods filter the route that already won the path; a rejected method is
 *    a 405, not a fall-through;
 * 6. `is_default` is the fallback, and only the first one counts;
 * 7. a disabled route is not in the index at all.
 *
 * This is a second implementation of those rules, which is a thing to be
 * careful with: the tester (`POST /web/routes/test`) asks the real matcher, so
 * anything a user acts on comes from the proxy itself. What lives here is the
 * static reading of the table — ordering and warnings — which the server has no
 * endpoint for.
 */

export type HostTier = 'exact' | 'wildcard' | 'any';

export type RouteWarning =
  | {
      kind: 'unreachable';
      /** The route that always takes these requests first. */
      by: string;
      message: string;
    }
  | { kind: 'partly-shadowed'; by: string; message: string }
  | { kind: 'ignored-default'; by: string; message: string }
  | { kind: 'disabled'; message: string };

export interface RouteInsight {
  index: number;
  route: RouteConfig;
  tier: HostTier;
  /** Sort key: higher wins. Mirrors the tiers above, not a server value. */
  precedence: number;
  warnings: RouteWarning[];
  /** Everything the table searches, lowercased once instead of per keystroke. */
  haystack: string;
}

const normalizeHost = (host: string): string => {
  const trimmed = host.trim().toLowerCase();
  if (!trimmed) return '';
  // The proxy strips the port before matching, so a host written with one here
  // would never match anything.
  const withoutPort = trimmed.replace(/:\d+$/, '');
  return withoutPort;
};

export const hostTier = (host: string): HostTier => {
  const normalized = normalizeHost(host);
  if (!normalized) return 'any';
  return normalized.startsWith('*.') ? 'wildcard' : 'exact';
};

/**
 * A number that orders routes the way the index does. Only comparable against
 * other routes in the same config.
 */
const precedenceOf = (route: RouteConfig): number => {
  const host = normalizeHost(route.host);
  const tier = hostTier(route.host);
  const tierWeight = tier === 'exact' ? 3_000_000 : tier === 'wildcard' ? 2_000_000 : 1_000_000;
  // Among wildcards the longer suffix wins; among paths the longer prefix does.
  const hostWeight = tier === 'wildcard' ? host.length * 1_000 : 0;
  const pathWeight = (route.path_prefix || '/').length;
  return tierWeight + hostWeight + pathWeight;
};

/** Do the methods of `earlier` leave anything for `later` to answer? */
const coversMethods = (earlier: RouteConfig, later: RouteConfig): 'all' | 'some' | 'none' => {
  if (earlier.methods.length === 0) return 'all';
  if (later.methods.length === 0) return 'some';

  const covered = new Set(earlier.methods.map((method) => method.toUpperCase()));
  const remaining = later.methods.filter((method) => !covered.has(method.toUpperCase()));
  if (remaining.length === 0) return 'all';
  return remaining.length === later.methods.length ? 'none' : 'some';
};

/**
 * Reads the whole route list once and returns per-route insight.
 *
 * O(n) plus one pass over the routes that share a bucket, which is what keeps
 * the table responsive with hundreds of routes.
 */
export function analyzeRoutes(routes: RouteConfig[]): RouteInsight[] {
  // Routes that share a host bucket and an exact path prefix are the only ones
  // that can shadow each other: a longer prefix always wins for its own paths,
  // and a different host never competes.
  const buckets = new Map<string, number[]>();
  let firstDefault = -1;

  routes.forEach((route, index) => {
    if (!route.enabled) return;
    if (route.is_default && firstDefault === -1) firstDefault = index;
    const key = `${normalizeHost(route.host)}\u0000${route.path_prefix || '/'}`;
    const bucket = buckets.get(key);
    if (bucket) bucket.push(index);
    else buckets.set(key, [index]);
  });

  return routes.map((route, index) => {
    const warnings: RouteWarning[] = [];

    if (!route.enabled) {
      warnings.push({
        kind: 'disabled',
        message: 'Disabled: this route is in the config but takes no traffic.'
      });
    } else {
      const key = `${normalizeHost(route.host)}\u0000${route.path_prefix || '/'}`;
      const bucket = buckets.get(key) ?? [];

      for (const other of bucket) {
        if (other >= index) break;
        const earlier = routes[other];
        const coverage = coversMethods(earlier, route);
        if (coverage === 'all') {
          warnings.push({
            kind: 'unreachable',
            by: earlier.name,
            message: `Never matches: “${earlier.name}” is above it with the same host and path prefix, and accepts the same methods.`
          });
          break;
        }
        if (coverage === 'some') {
          warnings.push({
            kind: 'partly-shadowed',
            by: earlier.name,
            message: `Partly shadowed: “${earlier.name}” is above it with the same host and path prefix and answers some of these methods first.`
          });
        }
      }

      if (route.is_default && firstDefault !== -1 && firstDefault !== index) {
        warnings.push({
          kind: 'ignored-default',
          by: routes[firstDefault].name,
          message: `Not the fallback: “${routes[firstDefault].name}” is marked default above it, and only the first one counts.`
        });
      }
    }

    return {
      index,
      route,
      tier: hostTier(route.host),
      precedence: precedenceOf(route),
      warnings,
      haystack:
        `${route.name} ${route.host} ${route.path_prefix} ${route.service}`.toLowerCase()
    };
  });
}

/** The order the matcher would consider these routes in, most specific first. */
export function matchOrder(insights: RouteInsight[]): RouteInsight[] {
  return [...insights]
    .filter((insight) => insight.route.enabled)
    .sort((a, b) => b.precedence - a.precedence || a.index - b.index);
}

export const tierLabel: Record<HostTier, string> = {
  exact: 'Exact host',
  wildcard: 'Wildcard host',
  any: 'Any host'
};
