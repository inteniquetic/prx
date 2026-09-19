//! T207: the live-stats API, through the real binary.
//!
//! The dashboard is only worth building if these hold: the numbers it shows are
//! the ones `/metrics` serves, the stream pushes without being asked, a wall of
//! dashboards cannot drown the control plane, and a closed tab gives its slot
//! back.

mod common;

use std::{
    io::{Read, Write},
    net::TcpStream,
    thread,
    time::{Duration, Instant},
};

use tempfile::TempDir;

use common::{PrxProcess, UpstreamServer, reserve_port, send_get, write_config};

/// The stats API is on the admin listener, so every read here is a plain GET
/// against it.
fn admin_get(port: u16, path: &str) -> String {
    send_get(port, "127.0.0.1", path)
}

fn body_of(response: &str) -> &str {
    response
        .split_once("\r\n\r\n")
        .map(|(_, body)| body)
        .unwrap_or("")
}

fn json_number(body: &str, key: &str) -> Option<f64> {
    let needle = format!("\"{key}\":");
    let start = body.find(&needle)? + needle.len();
    let rest = &body[start..];
    let end = rest
        .find(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-' || c == 'e'))
        .unwrap_or(rest.len());
    rest[..end].parse().ok()
}

/// Opens a stream and holds it open, the way a browser tab does.
struct StreamClient {
    stream: TcpStream,
    status: String,
}

impl StreamClient {
    fn open(port: u16) -> Self {
        let mut stream =
            TcpStream::connect(("127.0.0.1", port)).expect("failed to connect to the admin API");
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .expect("failed to set read timeout");
        stream
            .write_all(
                b"GET /web/stats/stream HTTP/1.1\r\nHost: 127.0.0.1\r\nAccept: text/event-stream\r\n\r\n",
            )
            .expect("failed to write the stream request");
        stream.flush().expect("failed to flush the stream request");

        let mut status = String::new();
        let mut byte = [0u8; 1];
        // Read just the status line; the body is a stream that never ends.
        while !status.ends_with("\r\n") {
            if stream.read(&mut byte).unwrap_or(0) == 0 {
                break;
            }
            status.push(byte[0] as char);
        }

        Self { stream, status }
    }

    fn ok(&self) -> bool {
        self.status.starts_with("HTTP/1.1 200")
    }

    /// Reads until the named SSE event shows up, or gives up.
    fn wait_for_event(&mut self, name: &str, timeout: Duration) -> Option<String> {
        let deadline = Instant::now() + timeout;
        let mut buffer = String::new();
        let mut chunk = [0u8; 4096];
        while Instant::now() < deadline {
            let read = match self.stream.read(&mut chunk) {
                Ok(0) => return None,
                Ok(read) => read,
                Err(_) => return None,
            };
            buffer.push_str(&String::from_utf8_lossy(&chunk[..read]));
            if let Some(at) = buffer.find(&format!("event: {name}\n")) {
                // Back up to the start of the frame so the `id:` line, which
                // axum writes first, comes along with it.
                let start = buffer[..at].rfind("\n\n").map(|at| at + 2).unwrap_or(0);
                let rest = &buffer[start..];
                if let Some(end) = rest.find("\n\n") {
                    return Some(rest[..end].to_string());
                }
            }
        }
        None
    }
}

fn stats_config(proxy_port: u16, upstream_port: u16, metrics_port: u16) -> String {
    format!(
        r#"[server]
listen = ["127.0.0.1:{proxy_port}"]
health_path = "/healthz"
ready_path = "/readyz"

[observability]
log_level = "error"
access_log = false
prometheus_listen = "127.0.0.1:{metrics_port}"

[[service]]
name = "app"
max_retries = 0

[[service.upstream]]
addr = "127.0.0.1:{upstream_port}"

[[route]]
name = "app-route"
service = "app"
path_prefix = "/"
is_default = true
"#
    )
}

/// Waits until the sampler has produced at least `seq` samples.
fn wait_for_samples(admin_port: u16, seq: u64) -> String {
    let deadline = Instant::now() + Duration::from_secs(15);
    let mut last = String::new();
    while Instant::now() < deadline {
        last = body_of(&admin_get(admin_port, "/web/stats")).to_string();
        if json_number(&last, "seq").unwrap_or(0.0) >= seq as f64 {
            return last;
        }
        thread::sleep(Duration::from_millis(200));
    }
    panic!("the sampler never reached seq {seq}; last payload was: {last}");
}

#[test]
fn live_stats_report_the_same_traffic_metrics_does() {
    let dir = TempDir::new().expect("failed to create temp dir");
    let proxy_port = reserve_port();
    let upstream_port = reserve_port();
    let admin_port = reserve_port();
    let metrics_port = reserve_port();

    let _upstream = UpstreamServer::spawn(upstream_port, "hello");
    let config = write_config(&dir, &stats_config(proxy_port, upstream_port, metrics_port));
    let prx = PrxProcess::spawn(&config, admin_port);
    prx.wait_until_listening(proxy_port);
    prx.wait_until_listening(admin_port);

    // Two samples so at least one covers real traffic.
    wait_for_samples(admin_port, 1);
    let sent = 12;
    for _ in 0..sent {
        let response = send_get(proxy_port, "app.example.com", "/things");
        assert!(response.starts_with("HTTP/1.1 200"), "proxy did not answer");
    }

    let payload = wait_for_samples(admin_port, 8);

    assert!(
        payload.contains("\"app-route\""),
        "the route leaderboard should name the route that served the traffic: {payload}"
    );
    let requests = json_number(
        payload.split("\"app-route\"").nth(1).expect("route entry"),
        "requests",
    )
    .expect("the route entry should carry a request count");
    assert_eq!(
        requests as u64, sent,
        "the windowed route count should match what was sent: {payload}"
    );

    // The same traffic, straight from the registry `/metrics` serves.
    let scrape_response = send_get(metrics_port, "127.0.0.1", "/metrics");
    let scrape = body_of(&scrape_response);
    let metrics_total: u64 = scrape
        .lines()
        .filter(|line| line.starts_with("prx_requests_total{") && line.contains("app-route"))
        .filter_map(|line| line.rsplit(' ').next()?.parse::<f64>().ok())
        .sum::<f64>() as u64;
    assert_eq!(
        metrics_total, sent,
        "/metrics and /web/stats must count the same requests:\n{scrape}"
    );

    // Percentiles need real buckets; the default ones stopped at 10 ms.
    assert!(
        payload.contains("\"p99_ms\":"),
        "the sample should carry percentiles: {payload}"
    );
}

#[test]
fn the_stream_pushes_samples_and_caps_its_clients() {
    let dir = TempDir::new().expect("failed to create temp dir");
    let proxy_port = reserve_port();
    let upstream_port = reserve_port();
    let admin_port = reserve_port();
    let metrics_port = reserve_port();

    let _upstream = UpstreamServer::spawn(upstream_port, "hello");
    let config = write_config(&dir, &stats_config(proxy_port, upstream_port, metrics_port));
    let prx = PrxProcess::spawn(&config, admin_port);
    prx.wait_until_listening(proxy_port);
    prx.wait_until_listening(admin_port);
    wait_for_samples(admin_port, 1);

    let mut client = StreamClient::open(admin_port);
    assert!(client.ok(), "the stream should open: {}", client.status);
    assert!(
        client
            .wait_for_event("hello", Duration::from_secs(5))
            .is_some_and(|event| event.contains("retry:")),
        "the stream should send a retry hint so the browser reconnects on its own"
    );
    let tick = client
        .wait_for_event("tick", Duration::from_secs(5))
        .expect("the stream should push a sample without being asked");
    assert!(tick.contains("\"rps\""), "a tick carries a sample: {tick}");
    assert!(tick.contains("id: "), "a tick is numbered: {tick}");

    // Fill the remaining slots, then prove the next one is refused rather than
    // quietly making the control plane fan out to everybody.
    let mut clients: Vec<StreamClient> = (1..16).map(|_| StreamClient::open(admin_port)).collect();
    assert!(
        clients.iter().all(StreamClient::ok),
        "the first 16 streams should all be accepted"
    );

    let refused = StreamClient::open(admin_port);
    assert!(
        refused.status.starts_with("HTTP/1.1 503"),
        "the 17th stream should be refused, got: {}",
        refused.status
    );

    let busy_response = admin_get(admin_port, "/web/stats");
    let busy = body_of(&busy_response);
    assert_eq!(
        json_number(busy, "stream_clients").map(|value| value as u64),
        Some(16),
        "the server should know how many streams are open: {busy}"
    );

    // Closing the tabs has to give the slots back — a leak here would make the
    // cap a one-way door.
    drop(client);
    drop(refused);
    clients.clear();

    let deadline = Instant::now() + Duration::from_secs(10);
    let mut remaining = 16;
    while Instant::now() < deadline {
        let payload = body_of(&admin_get(admin_port, "/web/stats")).to_string();
        remaining = json_number(&payload, "stream_clients").unwrap_or(-1.0) as i64;
        if remaining == 0 {
            break;
        }
        thread::sleep(Duration::from_millis(200));
    }
    assert_eq!(
        remaining, 0,
        "closed streams must release their slot and leave nothing running"
    );

    // And the API still works afterwards.
    let reopened = StreamClient::open(admin_port);
    assert!(
        reopened.ok(),
        "a stream should open again once the slots are free: {}",
        reopened.status
    );
}

#[test]
fn applying_a_config_lands_on_the_event_strip() {
    let dir = TempDir::new().expect("failed to create temp dir");
    let proxy_port = reserve_port();
    let upstream_port = reserve_port();
    let admin_port = reserve_port();
    let metrics_port = reserve_port();

    let _upstream = UpstreamServer::spawn(upstream_port, "hello");
    let toml = stats_config(proxy_port, upstream_port, metrics_port);
    let config = write_config(&dir, &toml);
    let prx = PrxProcess::spawn(&config, admin_port);
    prx.wait_until_listening(proxy_port);
    prx.wait_until_listening(admin_port);

    let mut stream = TcpStream::connect(("127.0.0.1", admin_port)).expect("failed to connect");
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("failed to set read timeout");
    let request = format!(
        "PUT /web/config HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        toml.len(),
        toml
    );
    stream
        .write_all(request.as_bytes())
        .expect("failed to write the config");
    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .expect("failed to read the apply response");
    assert!(
        response.starts_with("HTTP/1.1 200"),
        "the config should apply: {response}"
    );

    let payload = body_of(&admin_get(admin_port, "/web/stats")).to_string();
    assert!(
        payload.contains("config_apply") && payload.contains("Config applied"),
        "an applied config should show up in the event strip: {payload}"
    );
}
