//! Active health checking (T113).
//!
//! The passive circuit breaker only reacts after real requests have failed, so
//! users absorb the first errors of every outage and an upstream is let back in
//! purely because a timer expired. This module probes upstreams in the
//! background: a failing one is taken out before a request finds it, and a
//! recovering one has to pass probes before it gets traffic again.
//!
//! One task drives every service. It reads the current config snapshot on each
//! tick, so a reload is picked up without stopping or spawning anything, and
//! there are no per-upstream tasks to leak.

use std::{
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use arc_swap::ArcSwap;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
    time::timeout,
};
use tracing::{info, warn};

use crate::{
    config::{HealthCheckConfig, HealthCheckKind},
    metrics,
    runtime::RuntimeConfig,
};

/// How often the checker wakes up to see which probes are due. Individual
/// services still probe at their own `interval_ms`; this only bounds how
/// precisely that interval is honored.
const TICK: Duration = Duration::from_millis(200);

/// Starts the background checker on its own thread and runtime, matching how
/// the config watcher is run: pingora owns the main runtime, and a probe must
/// never compete with request handling for it.
pub fn spawn_health_checker(active_config: Arc<ArcSwap<RuntimeConfig>>) -> anyhow::Result<()> {
    std::thread::Builder::new()
        .name("prx-health-checker".to_string())
        .spawn(move || {
            let runtime = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(runtime) => runtime,
                Err(err) => {
                    warn!(error = %err, "failed to start the health checker runtime");
                    return;
                }
            };
            runtime.block_on(run(active_config));
        })?;
    Ok(())
}

async fn run(active_config: Arc<ArcSwap<RuntimeConfig>>) {
    let mut ticker = tokio::time::interval(TICK);
    loop {
        ticker.tick().await;
        let snapshot = active_config.load_full();
        probe_due_upstreams(&snapshot).await;
    }
}

/// Probes everything whose interval has elapsed, all in parallel so one slow
/// upstream cannot delay the others.
async fn probe_due_upstreams(snapshot: &Arc<RuntimeConfig>) {
    let now = now_epoch_ms();
    let mut probes = Vec::new();

    for service_idx in 0..snapshot.service_count() {
        let Some(service) = snapshot.service(service_idx) else {
            continue;
        };
        let config = service.health_check.clone();
        if !config.enabled {
            continue;
        }

        for upstream_idx in 0..service.upstreams.len() {
            let Some(upstream) = service.upstreams.get(upstream_idx) else {
                continue;
            };
            let last = upstream.last_probe_ms();
            if last != 0 && now.saturating_sub(last) < config.interval_ms {
                continue;
            }

            let snapshot = snapshot.clone();
            let config = config.clone();
            probes.push(tokio::spawn(async move {
                let Some(service) = snapshot.service(service_idx) else {
                    return;
                };
                let Some(upstream) = service.upstreams.get(upstream_idx) else {
                    return;
                };

                let healthy = probe(&upstream.addr, &config).await;
                metrics::inc_health_check(
                    service.name.as_str(),
                    upstream.addr.as_str(),
                    if healthy { "success" } else { "failure" },
                );

                if let Some(now_healthy) = upstream.record_probe(
                    healthy,
                    config.healthy_threshold,
                    config.unhealthy_threshold,
                ) {
                    metrics::set_upstream_healthy(
                        service.name.as_str(),
                        upstream.addr.as_str(),
                        now_healthy,
                    );
                    if now_healthy {
                        info!(
                            service = service.name.as_str(),
                            upstream = upstream.addr.as_str(),
                            "upstream passed its health checks and is back in rotation"
                        );
                    } else {
                        warn!(
                            service = service.name.as_str(),
                            upstream = upstream.addr.as_str(),
                            "upstream failed its health checks and was taken out of rotation"
                        );
                    }
                }
            }));
        }
    }

    for probe in probes {
        let _ = probe.await;
    }
}

/// Runs one probe. Any error, including a timeout, counts as a failure.
pub async fn probe(addr: &str, config: &HealthCheckConfig) -> bool {
    let deadline = Duration::from_millis(config.timeout_ms);
    match config.kind {
        HealthCheckKind::Tcp => {
            matches!(timeout(deadline, TcpStream::connect(addr)).await, Ok(Ok(_)))
        }
        HealthCheckKind::Http => {
            matches!(timeout(deadline, http_probe(addr, config)).await, Ok(true))
        }
    }
}

/// A deliberately small HTTP/1.1 GET: a probe must not pull in a client stack
/// or keep connections around.
async fn http_probe(addr: &str, config: &HealthCheckConfig) -> bool {
    let Ok(mut stream) = TcpStream::connect(addr).await else {
        return false;
    };
    let request = format!(
        "GET {} HTTP/1.1\r\nHost: {}\r\nUser-Agent: prx-health\r\nConnection: close\r\n\r\n",
        config.path, addr
    );
    if stream.write_all(request.as_bytes()).await.is_err() {
        return false;
    }

    // The status line is all that matters, so read just enough for it.
    let mut buf = [0u8; 64];
    let mut filled = 0;
    while filled < buf.len() {
        match stream.read(&mut buf[filled..]).await {
            Ok(0) => break,
            Ok(n) => {
                filled += n;
                if buf[..filled].contains(&b'\r') {
                    break;
                }
            }
            Err(_) => return false,
        }
    }

    match parse_status(&buf[..filled]) {
        Some(status) => config.expected_status.contains(&status),
        None => false,
    }
}

/// Extracts the status code from an HTTP/1.x status line.
fn parse_status(bytes: &[u8]) -> Option<u16> {
    let line = std::str::from_utf8(bytes).ok()?;
    let mut parts = line.split(' ');
    let version = parts.next()?;
    if !version.starts_with("HTTP/") {
        return None;
    }
    parts.next()?.parse().ok()
}

fn now_epoch_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_status_line() {
        assert_eq!(parse_status(b"HTTP/1.1 200 OK\r\n"), Some(200));
        assert_eq!(
            parse_status(b"HTTP/1.0 503 Service Unavailable\r"),
            Some(503)
        );
        assert_eq!(parse_status(b"garbage"), None);
        assert_eq!(parse_status(b""), None);
    }

    #[tokio::test]
    async fn a_tcp_probe_fails_when_nothing_is_listening() {
        // Port 1 on loopback is never in use in this environment.
        let config = HealthCheckConfig {
            enabled: true,
            timeout_ms: 300,
            ..Default::default()
        };
        assert!(!probe("127.0.0.1:1", &config).await);
    }

    #[tokio::test]
    async fn a_tcp_probe_succeeds_against_a_listener() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("failed to bind test listener");
        let addr = listener.local_addr().expect("failed to read local addr");
        tokio::spawn(async move {
            loop {
                if listener.accept().await.is_err() {
                    return;
                }
            }
        });

        let config = HealthCheckConfig {
            enabled: true,
            timeout_ms: 500,
            ..Default::default()
        };
        assert!(probe(&addr.to_string(), &config).await);
    }

    #[tokio::test]
    async fn an_http_probe_checks_the_status_code() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("failed to bind test listener");
        let addr = listener.local_addr().expect("failed to read local addr");
        tokio::spawn(async move {
            while let Ok((mut stream, _)) = listener.accept().await {
                tokio::spawn(async move {
                    let mut buf = [0u8; 1024];
                    let _ = stream.read(&mut buf).await;
                    let _ = stream
                        .write_all(b"HTTP/1.1 503 Service Unavailable\r\ncontent-length: 0\r\n\r\n")
                        .await;
                });
            }
        });

        let mut config = HealthCheckConfig {
            enabled: true,
            kind: HealthCheckKind::Http,
            timeout_ms: 500,
            ..Default::default()
        };
        assert!(
            !probe(&addr.to_string(), &config).await,
            "503 must not count as healthy when only 200 is expected"
        );

        config.expected_status = vec![200, 503];
        assert!(
            probe(&addr.to_string(), &config).await,
            "503 should pass once it is listed as expected"
        );
    }
}
