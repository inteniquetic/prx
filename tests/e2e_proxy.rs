use std::{
    fs,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use tempfile::TempDir;

struct UpstreamServer {
    shutdown: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
    port: u16,
}

impl UpstreamServer {
    fn spawn(port: u16, body: &'static str) -> Self {
        let shutdown = Arc::new(AtomicBool::new(false));
        let stop = shutdown.clone();
        let handle = thread::spawn(move || {
            let listener =
                TcpListener::bind(("127.0.0.1", port)).expect("failed to bind upstream server");
            listener
                .set_nonblocking(true)
                .expect("failed to set nonblocking upstream listener");

            while !stop.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        let _ = handle_upstream_conn(&mut stream, body);
                    }
                    Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(10));
                    }
                    Err(_) => break,
                }
            }
        });

        Self {
            shutdown,
            handle: Some(handle),
            port,
        }
    }
}

impl Drop for UpstreamServer {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Relaxed);
        let _ = TcpStream::connect(("127.0.0.1", self.port));
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

fn handle_upstream_conn(stream: &mut TcpStream, body: &'static str) -> std::io::Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(2)))?;
    let mut buf = [0u8; 2048];
    let _ = stream.read(&mut buf)?;

    let resp = format!(
        "HTTP/1.1 200 OK\r\ncontent-length: {}\r\ncontent-type: text/plain\r\nconnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    stream.write_all(resp.as_bytes())?;
    stream.flush()?;
    Ok(())
}

struct PrxProcess {
    child: Child,
}

impl PrxProcess {
    fn spawn(config_path: &Path) -> Self {
        let child = Command::new(resolve_prx_binary())
            .env("PRX_CONFIG", config_path)
            .env("RUST_LOG", "error")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("failed to spawn prx");
        Self { child }
    }

    fn wait_until_listening(&self, port: u16) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            if TcpStream::connect(("127.0.0.1", port)).is_ok() {
                return;
            }
            thread::sleep(Duration::from_millis(50));
        }
        panic!("prx did not start listening on port {port}");
    }
}

fn resolve_prx_binary() -> PathBuf {
    if let Ok(path) = std::env::var("CARGO_BIN_EXE_prx") {
        return PathBuf::from(path);
    }

    let mut candidate = std::env::current_exe()
        .expect("failed to resolve current test binary path")
        .parent()
        .expect("missing test binary parent")
        .parent()
        .expect("missing target debug parent")
        .join("prx");
    if cfg!(windows) {
        candidate.set_extension("exe");
    }

    if candidate.exists() {
        return candidate;
    }

    panic!(
        "unable to locate prx binary: tried CARGO_BIN_EXE_prx and {}",
        candidate.display()
    );
}

impl Drop for PrxProcess {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn reserve_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .expect("failed to bind random port")
        .local_addr()
        .expect("failed to get local addr")
        .port()
}

fn write_config(dir: &TempDir, content: &str) -> PathBuf {
    let path = dir.path().join("Prx.toml");
    fs::write(&path, content).expect("failed to write config");
    path
}

fn send_get(port: u16, host: &str, path: &str) -> String {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("failed to connect to prx");
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("failed to set read timeout");
    let req = format!("GET {path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n");
    stream
        .write_all(req.as_bytes())
        .expect("failed to write request");
    stream.flush().expect("failed to flush request");
    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .expect("failed to read response");
    response
}

#[test]
fn routes_request_to_upstream() {
    let upstream_port = reserve_port();
    let _upstream = UpstreamServer::spawn(upstream_port, "hello from upstream");
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

[[route]]
name = "app"
host = "app.local"
path_prefix = "/"
is_default = false
lb = "round_robin"
max_retries = 0

[[route.upstream]]
addr = "127.0.0.1:{upstream_port}"
"#
    );
    let cfg_path = write_config(&tmp, &cfg);

    let prx = PrxProcess::spawn(&cfg_path);
    prx.wait_until_listening(proxy_port);
    let response = send_get(proxy_port, "app.local", "/");

    assert!(response.starts_with("HTTP/1.1 200"), "response: {response}");
    assert!(
        response.contains("hello from upstream"),
        "response: {response}"
    );
}

#[test]
fn returns_404_when_no_route_matches() {
    let upstream_port = reserve_port();
    let _upstream = UpstreamServer::spawn(upstream_port, "unused");
    let proxy_port = reserve_port();
    let tmp = TempDir::new().expect("failed to create temp dir");
    let cfg = format!(
        r#"[server]
listen = ["127.0.0.1:{proxy_port}"]

[observability]
log_level = "error"
access_log = false

[[route]]
name = "only"
host = "only.local"
path_prefix = "/"
is_default = false
lb = "round_robin"
max_retries = 0

[[route.upstream]]
addr = "127.0.0.1:{upstream_port}"
"#
    );
    let cfg_path = write_config(&tmp, &cfg);

    let prx = PrxProcess::spawn(&cfg_path);
    prx.wait_until_listening(proxy_port);
    let response = send_get(proxy_port, "other.local", "/");

    assert!(response.starts_with("HTTP/1.1 404"), "response: {response}");
}

#[test]
fn retries_and_fails_over_to_next_upstream() {
    let unreachable_port = reserve_port();
    let healthy_port = reserve_port();
    let _healthy = UpstreamServer::spawn(healthy_port, "served by failover");
    let proxy_port = reserve_port();
    let tmp = TempDir::new().expect("failed to create temp dir");
    let cfg = format!(
        r#"[server]
listen = ["127.0.0.1:{proxy_port}"]

[observability]
log_level = "error"
access_log = false

[[route]]
name = "app"
host = "app.local"
path_prefix = "/"
is_default = false
lb = "round_robin"
max_retries = 1
retry_backoff_ms = 0

[[route.upstream]]
addr = "127.0.0.1:{unreachable_port}"

[[route.upstream]]
addr = "127.0.0.1:{healthy_port}"
"#
    );
    let cfg_path = write_config(&tmp, &cfg);

    let prx = PrxProcess::spawn(&cfg_path);
    prx.wait_until_listening(proxy_port);
    let response = send_get(proxy_port, "app.local", "/");

    assert!(response.starts_with("HTTP/1.1 200"), "response: {response}");
    assert!(
        response.contains("served by failover"),
        "response: {response}"
    );
}

#[test]
fn serves_health_and_ready_endpoints() {
    let upstream_port = reserve_port();
    let _upstream = UpstreamServer::spawn(upstream_port, "ok");
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

[[route]]
name = "app"
host = "app.local"
path_prefix = "/"
is_default = false
lb = "round_robin"
max_retries = 0

[[route.upstream]]
addr = "127.0.0.1:{upstream_port}"
"#
    );
    let cfg_path = write_config(&tmp, &cfg);

    let prx = PrxProcess::spawn(&cfg_path);
    prx.wait_until_listening(proxy_port);

    let health = send_get(proxy_port, "any.local", "/healthz");
    assert!(health.starts_with("HTTP/1.1 200"), "health: {health}");
    assert!(health.contains("ok"), "health: {health}");

    let ready = send_get(proxy_port, "any.local", "/readyz");
    assert!(ready.starts_with("HTTP/1.1 200"), "ready: {ready}");
    assert!(ready.contains("ready"), "ready: {ready}");
}
