# Caddy with the Coraza WAF module. The same binary serves both the `caddy`
# (WAF off) and `caddy-coraza` targets, so the twins differ only by Caddyfile.
FROM caddy:2-builder AS build
ARG CORAZA_CADDY
RUN xcaddy build --with github.com/corazawaf/coraza-caddy/v2@${CORAZA_CADDY}

FROM caddy:2
COPY --from=build /usr/bin/caddy /usr/bin/caddy
