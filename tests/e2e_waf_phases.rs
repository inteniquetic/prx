//! T512: WAF phases 2-5 through the real Pingora pipeline, with a TCP upstream.
//!
//! Phase 1 is covered by `e2e_waf.rs`. These tests cover request body (2),
//! response headers (3), response body (4, audit only) and logging (5, audit
//! only). Each test reports whether the upstream was contacted, and what it
//! received, so "never contacted" is checked rather than assumed.
use std::sync::Arc;
use std::time::Duration;

use arc_swap::ArcSwap;
use pingora::apps::ServerApp;
use pingora::protocols::l4::stream::Stream;
use prx::{
    acme::ChallengeStore, config::PrxConfig, proxy::PrxProxy, runtime::RuntimeConfig,
    waf::evaluator::audit,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// What the upstream saw and what the client got back.
struct Outcome {
    response: String,
    /// `Some(request bytes)` when the upstream was contacted, `None` otherwise.
    upstream_request: Option<Vec<u8>>,
}

impl Outcome {
    fn contacted(&self) -> bool {
        self.upstream_request.is_some()
    }
}

/// Builds the runtime from `rules` (and optional `[waf]` keys), serves the
/// upstream with `upstream_status_line` and `upstream_body`, then sends `raw`.
async fn run(
    waf_keys: &str,
    rules: &str,
    raw: Vec<u8>,
    upstream_status_line: &str,
    upstream_body: &'static str,
) -> Outcome {
    let upstream = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("rules.conf"), rules).unwrap();
    let config_path = dir.path().join("Prx.toml");
    std::fs::write(
        &config_path,
        format!(
            r#"
[waf]
rules_file = "rules.conf"
{waf_keys}

[[service]]
name = "app"
[[service.upstream]]
addr = "{}"

[[route]]
name = "app"
service = "app"
is_default = true
"#,
            upstream.local_addr().unwrap()
        ),
    )
    .unwrap();
    let config = PrxConfig::from_file(&config_path).expect("config loads");
    let runtime = RuntimeConfig::from_config(config);

    let proxy = PrxProxy::new(
        Arc::new(ArcSwap::from_pointee(runtime)),
        false,
        "/healthz".into(),
        "/readyz".into(),
        Default::default(),
        Arc::new(ChallengeStore::new()),
    );
    let app = Arc::new(pingora::proxy::http_proxy(&Arc::default(), proxy));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let mut client = tokio::net::TcpStream::connect(listener.local_addr().unwrap())
        .await
        .unwrap();
    let (socket, _) = listener.accept().await.unwrap();
    let (_shutdown, watch) = tokio::sync::watch::channel(false);
    let task = tokio::spawn(async move {
        app.process_new(Box::new(Stream::from(socket)), &watch)
            .await;
    });

    let status_line = upstream_status_line.to_string();
    let upstream_task = tokio::spawn(async move {
        let accepted = tokio::time::timeout(Duration::from_millis(800), upstream.accept()).await;
        let Ok(Ok((mut stream, _))) = accepted else {
            return None;
        };
        // Read until the request header and its declared body are complete.
        let mut received = Vec::new();
        let mut buf = [0u8; 4096];
        loop {
            let read =
                tokio::time::timeout(Duration::from_millis(300), stream.read(&mut buf)).await;
            match read {
                Ok(Ok(0)) | Err(_) => break,
                Ok(Ok(n)) => {
                    received.extend_from_slice(&buf[..n]);
                    if request_complete(&received) {
                        break;
                    }
                }
                Ok(Err(_)) => break,
            }
        }
        let response = format!(
            "{status_line}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{upstream_body}",
            upstream_body.len()
        );
        let _ = stream.write_all(response.as_bytes()).await;
        Some(received)
    });

    client.write_all(&raw).await.unwrap();
    let mut response = String::new();
    tokio::time::timeout(Duration::from_secs(5), client.read_to_string(&mut response))
        .await
        .unwrap()
        .unwrap_or_default();
    task.await.unwrap();
    let upstream_request = upstream_task.await.unwrap();
    Outcome {
        response,
        upstream_request,
    }
}

/// True once the bytes hold a full header block plus `Content-Length` bytes.
fn request_complete(received: &[u8]) -> bool {
    let Some(end) = received.windows(4).position(|w| w == b"\r\n\r\n") else {
        return false;
    };
    let head = String::from_utf8_lossy(&received[..end]).to_ascii_lowercase();
    let length = head
        .lines()
        .find_map(|line| line.strip_prefix("content-length:"))
        .and_then(|value| value.trim().parse::<usize>().ok())
        .unwrap_or(0);
    received.len() >= end + 4 + length
}

fn post(body: &[u8]) -> Vec<u8> {
    let mut raw = format!(
        "POST /submit HTTP/1.1\r\nHost: test\r\nContent-Type: application/x-www-form-urlencoded\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    )
    .into_bytes();
    raw.extend_from_slice(body);
    raw
}

const OK_LINE: &str = "HTTP/1.1 200 OK";

#[tokio::test]
async fn phase2_body_deny_returns_403_without_contacting_upstream() {
    let rules = concat!(
        "SecRequestBodyAccess On\n",
        "SecRequestBodyLimit 65536\n",
        "SecRule REQUEST_BODY \"@contains ' OR '1'='1\" \"id:200,phase:2,deny,status:403\"\n",
    );
    let attack = post(b"user=admin' OR '1'='1");
    let out = run("", rules, attack, OK_LINE, "ok").await;
    assert!(out.response.starts_with("HTTP/1.1 403"), "{}", out.response);
    assert!(!out.contacted(), "phase 2 denied request reached upstream");

    // Control: the same route with a benign body is forwarded and answered.
    let benign = post(b"user=admin");
    let out = run("", rules, benign, OK_LINE, "ok").await;
    assert!(out.response.starts_with("HTTP/1.1 200"), "{}", out.response);
    assert!(out.contacted(), "benign body must reach upstream");
    let received = out.upstream_request.unwrap();
    let text = String::from_utf8_lossy(&received).into_owned();
    assert_eq!(
        text.matches("user=admin").count(),
        1,
        "body must reach upstream exactly once, got {text:?}"
    );
    assert!(
        text.ends_with("user=admin"),
        "body must be last, got {text:?}"
    );
}

#[tokio::test]
async fn phase2_body_over_limit_returns_413_without_contacting_upstream() {
    let rules = "SecRequestBodyAccess On\nSecRequestBodyLimit 16\n\
                 SecRule ARGS:attack \"@streq yes\" \"id:201,phase:1,pass,nolog\"\n\
                 SecRule REQUEST_BODY \"@contains never\" \"id:202,phase:2,deny\"\n";
    let big = post(&[b'a'; 64]);
    let out = run("", rules, big, OK_LINE, "ok").await;
    assert!(out.response.starts_with("HTTP/1.1 413"), "{}", out.response);
    assert!(!out.contacted(), "oversized body reached upstream");
}

#[tokio::test]
async fn phase3_response_header_deny_replaces_upstream_response() {
    let rules =
        "SecRule RESPONSE_HEADERS:x-secret \"@streq leak\" \"id:300,phase:3,deny,status:502\"\n";
    let out = run(
        "",
        rules,
        get_request(),
        "HTTP/1.1 200 OK\r\nX-Secret: leak",
        "upstream-body",
    )
    .await;
    assert!(out.contacted(), "phase 3 runs after the upstream responded");
    assert!(out.response.starts_with("HTTP/1.1 502"), "{}", out.response);
    assert!(
        !out.response.contains("upstream-body"),
        "upstream body leaked: {}",
        out.response
    );
    assert!(out.response.contains("forbidden"), "{}", out.response);
}

#[tokio::test]
async fn phase3_benign_response_header_passes_through() {
    let rules =
        "SecRule RESPONSE_HEADERS:x-secret \"@streq leak\" \"id:300,phase:3,deny,status:502\"\n";
    let out = run("", rules, get_request(), OK_LINE, "upstream-body").await;
    assert!(out.response.starts_with("HTTP/1.1 200"), "{}", out.response);
    assert!(out.response.contains("upstream-body"), "{}", out.response);
}

#[tokio::test]
async fn phase4_matches_response_body_but_body_is_delivered_unchanged() {
    // The rule matches the upstream body, but phase 4 is audit only, so the
    // client must still receive the body byte for byte.
    let rules =
        "SecRule RESPONSE_BODY \"@contains secret-token\" \"id:400,phase:4,deny,status:500\"\n";
    let before = audit::phase4_matches();
    let out = run(
        "",
        rules,
        get_request(),
        OK_LINE,
        "contains secret-token here",
    )
    .await;
    assert!(out.response.starts_with("HTTP/1.1 200"), "{}", out.response);
    assert!(
        out.response.ends_with("contains secret-token here"),
        "phase 4 altered the body: {}",
        out.response
    );
    assert!(out.contacted());
    assert_eq!(
        audit::phase4_matches(),
        before + 1,
        "phase 4 rule did not match the response body"
    );
}

#[tokio::test]
async fn phase5_logging_runs_for_allowed_and_blocked_requests() {
    // Phase 5 is audit only: it must run for every routed request and never
    // change the outcome. The runs are counted from the real log output.
    // A phase 5 rule must exist, or the audit hook is correctly skipped and the
    // test would prove nothing. It is `pass`, so it never changes an outcome.
    let rules = concat!(
        "SecRule ARGS:attack \"@streq yes\" \"id:1,phase:1,deny\"\n",
        "SecRule REQUEST_HEADERS:host \"@streq test\" \"id:501,phase:5,pass,nolog\"\n",
    );

    let before = audit::phase5_runs();
    let allowed = run("", rules, get_request(), OK_LINE, "ok").await;
    assert!(
        allowed.response.starts_with("HTTP/1.1 200"),
        "{}",
        allowed.response
    );
    assert_eq!(
        audit::phase5_runs(),
        before + 1,
        "phase 5 did not run for the allowed request"
    );

    let before = audit::phase5_runs();
    let blocked = run(
        "",
        rules,
        get_request_with_query("attack=yes"),
        OK_LINE,
        "ok",
    )
    .await;
    assert!(
        blocked.response.starts_with("HTTP/1.1 403"),
        "{}",
        blocked.response
    );
    assert!(!blocked.contacted());
    assert_eq!(
        audit::phase5_runs(),
        before + 1,
        "phase 5 did not run for the blocked request"
    );
}

/// T513: the anomaly score a phase-1 rule adds must reach the phase-2 rule that
/// compares it with the threshold. Phase 1 scores every request by +5, and the
/// phase-2 rule (which reads the body) denies once the score meets the
/// threshold. Each phase used to start from an empty transaction, so the
/// threshold never fired at the final phase.
#[tokio::test]
async fn phase2_denies_on_score_accumulated_in_phase1() {
    let rules = concat!(
        "SecRequestBodyAccess On\n",
        "SecRequestBodyLimit 65536\n",
        "SecAction \"id:300,phase:1,pass,nolog,setvar:tx.blocking_inbound_anomaly_score=+5\"\n",
        "SecRule TX:BLOCKING_INBOUND_ANOMALY_SCORE \"@ge %{tx.inbound_anomaly_score_threshold}\" \"id:301,phase:2,deny,status:403,nolog\"\n",
    );
    // Default threshold is 5: a score of 5 meets it, so the request is denied.
    let out = run("", rules, post(b"user=admin"), OK_LINE, "ok").await;
    assert!(out.response.starts_with("HTTP/1.1 403"), "{}", out.response);
    assert!(!out.contacted(), "phase 2 denied request reached upstream");

    // Control: the same rules with the threshold above the score are forwarded.
    let out = run(
        "inbound_anomaly_threshold = 10",
        rules,
        post(b"user=admin"),
        OK_LINE,
        "ok",
    )
    .await;
    assert!(out.response.starts_with("HTTP/1.1 200"), "{}", out.response);
    assert!(
        out.contacted(),
        "below-threshold request must reach upstream"
    );
}

fn get_request() -> Vec<u8> {
    b"GET / HTTP/1.1\r\nHost: test\r\nConnection: close\r\n\r\n".to_vec()
}

fn get_request_with_query(query: &str) -> Vec<u8> {
    format!("GET /?{query} HTTP/1.1\r\nHost: test\r\nConnection: close\r\n\r\n").into_bytes()
}
