# SLO Baseline

This document defines the initial service-level objectives for `prx`.

## Objectives

1. Availability SLO: monthly availability >= 99.95%
2. Latency SLO: p95 proxy latency <= 150ms for normal HTTP traffic
3. Error SLO: 5xx ratio <= 1% over a rolling 10 minute window
4. Routing SLO: no-route (`404`) ratio <= 0.5% over a rolling 10 minute window

## Primary SLIs

- `prx_requests_total{status=...}` for request/error-rate math
- `prx_request_latency_ms` for latency percentiles
- `prx_upstream_errors_total{stage=...}` for upstream/connect instability
- `prx_circuit_breaker_open_total` and `prx_upstream_circuit_open` for upstream health pressure

## Error Budget Policy

1. If error budget burn rate is above 2x for > 1 hour, freeze risky deploys
2. If burn rate is above 5x for > 15 minutes, initiate incident and rollback candidate change
3. Resume normal deploy cadence only after burn-rate normalizes and root cause is identified

## Dashboard Minimum

1. Request rate, 4xx ratio, 5xx ratio
2. p50/p95/p99 latency
3. Top failing routes/upstreams
4. Circuit breaker open state by upstream
5. Readiness probe success rate
