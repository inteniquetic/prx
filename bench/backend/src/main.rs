//! Minimal HTTP/1.1 backend for the prx benchmark harness.
//!
//! The backend must never be the bottleneck, so it does the least work that is
//! still a correct HTTP/1.1 keep-alive server: it reads until the end of the
//! request head, picks a canned response by path, drops any request body, and
//! writes the response back.
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
        // Read until the request head is complete. The body, if any, is
        // dropped below once the response has been picked.
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
        let body_len = content_length(&buf[..head_end]);

        // Drop the request body: what is already buffered, then the rest off
        // the wire. Anything past it is a pipelined request and is carried over.
        let buffered = (filled - head_end).min(body_len);
        buf.copy_within(head_end + buffered..filled, 0);
        filled -= head_end + buffered;
        let mut remaining = body_len - buffered;
        while remaining > 0 {
            let want = remaining.min(buf.len());
            let n = stream.read(&mut buf[..want]).await?;
            if n == 0 {
                return Ok(());
            }
            remaining -= n;
        }

        stream.write_all(&body).await?;
    }
}

// ponytail: Content-Length only. Every proxy under test forwards a length for
// the fixed-size bodies oha sends; add chunked decoding if a scenario needs it.
fn content_length(head: &[u8]) -> usize {
    for line in head.split(|b| *b == b'\n') {
        let Some(colon) = line.iter().position(|b| *b == b':') else {
            continue;
        };
        if line[..colon].eq_ignore_ascii_case(b"content-length") {
            return std::str::from_utf8(&line[colon + 1..])
                .ok()
                .and_then(|v| v.trim().parse().ok())
                .unwrap_or(0);
        }
    }
    0
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

#[cfg(test)]
mod tests {
    use super::content_length;

    #[test]
    fn reads_content_length_case_insensitively() {
        assert_eq!(
            content_length(b"POST / HTTP/1.1\r\ncontent-LENGTH: 42\r\n\r\n"),
            42
        );
    }

    #[test]
    fn no_header_means_no_body() {
        assert_eq!(content_length(b"GET / HTTP/1.1\r\nHost: x\r\n\r\n"), 0);
    }
}
