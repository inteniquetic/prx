//! T113: active health checking, through the real binary.
//!
//! The point of probing is that an outage is found before a user request finds
//! it, and that a recovered upstream has to prove itself before traffic returns.
//! Both are asserted here against real listeners.

mod common;

use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

use tempfile::TempDir;

use common::{PrxProcess, reserve_port, send_get, write_config};

/// An upstream that can be switched between healthy and unhealthy at runtime.
struct ToggleUpstream {
    shutdown: Arc<AtomicBool>,
    healthy: Arc<AtomicBool>,
    hits: Arc<AtomicUsize>,
    port: u16,
    handle: Option<thread::JoinHandle<()>>,
}

impl ToggleUpstream {
    fn spawn(port: u16, body: &'static str) -> Self {
        let shutdown = Arc::new(AtomicBool::new(false));
        let healthy = Arc::new(AtomicBool::new(true));
        let hits = Arc::new(AtomicUsize::new(0));

        let stop = shutdown.clone();
        let is_healthy = healthy.clone();
        let counter = hits.clone();
        let handle = thread::spawn(move || {
            let listener =
                TcpListener::bind(("127.0.0.1", port)).expect("failed to bind toggle upstream");
            listener
                .set_nonblocking(true)
                .expect("failed to set nonblocking toggle listener");

            while !stop.load(Ordering::Relaxed) {
                match common::accept_blocking(&listener) {
                    Ok((mut stream, _)) => {
                        let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
                        let mut buf = [0u8; 2048];
                        let read = stream.read(&mut buf).unwrap_or(0);
                        let request = String::from_utf8_lossy(&buf[..read]).to_string();
                        let is_probe = request.contains("prx-health");
                        if !is_probe {
                            counter.fetch_add(1, Ordering::Relaxed);
                        }

                        let healthy_now = is_healthy.load(Ordering::Relaxed);
                        let resp = if healthy_now {
                            format!(
                                "HTTP/1.1 200 OK\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                                body.len(),
                                body
                            )
                        } else {
                            "HTTP/1.1 503 Service Unavailable\r\ncontent-length: 0\r\nconnection: close\r\n\r\n".to_string()
                        };
                        let _ = stream.write_all(resp.as_bytes());
                        let _ = stream.flush();
                    }
                    Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(5));
                    }
                    Err(_) => break,
                }
            }
        });

        Self {
            shutdown,
            healthy,
            hits,
            port,
            handle: Some(handle),
        }
    }

    fn set_healthy(&self, healthy: bool) {
        self.healthy.store(healthy, Ordering::Relaxed);
    }

    fn hits(&self) -> usize {
        self.hits.load(Ordering::Relaxed)
    }
}

impl Drop for ToggleUpstream {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Relaxed);
        let _ = TcpStream::connect(("127.0.0.1", self.port));
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

/// Polls until `check` passes or the deadline expires.
fn wait_until(timeout: Duration, mut check: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if check() {
            return true;
        }
        thread::sleep(Duration::from_millis(50));
    }
    false
}

#[test]
fn an_unhealthy_upstream_is_taken_out_before_a_user_request_finds_it() {
    let good_port = reserve_port();
    let bad_port = reserve_port();
    let good = ToggleUpstream::spawn(good_port, "from good");
    let bad = ToggleUpstream::spawn(bad_port, "from bad");

    let proxy_port = reserve_port();
    let tmp = TempDir::new().expect("failed to create temp dir");
    let cfg = format!(
        r#"[server]
listen = ["127.0.0.1:{proxy_port}"]

[observability]
log_level = "error"
access_log = false

[[service]]
name = "app"
max_retries = 0

[service.health_check]
enabled = true
kind = "http"
path = "/healthz"
interval_ms = 100
timeout_ms = 500
healthy_threshold = 1
unhealthy_threshold = 2

[[service.upstream]]
addr = "127.0.0.1:{good_port}"

[[service.upstream]]
addr = "127.0.0.1:{bad_port}"

[[route]]
name = "app"
service = "app"
path_prefix = "/"
is_default = true
"#
    );
    let cfg_path = write_config(&tmp, &cfg);
    let prx = PrxProcess::spawn(&cfg_path, reserve_port());
    prx.wait_until_listening(proxy_port);

    // Both are healthy, so round-robin should reach each of them.
    let mut saw_bad = false;
    for _ in 0..6 {
        if send_get(proxy_port, "app.local", "/").contains("from bad") {
            saw_bad = true;
        }
    }
    assert!(
        saw_bad,
        "the second upstream never received traffic to begin with"
    );

    // Break it and let the prober notice. Round-robin means a single request
    // can miss the broken upstream by luck, so require a run of requests to
    // all land on the healthy one.
    bad.set_healthy(false);
    let taken_out = wait_until(Duration::from_secs(5), || {
        (0..4).all(|_| send_get(proxy_port, "app.local", "/").contains("from good"))
    });
    assert!(
        taken_out,
        "the failing upstream was never removed from rotation"
    );

    // Every request now lands on the healthy one, and none errors out.
    let before_bad = bad.hits();
    for _ in 0..10 {
        let response = send_get(proxy_port, "app.local", "/");
        assert!(
            response.contains("from good"),
            "a user request hit the unhealthy upstream:\n{response}"
        );
    }
    assert_eq!(
        bad.hits(),
        before_bad,
        "the unhealthy upstream still received user traffic"
    );
    assert!(good.hits() > 0);

    // Once it recovers it has to pass a probe before traffic returns.
    bad.set_healthy(true);
    let back = wait_until(Duration::from_secs(5), || {
        send_get(proxy_port, "app.local", "/").contains("from bad")
    });
    assert!(back, "the recovered upstream never came back into rotation");
}

/// With every upstream down, readiness must report it rather than claiming the
/// proxy is fine while it 502s.
#[test]
fn readiness_reflects_the_probe_verdict() {
    let upstream_port = reserve_port();
    let upstream = ToggleUpstream::spawn(upstream_port, "hello");
    let proxy_port = reserve_port();
    let tmp = TempDir::new().expect("failed to create temp dir");

    let cfg = format!(
        r#"[server]
listen = ["127.0.0.1:{proxy_port}"]
health_path = "/healthz"
ready_path = "/readyz"

[observability]
log_level = "error"
access_log = false

[[service]]
name = "app"

[service.health_check]
enabled = true
kind = "http"
path = "/healthz"
interval_ms = 100
timeout_ms = 500
healthy_threshold = 1
unhealthy_threshold = 2

[[service.upstream]]
addr = "127.0.0.1:{upstream_port}"

[[route]]
name = "app"
service = "app"
path_prefix = "/"
is_default = true
"#
    );
    let cfg_path = write_config(&tmp, &cfg);
    let prx = PrxProcess::spawn(&cfg_path, reserve_port());
    prx.wait_until_listening(proxy_port);

    assert!(
        send_get(proxy_port, "app.local", "/readyz").starts_with("HTTP/1.1 200"),
        "prx should be ready while its only upstream is healthy"
    );

    upstream.set_healthy(false);
    let not_ready = wait_until(Duration::from_secs(5), || {
        send_get(proxy_port, "app.local", "/readyz").starts_with("HTTP/1.1 503")
    });
    assert!(
        not_ready,
        "readiness kept reporting 200 with every upstream down"
    );

    upstream.set_healthy(true);
    let ready_again = wait_until(Duration::from_secs(5), || {
        send_get(proxy_port, "app.local", "/readyz").starts_with("HTTP/1.1 200")
    });
    assert!(ready_again, "readiness never recovered");
}
