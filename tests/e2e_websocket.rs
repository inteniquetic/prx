//! T115: WebSocket proxying, end to end through the real binary.
//!
//! The README advertises websocket support but nothing verified it, which made
//! it easy for a request-path refactor (T101/T102) to break upgrades without a
//! test noticing. These tests drive a real upstream websocket echo server
//! through prx.

mod common;

use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use tempfile::TempDir;
use tokio::net::TcpListener;
use tokio_tungstenite::{
    accept_async, connect_async,
    tungstenite::{Message, protocol::CloseFrame, protocol::frame::coding::CloseCode},
};

use common::{PrxProcess, reserve_port, write_config};

/// Echo server: sends every frame straight back and answers a close with a close.
async fn spawn_echo_upstream(port: u16) {
    let listener = TcpListener::bind(("127.0.0.1", port))
        .await
        .expect("failed to bind websocket upstream");

    tokio::spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            tokio::spawn(async move {
                let Ok(mut ws) = accept_async(stream).await else {
                    return;
                };
                while let Some(Ok(msg)) = ws.next().await {
                    match msg {
                        Message::Text(_) | Message::Binary(_) => {
                            if ws.send(msg).await.is_err() {
                                return;
                            }
                        }
                        Message::Close(frame) => {
                            // Mirror the close, then keep polling until the
                            // peer finishes: dropping the socket right away
                            // resets the connection instead of closing it.
                            let _ = ws.send(Message::Close(frame)).await;
                            let _ = ws.close(None).await;
                            while ws.next().await.is_some() {}
                            return;
                        }
                        // tungstenite answers pings for us.
                        Message::Ping(_) | Message::Pong(_) | Message::Frame(_) => {}
                    }
                }
            });
        }
    });
}

fn config(proxy_port: u16, upstream_port: u16, read_timeout_ms: u64) -> String {
    format!(
        r#"[server]
listen = ["127.0.0.1:{proxy_port}"]

[observability]
log_level = "error"
access_log = false

[[service]]
name = "ws"

[[service.upstream]]
addr = "127.0.0.1:{upstream_port}"
read_timeout_ms = {read_timeout_ms}

[[route]]
name = "ws"
service = "ws"
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

async fn start(read_timeout_ms: u64) -> Harness {
    let upstream_port = reserve_port();
    spawn_echo_upstream(upstream_port).await;

    let proxy_port = reserve_port();
    let tmp = TempDir::new().expect("failed to create temp dir");
    let cfg_path = write_config(&tmp, &config(proxy_port, upstream_port, read_timeout_ms));
    let prx = PrxProcess::spawn(&cfg_path, reserve_port());
    prx.wait_until_listening(proxy_port);

    Harness {
        _prx: prx,
        proxy_port,
        _tmp: tmp,
    }
}

#[tokio::test]
async fn proxies_text_and_binary_frames() {
    let harness = start(5_000).await;
    let url = format!("ws://127.0.0.1:{}/socket", harness.proxy_port);
    let (mut ws, response) = connect_async(&url).await.expect("websocket upgrade failed");

    assert_eq!(
        response.status().as_u16(),
        101,
        "expected a 101 upgrade, got {:?}",
        response.status()
    );

    ws.send(Message::text("hello")).await.expect("send text");
    let echoed = ws.next().await.expect("no reply").expect("reply error");
    assert_eq!(echoed, Message::text("hello"));

    let payload = vec![7u8; 1024];
    ws.send(Message::binary(payload.clone()))
        .await
        .expect("send binary");
    let echoed = ws.next().await.expect("no reply").expect("reply error");
    assert_eq!(echoed, Message::binary(payload));
}

#[tokio::test]
async fn proxies_frames_larger_than_a_single_read() {
    let harness = start(5_000).await;
    let url = format!("ws://127.0.0.1:{}/socket", harness.proxy_port);
    let (mut ws, _) = connect_async(&url).await.expect("websocket upgrade failed");

    // Larger than the usual 64KiB buffer, so the frame spans several reads.
    let payload = vec![3u8; 256 * 1024];
    ws.send(Message::binary(payload.clone()))
        .await
        .expect("send large frame");
    let echoed = ws.next().await.expect("no reply").expect("reply error");
    assert_eq!(echoed, Message::binary(payload));
}

#[tokio::test]
async fn forwards_the_close_handshake() {
    let harness = start(5_000).await;
    let url = format!("ws://127.0.0.1:{}/socket", harness.proxy_port);
    let (mut ws, _) = connect_async(&url).await.expect("websocket upgrade failed");

    ws.send(Message::Close(Some(CloseFrame {
        code: CloseCode::Normal,
        reason: "done".into(),
    })))
    .await
    .expect("send close");

    let reply = ws
        .next()
        .await
        .expect("no close reply")
        .expect("close error");
    match reply {
        Message::Close(Some(frame)) => assert_eq!(frame.code, CloseCode::Normal),
        other => panic!("expected a close frame, got {other:?}"),
    }
}

/// The classic websocket failure: an idle connection gets cut by the upstream
/// read timeout, which is meant for request/response traffic.
#[tokio::test]
async fn an_idle_connection_survives_the_upstream_read_timeout() {
    let read_timeout_ms = 500;
    let harness = start(read_timeout_ms).await;
    let url = format!("ws://127.0.0.1:{}/socket", harness.proxy_port);
    let (mut ws, _) = connect_async(&url).await.expect("websocket upgrade failed");

    // Stay silent for longer than the upstream read timeout, then talk again.
    tokio::time::sleep(Duration::from_millis(read_timeout_ms * 3)).await;

    ws.send(Message::text("still here"))
        .await
        .expect("send after idle");
    let echoed = ws
        .next()
        .await
        .expect("connection was closed while idle")
        .expect("reply error after idle");
    assert_eq!(echoed, Message::text("still here"));
}
