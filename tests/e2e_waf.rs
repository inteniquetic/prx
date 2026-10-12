//! T513: real Pingora request pipeline and TCP upstream, with an injected ruleset.
//! Configuration loading and phase scheduling are separate WAF tasks.
use std::sync::Arc;
use std::time::Duration;

use arc_swap::ArcSwap;
use pingora::apps::ServerApp;
use pingora::protocols::l4::stream::Stream;
use prx::{acme::ChallengeStore, config::PrxConfig, proxy::PrxProxy, runtime::RuntimeConfig};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

async fn exercise(rules: &str, attack: bool) -> (String, bool) {
    let upstream = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let config = PrxConfig::from_toml_str(&format!(
        r#"
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
    ))
    .unwrap();
    let mut runtime = RuntimeConfig::from_config(config);
    let loaded = prx::waf::load_text(&[("test.conf", rules)]);
    assert_eq!(loaded.summary.skipped, 0, "{loaded}");
    runtime.ruleset = Arc::new(loaded.ruleset);
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
    let upstream_task = tokio::spawn(async move {
        if let Ok(Ok((mut stream, _))) =
            tokio::time::timeout(Duration::from_millis(500), upstream.accept()).await
        {
            let mut request = [0; 4096];
            let _read = stream.read(&mut request).await.unwrap();
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok")
                .await
                .unwrap();
            true
        } else {
            false
        }
    });
    let path = if attack { "/?attack=yes" } else { "/" };
    client
        .write_all(
            format!("GET {path} HTTP/1.1\r\nHost: test\r\nConnection: close\r\n\r\n").as_bytes(),
        )
        .await
        .unwrap();
    let mut response = String::new();
    tokio::time::timeout(Duration::from_secs(5), client.read_to_string(&mut response))
        .await
        .unwrap()
        .unwrap();
    task.await.unwrap();
    (response, upstream_task.await.unwrap())
}

#[tokio::test]
async fn disruptive_defaults_and_modes_are_enforced() {
    for (action, status) in [
        ("deny,status:451", "451"),
        ("redirect:/safe", "302"),
        ("drop", ""),
    ] {
        let rules = format!(
            "SecDefaultAction \"phase:1,{action}\"\nSecRule ARGS:attack \"@streq yes\" \"id:1,phase:1,block\""
        );
        let (response, contacted) = exercise(&rules, true).await;
        assert!(!contacted, "{action} contacted upstream");
        if status.is_empty() {
            assert!(
                response.is_empty(),
                "drop sent an HTTP response: {response}"
            );
        } else {
            assert!(
                response.starts_with(&format!("HTTP/1.1 {status}")),
                "{response}"
            );
        }
        if status == "302" {
            assert!(response.to_ascii_lowercase().contains("location: /safe"));
        }
    }
    for mode in ["DetectionOnly", "Off"] {
        let rules = format!(
            "SecRuleEngine {mode}\nSecRule ARGS:attack \"@streq yes\" \"id:1,phase:1,deny\""
        );
        let (response, contacted) = exercise(&rules, true).await;
        assert!(response.starts_with("HTTP/1.1 200"), "{mode}: {response}");
        assert!(contacted);
    }
}

#[tokio::test]
async fn deny_returns_403_without_contacting_upstream() {
    let rules = r#"SecRule ARGS:attack "@streq yes" "id:1,phase:1,deny""#;
    let (response, contacted) = exercise(rules, true).await;
    assert!(response.starts_with("HTTP/1.1 403"), "{response}");
    assert!(!contacted, "denied request contacted upstream");
    let (response, contacted) = exercise(rules, false).await;
    assert!(response.starts_with("HTTP/1.1 200"), "{response}");
    assert!(contacted, "benign request must reach upstream");
}

/// T-wiring: a `[waf] rules_file` reaches the running proxy. The rule comes
/// from a file, loaded by `RuntimeConfig::from_config` the way startup and
/// hot-reload do it, not injected into the ruleset by the test.
#[tokio::test]
async fn rules_file_deny_returns_403_without_contacting_upstream() {
    let upstream = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("rules.conf"),
        "SecRule ARGS:attack \"@streq yes\" \"id:1,phase:1,deny\"\n",
    )
    .unwrap();
    let config_path = dir.path().join("Prx.toml");
    std::fs::write(
        &config_path,
        format!(
            r#"
[waf]
rules_file = "rules.conf"

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
    let config = PrxConfig::from_file(&config_path).expect("config with rules_file loads");
    let runtime = RuntimeConfig::from_config(config);
    assert!(
        !runtime.ruleset.phases[0].is_empty(),
        "rules_file was not loaded"
    );

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
    let upstream_task = tokio::spawn(async move {
        matches!(
            tokio::time::timeout(Duration::from_millis(500), upstream.accept()).await,
            Ok(Ok(_))
        )
    });
    client
        .write_all(b"GET /?attack=yes HTTP/1.1\r\nHost: test\r\nConnection: close\r\n\r\n")
        .await
        .unwrap();
    let mut response = String::new();
    tokio::time::timeout(Duration::from_secs(5), client.read_to_string(&mut response))
        .await
        .unwrap()
        .unwrap();
    task.await.unwrap();
    assert!(response.starts_with("HTTP/1.1 403"), "{response}");
    assert!(
        !upstream_task.await.unwrap(),
        "denied request contacted upstream"
    );
}
