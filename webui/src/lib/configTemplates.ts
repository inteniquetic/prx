/**
 * Configs to start from (T309).
 *
 * An empty proxy is the hardest screen in the product: nothing to click, and a
 * TOML schema to learn before anything happens. These are the three shapes
 * `docs/CONFIG-PLAYBOOK.md` opens with, written in the schema prx reads today,
 * comments and all — they land in the draft like any other change, so the first
 * thing a new operator sees is still a diff and an Apply.
 */

export type TemplateId = 'single' | 'blue-green' | 'gateway';

export interface ConfigTemplate {
  id: TemplateId;
  /** i18n keys; the copy itself lives in the dictionaries. */
  titleKey: string;
  descriptionKey: string;
  /** What it leaves behind, for the summary line. */
  services: number;
  routes: number;
  toml(): string;
}

const SERVER = `[server]
listen = ["0.0.0.0:8080"]
health_path = "/healthz"
ready_path = "/readyz"

[observability]
log_level = "info"
access_log = true
`;

export const TEMPLATES: ConfigTemplate[] = [
  {
    id: 'single',
    titleKey: 'template.single.title',
    descriptionKey: 'template.single.body',
    services: 1,
    routes: 1,
    toml: () => `# One backend behind the proxy.
${SERVER}
[[service]]
name = "app"
lb = "round_robin"

[[service.upstream]]
addr = "127.0.0.1:3000"

# Everything goes here: is_default catches whatever no other route matches.
[[route]]
name = "app"
service = "app"
path_prefix = "/"
is_default = true
`
  },
  {
    id: 'blue-green',
    titleKey: 'template.blueGreen.title',
    descriptionKey: 'template.blueGreen.body',
    services: 1,
    routes: 1,
    toml: () => `# Two versions behind one route. Shift traffic by moving the weights:
# 9/1 is a 10% canary, 1/1 is an even split, and draining the old one with
# \`enabled = false\` retires it without deleting anything.
${SERVER}
[[service]]
name = "app"
lb = "round_robin"

[[service.upstream]]
addr = "127.0.0.1:3000"
weight = 9

[[service.upstream]]
addr = "127.0.0.1:3001"
weight = 1

[[route]]
name = "app"
service = "app"
path_prefix = "/"
is_default = true
`
  },
  {
    id: 'gateway',
    titleKey: 'template.gateway.title',
    descriptionKey: 'template.gateway.body',
    services: 2,
    routes: 3,
    toml: () => `# One host, several services, split by path. The longest matching prefix
# wins, so /api/v2 beats /api and / catches the rest.
${SERVER}
[[service]]
name = "api-v1"
lb = "round_robin"

[[service.upstream]]
addr = "127.0.0.1:3001"

[[service]]
name = "api-v2"
lb = "round_robin"

[[service.upstream]]
addr = "127.0.0.1:3002"

[[route]]
name = "api-v2"
service = "api-v2"
path_prefix = "/api/v2"

[[route]]
name = "api-v1"
service = "api-v1"
path_prefix = "/api"

# Anything that is not the API: send it to v1 rather than 404.
[[route]]
name = "fallback"
service = "api-v1"
path_prefix = "/"
is_default = true
`
  }
];

export interface WizardAnswers {
  serviceName: string;
  /** One or more `host:port`. Blank ones are dropped. */
  upstreams: string[];
  /** Empty means the route matches any host. */
  host: string;
  pathPrefix: string;
}

const escape = (value: string): string => value.replaceAll('\\', '\\\\').replaceAll('"', '\\"');

/** The config the three questions in the wizard add up to. */
export function wizardToml(answers: WizardAnswers): string {
  const name = answers.serviceName.trim() || 'app';
  const upstreams = answers.upstreams.map((addr) => addr.trim()).filter(Boolean);
  const path = answers.pathPrefix.trim() || '/';
  const host = answers.host.trim();

  const upstreamBlocks = (upstreams.length > 0 ? upstreams : ['127.0.0.1:3000'])
    .map((addr) => `[[service.upstream]]\naddr = "${escape(addr)}"\n`)
    .join('\n');

  return `# Written by the setup wizard. Everything in it can be changed from the
# other pages, or here.
${SERVER}
[[service]]
name = "${escape(name)}"
lb = "round_robin"

${upstreamBlocks}
[[route]]
name = "${escape(name)}"
service = "${escape(name)}"
${host ? `host = "${escape(host)}"\n` : ''}path_prefix = "${escape(path)}"
is_default = ${path === '/' && !host ? 'true' : 'false'}
`;
}

/** `host:port`, near enough to catch a typed mistake before it is applied. */
export function looksLikeAddress(value: string): boolean {
  return /^[A-Za-z0-9._-]+:\d{1,5}$/.test(value.trim());
}

/** True for a config nobody has set up yet: no services to send traffic to. */
export function isUnconfigured(config: { services: unknown[] } | null): boolean {
  return !config || config.services.length === 0;
}
