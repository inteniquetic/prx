# Rollback Runbook

This runbook is for rolling back `prx` safely during degraded production conditions.

## Rollback Triggers

1. `PrxHigh5xxRatio` is firing and not recovering
2. Readiness (`/readyz`) remains unhealthy after mitigation
3. Circuit-breaker churn indicates broad upstream impact after a deploy

## Preconditions

1. Identify the last known good release tag or artifact digest
2. Confirm config repository/version to pair with that release
3. Confirm on-call owner for rollback execution

## Procedure

1. Stop rollout of current release in deployment controller
2. Deploy last known good `prx` artifact
3. Restore last known good `Prx.toml` revision
4. Verify startup logs contain no config parse/validation errors
5. Validate `/healthz` returns `200`
6. Validate `/readyz` returns `200`
7. Validate request success ratio and p95 latency recover

## Post-Rollback Validation

1. `prx_requests_total{status=~"5.."}` falls back to baseline
2. `prx_upstream_circuit_open` trends down for impacted upstreams
3. No active critical alerts after 15 minutes

## Aftercare

1. Open incident report with timeline and rollback decision point
2. Capture bad release commit/config SHA
3. Block redeploy of the same artifact until root cause is resolved
