//! Minimal HTTP/1.1 backend for the prx benchmark harness.
//!
//! The backend must never be the bottleneck, so it does the least work that is
//! still a correct HTTP/1.1 keep-alive server: it reads until the end of the
//! request head, picks a canned response by path, and writes it back.
//!
//! Routes:
//!   /            -> 1 KiB body
//!   /1k          -> 1 KiB body
//!   /64k         -> 64 KiB body
//!   /slow/<ms>   -> 1 KiB body after <ms> milliseconds (for tail-latency tests)
//!   /healthz     -> "ok"
//!
//! Env:
//!   LISTEN       default 0.0.0.0:8000
//!   WORKERS      default: number of cores

use std::{env, io, sync::Arc, time::Duration};

use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};

struct Bodies {
    small: Arc<Vec<u8>>,
    large: Arc<Vec<u8>>,
}

fn response(body: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(body.len() + 128);
    out.extend_from_slice(b"HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\n");
    out.extend_from_slice(format!("Content-Length: {}\r\n", body.len()).as_bytes());
    out.extend_from_slice(b"Connection: keep-alive\r\n\r\n");
    out.extend_from_slice(body);
    out
}

fn main() -> io::Result<()> {
    let listen = env::var("LISTEN").unwrap_or_else(|_| "0.0.0.0:8000".to_string());
    let workers = env::var("WORKERS")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or_else(|| std::thread::available_parallelism().map_or(4, |n| n.get()));

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(workers)
        .enable_all()
        .build()?;

    runtime.block_on(async move {
        let bodies = Arc::new(Bodies {
            small: Arc::new(response(&vec![b'x'; 1024])),
            large: Arc::new(response(&vec![b'x'; 64 * 1024])),
        });
        let listener = TcpListener::bind(&listen).await?;
        eprintln!("prx-bench-backend listening on {listen} with {workers} workers");

        loop {
            let (stream, _) = listener.accept().await?;
            let bodies = bodies.clone();
            tokio::spawn(async move {
                let _ = serve(stream, bodies).await;
            });
        }
    })
}

async fn serve(mut stream: TcpStream, bodies: Arc<Bodies>) -> io::Result<()> {
    stream.set_nodelay(true)?;
    let mut buf = vec![0u8; 16 * 1024];
    let mut filled = 0usize;

    loop {
        // Read until the request head is complete. Bodies are not consumed:
        // the harness only issues GETs.
        let head_end = loop {
            if let Some(pos) = find_head_end(&buf[..filled]) {
                break pos;
            }
            if filled == buf.len() {
                return Ok(()); // request head too large for a benchmark client
            }
            let n = stream.read(&mut buf[filled..]).await?;
            if n == 0 {
                return Ok(());
            }
            filled += n;
        };

        let path = request_path(&buf[..head_end]);
        let delay_ms = path
            .strip_prefix("/slow/")
            .and_then(|rest| rest.split('/').next())
            .and_then(|ms| ms.parse::<u64>().ok());
        if let Some(ms) = delay_ms {
            tokio::time::sleep(Duration::from_millis(ms)).await;
        }

        let body = match path {
            "/64k" => bodies.large.clone(),
            "/healthz" => Arc::new(response(b"ok\n")),
            _ => bodies.small.clone(),
        };
        stream.write_all(&body).await?;

        // Carry over any pipelined bytes that arrived with this request.
        buf.copy_within(head_end..filled, 0);
        filled -= head_end;
    }
}

/// Returns the index just past the `\r\n\r\n` that ends the request head.
fn find_head_end(buf: &[u8]) -> Option<usize> {
    buf.windows(4).position(|w| w == b"\r\n\r\n").map(|p| p + 4)
}

fn request_path(head: &[u8]) -> &str {
    let line = head.split(|b| *b == b'\r').next().unwrap_or(head);
    let mut parts = line.split(|b| *b == b' ');
    let _method = parts.next();
    let path = parts.next().unwrap_or(b"/");
    std::str::from_utf8(path).unwrap_or("/")
}
