# Failure Drills

Use these drills before production rollout and periodically afterward.

## Drill 1: Upstream Failover

1. Configure route with at least 2 upstreams and `max_retries >= 1`
2. Stop one upstream instance
3. Send traffic to the route
4. Verify requests still succeed via alternate upstream
5. Confirm `prx_upstream_errors_total` increases for the failed upstream

## Drill 2: Circuit Breaker Trip

1. Enable route circuit breaker in `Prx.toml`
2. Set low threshold temporarily:
   `consecutive_failures = 1`
   `open_ms = 60000`
3. Force repeated upstream connect/proxy failures
4. Verify `prx_circuit_breaker_open_total` increments
5. Verify `prx_upstream_circuit_open` becomes `1` for affected upstream

## Drill 3: Load Smoke

1. Start `prx` with production-like config
2. Run:
   `REQUESTS=2000 CONCURRENCY=32 scripts/load-smoke.sh http://127.0.0.1:8080/healthz`
3. Check p95 latency and 5xx ratio remain within SLO guardrails
