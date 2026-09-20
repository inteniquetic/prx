//! Shared helpers for the end-to-end tests.
//!
//! Each integration test file is its own crate, so the process/port/config
//! plumbing lives here instead of being copied per file.

#![allow(dead_code)]

use std::{
    fs,
    io::{Read, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use tempfile::TempDir;

/// Accepts from a non-blocking listener and hands back a *blocking* stream.
///
/// macOS and the BSDs make an accepted socket inherit `O_NONBLOCK` from its
/// listener; Linux does not. Left that way, a mock's first `read` returns
/// `WouldBlock` whenever the accept wins the race against the request bytes
/// (`set_read_timeout` does nothing on a non-blocking socket), so the mock
/// hangs up or answers without having read the request, and prx reports a 502.
pub fn accept_blocking(listener: &TcpListener) -> std::io::Result<(TcpStream, SocketAddr)> {
    let (stream, peer) = listener.accept()?;
    stream.set_nonblocking(false)?;
    Ok((stream, peer))
}

pub struct UpstreamServer {
    shutdown: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
    port: u16,
}

impl UpstreamServer {
    pub fn spawn(port: u16, body: &'static str) -> Self {
        let shutdown = Arc::new(AtomicBool::new(false));
        let stop = shutdown.clone();
        let handle = thread::spawn(move || {
            let listener =
                TcpListener::bind(("127.0.0.1", port)).expect("failed to bind upstream server");
            listener
                .set_nonblocking(true)
                .expect("failed to set nonblocking upstream listener");

            while !stop.load(Ordering::Relaxed) {
                match accept_blocking(&listener) {
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

pub struct PrxProcess {
    child: Child,
}

impl PrxProcess {
    pub fn spawn(config_path: &Path, admin_port: u16) -> Self {
        let child = Command::new(resolve_prx_binary())
            .env("PRX_CONFIG", config_path)
            .env("PRX_ADMIN_LISTEN", format!("127.0.0.1:{admin_port}"))
            .env("RUST_LOG", "error")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("failed to spawn prx");
        Self { child }
    }

    pub fn wait_until_listening(&self, port: u16) {
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

pub fn prx_binary() -> PathBuf {
    resolve_prx_binary()
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

pub fn reserve_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .expect("failed to bind random port")
        .local_addr()
        .expect("failed to get local addr")
        .port()
}

pub fn write_config(dir: &TempDir, content: &str) -> PathBuf {
    let path = dir.path().join("Prx.toml");
    fs::write(&path, content).expect("failed to write config");
    path
}

pub fn send_get(port: u16, host: &str, path: &str) -> String {
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

/// Upstream that replies with the request head it received, so a test can
/// assert on exactly which headers prx forwarded.
pub struct EchoHeadersUpstream {
    shutdown: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
    port: u16,
}

impl EchoHeadersUpstream {
    pub fn spawn(port: u16) -> Self {
        let shutdown = Arc::new(AtomicBool::new(false));
        let stop = shutdown.clone();
        let handle = thread::spawn(move || {
            let listener =
                TcpListener::bind(("127.0.0.1", port)).expect("failed to bind echo upstream");
            listener
                .set_nonblocking(true)
                .expect("failed to set nonblocking echo listener");

            while !stop.load(Ordering::Relaxed) {
                match accept_blocking(&listener) {
                    Ok((mut stream, _)) => {
                        let _ = echo_request_head(&mut stream);
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

impl Drop for EchoHeadersUpstream {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Relaxed);
        let _ = TcpStream::connect(("127.0.0.1", self.port));
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

fn echo_request_head(stream: &mut TcpStream) -> std::io::Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(2)))?;
    let mut buf = [0u8; 8192];
    let read = stream.read(&mut buf)?;
    let head = String::from_utf8_lossy(&buf[..read]).to_string();

    let resp = format!(
        "HTTP/1.1 200 OK\r\ncontent-length: {}\r\ncontent-type: text/plain\r\nx-upstream: echo\r\nserver: test-upstream\r\nconnection: close\r\n\r\n{}",
        head.len(),
        head
    );
    stream.write_all(resp.as_bytes())?;
    stream.flush()?;
    Ok(())
}

/// Sends a request with extra raw header lines (e.g. "X-Forwarded-For: 1.2.3.4").
pub fn send_get_with_headers(port: u16, host: &str, path: &str, extra: &[&str]) -> String {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("failed to connect to prx");
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("failed to set read timeout");
    let mut req = format!("GET {path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n");
    for line in extra {
        req.push_str(line);
        req.push_str("\r\n");
    }
    req.push_str("\r\n");
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

/// Upstream that accepts the connection and then answers after a delay, for
/// exercising request time budgets.
pub struct SlowUpstream {
    shutdown: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
    port: u16,
}

impl SlowUpstream {
    pub fn spawn(port: u16, delay: Duration) -> Self {
        let shutdown = Arc::new(AtomicBool::new(false));
        let stop = shutdown.clone();
        let handle = thread::spawn(move || {
            let listener =
                TcpListener::bind(("127.0.0.1", port)).expect("failed to bind slow upstream");
            listener
                .set_nonblocking(true)
                .expect("failed to set nonblocking slow listener");

            while !stop.load(Ordering::Relaxed) {
                match accept_blocking(&listener) {
                    Ok((mut stream, _)) => {
                        let stop = stop.clone();
                        thread::spawn(move || {
                            let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
                            let mut buf = [0u8; 2048];
                            let _ = stream.read(&mut buf);

                            // Sleep in slices so shutdown does not have to wait
                            // out the whole delay.
                            let deadline = Instant::now() + delay;
                            while Instant::now() < deadline && !stop.load(Ordering::Relaxed) {
                                thread::sleep(Duration::from_millis(20));
                            }

                            let body = "slow";
                            let resp = format!(
                                "HTTP/1.1 200 OK\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                                body.len(),
                                body
                            );
                            let _ = stream.write_all(resp.as_bytes());
                            let _ = stream.flush();
                        });
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

impl Drop for SlowUpstream {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Relaxed);
        let _ = TcpStream::connect(("127.0.0.1", self.port));
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

/// Counts how many requests reach it, always failing the connection, so a test
/// can see exactly how many attempts prx made.
pub struct CountingRefusedUpstream {
    shutdown: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
    port: u16,
    pub attempts: Arc<AtomicUsize>,
}

impl CountingRefusedUpstream {
    pub fn spawn(port: u16) -> Self {
        let shutdown = Arc::new(AtomicBool::new(false));
        let attempts = Arc::new(AtomicUsize::new(0));
        let stop = shutdown.clone();
        let counter = attempts.clone();
        let handle = thread::spawn(move || {
            let listener =
                TcpListener::bind(("127.0.0.1", port)).expect("failed to bind counting upstream");
            listener
                .set_nonblocking(true)
                .expect("failed to set nonblocking counting listener");

            while !stop.load(Ordering::Relaxed) {
                match accept_blocking(&listener) {
                    Ok((stream, _)) => {
                        counter.fetch_add(1, Ordering::Relaxed);
                        // Drop without answering: the proxy sees the connection
                        // close mid-request.
                        drop(stream);
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
            handle: Some(handle),
            port,
            attempts,
        }
    }

    pub fn attempts(&self) -> usize {
        self.attempts.load(Ordering::Relaxed)
    }
}

impl Drop for CountingRefusedUpstream {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Relaxed);
        let _ = TcpStream::connect(("127.0.0.1", self.port));
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

/// Sends a request with a body, for testing that non-idempotent methods are
/// not replayed.
pub fn send_post(port: u16, host: &str, path: &str, body: &str) -> String {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("failed to connect to prx");
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .expect("failed to set read timeout");
    let req = format!(
        "POST {path} HTTP/1.1\r\nHost: {host}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    stream
        .write_all(req.as_bytes())
        .expect("failed to write request");
    stream.flush().expect("failed to flush request");
    let mut response = String::new();
    let _ = stream.read_to_string(&mut response);
    response
}

/// Upstream that counts requests and can be made slow, so a test can see how
/// many fetches actually reached it.
pub struct CountingUpstream {
    shutdown: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
    port: u16,
    hits: Arc<AtomicUsize>,
}

impl CountingUpstream {
    pub fn spawn(port: u16, body: &'static str, delay: Duration) -> Self {
        let shutdown = Arc::new(AtomicBool::new(false));
        let hits = Arc::new(AtomicUsize::new(0));
        let stop = shutdown.clone();
        let counter = hits.clone();

        let handle = thread::spawn(move || {
            let listener =
                TcpListener::bind(("127.0.0.1", port)).expect("failed to bind counting upstream");
            listener
                .set_nonblocking(true)
                .expect("failed to set nonblocking listener");

            while !stop.load(Ordering::Relaxed) {
                match accept_blocking(&listener) {
                    Ok((mut stream, _)) => {
                        let counter = counter.clone();
                        thread::spawn(move || {
                            let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
                            let mut buf = [0u8; 4096];
                            let read = stream.read(&mut buf).unwrap_or(0);
                            let request = String::from_utf8_lossy(&buf[..read]).to_string();
                            counter.fetch_add(1, Ordering::Relaxed);
                            thread::sleep(delay);

                            // Echo the request line so a test can tell which
                            // variant of a request produced this response.
                            let first_line = request.lines().next().unwrap_or("").to_string();
                            let payload = format!("{body}|{first_line}");
                            let resp = format!(
                                "HTTP/1.1 200 OK\r\ncontent-length: {}\r\ncontent-type: text/plain\r\nconnection: close\r\n\r\n{}",
                                payload.len(),
                                payload
                            );
                            let _ = stream.write_all(resp.as_bytes());
                            let _ = stream.flush();
                        });
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
            handle: Some(handle),
            port,
            hits,
        }
    }

    pub fn hits(&self) -> usize {
        self.hits.load(Ordering::Relaxed)
    }
}

impl Drop for CountingUpstream {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Relaxed);
        let _ = TcpStream::connect(("127.0.0.1", self.port));
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

/// Upstream that answers with the headers a test asks for, to exercise
/// cacheability rules.
pub struct HeaderControlledUpstream {
    shutdown: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
    port: u16,
    hits: Arc<AtomicUsize>,
}

impl HeaderControlledUpstream {
    pub fn spawn(port: u16, extra_headers: &'static str, body: &'static str) -> Self {
        let shutdown = Arc::new(AtomicBool::new(false));
        let hits = Arc::new(AtomicUsize::new(0));
        let stop = shutdown.clone();
        let counter = hits.clone();

        let handle = thread::spawn(move || {
            let listener = TcpListener::bind(("127.0.0.1", port))
                .expect("failed to bind header-controlled upstream");
            listener
                .set_nonblocking(true)
                .expect("failed to set nonblocking listener");

            while !stop.load(Ordering::Relaxed) {
                match accept_blocking(&listener) {
                    Ok((mut stream, _)) => {
                        counter.fetch_add(1, Ordering::Relaxed);
                        let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
                        let mut buf = [0u8; 4096];
                        let _ = stream.read(&mut buf);
                        let resp = format!(
                            "HTTP/1.1 200 OK\r\ncontent-length: {}\r\n{}connection: close\r\n\r\n{}",
                            body.len(),
                            extra_headers,
                            body
                        );
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
            handle: Some(handle),
            port,
            hits,
        }
    }

    pub fn hits(&self) -> usize {
        self.hits.load(Ordering::Relaxed)
    }
}

impl Drop for HeaderControlledUpstream {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Relaxed);
        let _ = TcpStream::connect(("127.0.0.1", self.port));
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

/// Sends a request and returns the raw bytes of the response, so a test can
/// inspect a compressed body rather than assuming it is UTF-8.
pub fn send_get_raw(port: u16, host: &str, path: &str, extra: &[&str]) -> Vec<u8> {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("failed to connect to prx");
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("failed to set read timeout");
    let mut req = format!("GET {path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n");
    for line in extra {
        req.push_str(line);
        req.push_str("\r\n");
    }
    req.push_str("\r\n");
    stream
        .write_all(req.as_bytes())
        .expect("failed to write request");
    stream.flush().expect("failed to flush request");
    let mut response = Vec::new();
    std::io::Read::read_to_end(&mut stream, &mut response).expect("failed to read response");
    response
}

/// Splits a raw response into its head (as text) and body bytes.
pub fn split_response(raw: &[u8]) -> (String, Vec<u8>) {
    let separator = b"\r\n\r\n";
    let position = raw
        .windows(separator.len())
        .position(|window| window == separator);
    match position {
        Some(idx) => (
            String::from_utf8_lossy(&raw[..idx]).to_string(),
            raw[idx + separator.len()..].to_vec(),
        ),
        None => (String::from_utf8_lossy(raw).to_string(), Vec::new()),
    }
}

/// Upstream that returns a large, highly compressible body.
pub struct BigBodyUpstream {
    shutdown: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
    port: u16,
}

impl BigBodyUpstream {
    pub fn spawn(port: u16, body_len: usize, extra_headers: &'static str) -> Self {
        let shutdown = Arc::new(AtomicBool::new(false));
        let stop = shutdown.clone();
        let handle = thread::spawn(move || {
            let listener =
                TcpListener::bind(("127.0.0.1", port)).expect("failed to bind big-body upstream");
            listener
                .set_nonblocking(true)
                .expect("failed to set nonblocking listener");
            let body = "a".repeat(body_len);

            while !stop.load(Ordering::Relaxed) {
                match accept_blocking(&listener) {
                    Ok((mut stream, _)) => {
                        let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
                        let mut buf = [0u8; 4096];
                        let _ = stream.read(&mut buf);
                        let resp = format!(
                            "HTTP/1.1 200 OK\r\ncontent-length: {}\r\ncontent-type: text/plain\r\n{}connection: close\r\n\r\n{}",
                            body.len(),
                            extra_headers,
                            body
                        );
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
            handle: Some(handle),
            port,
        }
    }
}

impl Drop for BigBodyUpstream {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Relaxed);
        let _ = TcpStream::connect(("127.0.0.1", self.port));
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}
