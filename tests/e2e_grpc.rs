//! T115: gRPC proxying, end to end through the real binary.
//!
//! gRPC is HTTP/2 plus two things a proxy must not lose: the request must stay
//! HTTP/2 all the way to the upstream, and the `grpc-status` trailer must come
//! back. If either is dropped, every gRPC call fails, so these tests drive raw
//! h2 frames rather than a protobuf stack (no protoc needed) and assert on the
//! frames prx actually forwards.

mod common;

use std::time::Duration;

use bytes::{BufMut, Bytes, BytesMut};
use h2::client;
use h2::server;
use http::{HeaderMap, Method, Request, Version};
use tempfile::TempDir;
use tokio::net::{TcpListener, TcpStream};

use common::{PrxProcess, reserve_port, write_config};

/// Wraps a payload in the gRPC length-prefixed message framing.
fn grpc_frame(payload: &[u8]) -> Bytes {
    let mut buf = BytesMut::with_capacity(payload.len() + 5);
    buf.put_u8(0); // not compressed
    buf.put_u32(payload.len() as u32);
    buf.put_slice(payload);
    buf.freeze()
}

/// Minimal h2c "gRPC" upstream: echoes the request body and always finishes
/// with trailers, the way a real gRPC server does.
async fn spawn_grpc_upstream(port: u16, status: &'static str, message_count: usize) {
    let listener = TcpListener::bind(("127.0.0.1", port))
        .await
        .expect("failed to bind grpc upstream");

    tokio::spawn(async move {
        while let Ok((socket, _)) = listener.accept().await {
            tokio::spawn(async move {
                let Ok(mut conn) = server::handshake(socket).await else {
                    return;
                };
                while let Some(Ok((request, mut respond))) = conn.accept().await {
                    tokio::spawn(async move {
                        let mut body = request.into_body();
                        let mut received = BytesMut::new();
                        while let Some(Ok(chunk)) = body.data().await {
                            let _ = body.flow_control().release_capacity(chunk.len());
                            received.put_slice(&chunk);
                        }

                        let response = http::Response::builder()
                            .status(200)
                            .header("content-type", "application/grpc")
                            .body(())
                            .expect("failed to build response");
                        let Ok(mut stream) = respond.send_response(response, false) else {
                            return;
                        };

                        // The client already sent a framed gRPC message, so
                        // echo it back as-is rather than framing it twice.
                        let echo = received.freeze();
                        for _ in 0..message_count {
                            if stream.send_data(echo.clone(), false).is_err() {
                                return;
                            }
                        }

                        let mut trailers = HeaderMap::new();
                        trailers.insert("grpc-status", status.parse().expect("valid status"));
                        trailers.insert("grpc-message", "ok".parse().expect("valid message"));
                        let _ = stream.send_trailers(trailers);
                    });
                }
            });
        }
    });
}

fn config(proxy_port: u16, upstream_port: u16) -> String {
    format!(
        r#"[server]
listen = ["127.0.0.1:{proxy_port}"]
h2c = true

[observability]
log_level = "error"
access_log = false

[[service]]
name = "grpc"
upstream_h2 = "always"

[[service.upstream]]
addr = "127.0.0.1:{upstream_port}"

[[route]]
name = "grpc"
service = "grpc"
path_prefix = "/"
is_default = true
"#
    )
}

struct Harness {
    _prx: PrxProcess,
    proxy_port: u16,
    _tmp: TempDir,
}

async fn start(status: &'static str, message_count: usize) -> Harness {
    let upstream_port = reserve_port();
    spawn_grpc_upstream(upstream_port, status, message_count).await;

    let proxy_port = reserve_port();
    let tmp = TempDir::new().expect("failed to create temp dir");
    let cfg_path = write_config(&tmp, &config(proxy_port, upstream_port));
    let prx = PrxProcess::spawn(&cfg_path, reserve_port());
    prx.wait_until_listening(proxy_port);

    Harness {
        _prx: prx,
        proxy_port,
        _tmp: tmp,
    }
}

struct Call {
    status: u16,
    version: Version,
    messages: Vec<Bytes>,
    trailers: Option<HeaderMap>,
}

/// Sends one unary gRPC call through prx over cleartext HTTP/2.
async fn call(proxy_port: u16, payload: &[u8], message_count: usize) -> Call {
    let tcp = TcpStream::connect(("127.0.0.1", proxy_port))
        .await
        .expect("failed to connect to prx");
    let (mut send_request, connection) = client::handshake(tcp)
        .await
        .expect("h2c handshake with prx failed");
    tokio::spawn(async move {
        let _ = connection.await;
    });

    let request = Request::builder()
        .method(Method::POST)
        .uri(format!("http://127.0.0.1:{proxy_port}/echo.Echo/Unary"))
        .header("content-type", "application/grpc")
        .header("te", "trailers")
        .body(())
        .expect("failed to build request");

    let (response, mut stream) = send_request
        .send_request(request, false)
        .expect("failed to send request");
    stream
        .send_data(grpc_frame(payload), true)
        .expect("failed to send body");

    let response = tokio::time::timeout(Duration::from_secs(10), response)
        .await
        .expect("timed out waiting for the response")
        .expect("response failed");

    let status = response.status().as_u16();
    let version = response.version();
    let mut body = response.into_body();

    let mut messages = Vec::new();
    while let Some(chunk) = body.data().await {
        let chunk = chunk.expect("body chunk failed");
        let _ = body.flow_control().release_capacity(chunk.len());
        messages.push(chunk);
    }
    let trailers = body.trailers().await.expect("trailers failed");

    let _ = message_count;
    Call {
        status,
        version,
        messages,
        trailers,
    }
}

#[tokio::test]
async fn forwards_a_unary_call_with_its_trailers() {
    let harness = start("0", 1).await;
    let payload = b"ping";
    let result = call(harness.proxy_port, payload, 1).await;

    assert_eq!(result.status, 200);
    assert_eq!(
        result.version,
        Version::HTTP_2,
        "the client must stay on HTTP/2 end to end"
    );
    assert_eq!(
        result.messages.concat(),
        grpc_frame(payload),
        "the gRPC message came back altered"
    );

    let trailers = result
        .trailers
        .expect("no trailers: every gRPC call fails without grpc-status");
    assert_eq!(
        trailers.get("grpc-status").map(|v| v.to_str().unwrap()),
        Some("0")
    );
}

#[tokio::test]
async fn forwards_a_non_zero_grpc_status() {
    // 5 = NOT_FOUND. An error status rides the same trailer as success, so it
    // must survive the proxy too.
    let harness = start("5", 1).await;
    let result = call(harness.proxy_port, b"missing", 1).await;

    assert_eq!(result.status, 200, "gRPC errors are still HTTP 200");
    let trailers = result.trailers.expect("no trailers on the error path");
    assert_eq!(
        trailers.get("grpc-status").map(|v| v.to_str().unwrap()),
        Some("5")
    );
}

#[tokio::test]
async fn forwards_a_server_streaming_response() {
    let harness = start("0", 4).await;
    let payload = b"stream";
    let result = call(harness.proxy_port, payload, 4).await;

    let expected = grpc_frame(payload);
    let body = result.messages.concat();
    assert_eq!(
        body.len(),
        expected.len() * 4,
        "expected 4 streamed messages, got {} bytes",
        body.len()
    );
    assert!(
        result.trailers.is_some(),
        "a streaming call must still end with trailers"
    );
}
