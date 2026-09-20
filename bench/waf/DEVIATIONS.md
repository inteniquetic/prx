# Per-engine deviations from `main.conf`

Every WAF target loads the same `main.conf` and the same CRS checkout. A row
here is a place where one engine could not, and what was done instead. A
deviation that is not in this table invalidates the run it happened in.

| Target | Directive | What differs | Why |
|---|---|---|---|

## Proxy configuration

Not SecLang, but the same rule applies: a difference between targets that is not
written down here invalidates the comparison it touches.

| Targets | What differs | Effect | Why it stays |
|---|---|---|---|
| nginx, nginx-modsec vs the rest | `listen ... reuseport`: one accept queue per worker | Helps every nginx absolute number. Present in both nginx twins, so it cancels out of the WAF tax | It is how nginx is deployed; removing it would benchmark a configuration nobody runs |
| nginx twins vs caddy twins | nginx `proxy_read_timeout 5s` bounds the gap between upstream reads; Caddy `response_header_timeout 5s` bounds only the wait for headers | A worker stalled inside the WAF can turn into a 504 on nginx and cannot on Caddy | No equivalent control exists on both sides. Read the nginx 504s on `waf-json-128k` as "the worker was stalled for > 5 s", not as a difference in WAF correctness |
| nginx twins vs caddy / prx twins | nginx buffers the request body with the WAF on and off. Caddy and prx stream it with the WAF off and buffer it only with the WAF on | The Coraza (and later prx) tax on `waf-form` / `waf-json-*` includes body buffering; the ModSecurity tax does not | Buffering is part of what turning a WAF on costs on those proxies. It is microseconds next to the milliseconds-to-seconds of rule evaluation on the same rows |
| prx vs nginx, caddy | prx is built with cargo's default release profile (no LTO, 16 codegen units); nginx and Caddy are vendor-optimised builds | Understates prx by an unknown, probably single-digit, percentage | Changing the release profile changes the production binary; decide that on its own, with numbers |

