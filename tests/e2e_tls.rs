//! T111: TLS listener and SNI certificate selection, through the real binary.
//!
//! These tests drive the `openssl` CLI rather than a Rust TLS client, so they
//! check what a real client sees on the wire, including which certificate the
//! server chose for a given SNI.

mod common;

use std::{
    path::{Path, PathBuf},
    process::Command,
};

use tempfile::TempDir;

use common::{PrxProcess, UpstreamServer, reserve_port, write_config};

/// Creates a self-signed certificate for `cn` inside `dir`, returning
/// (cert_path, key_path). Skips the suite if openssl is unavailable.
fn make_cert(dir: &Path, name: &str, cn: &str) -> (PathBuf, PathBuf) {
    let cert = dir.join(format!("{name}.crt"));
    let key = dir.join(format!("{name}.key"));

    let status = Command::new("openssl")
        .args([
            "req",
            "-x509",
            "-newkey",
            "rsa:2048",
            "-keyout",
            key.to_str().expect("key path"),
            "-out",
            cert.to_str().expect("cert path"),
            "-days",
            "30",
            "-nodes",
            "-subj",
            &format!("/CN={cn}"),
            "-addext",
            &format!("subjectAltName=DNS:{cn}"),
        ])
        .output()
        .expect("failed to run openssl; it is required for the TLS tests");
    assert!(
        status.status.success(),
        "openssl could not generate a certificate: {}",
        String::from_utf8_lossy(&status.stderr)
    );

    (cert, key)
}

/// Returns the subject of the certificate the server presents for `sni`.
fn presented_subject(port: u16, sni: &str) -> String {
    let s_client = Command::new("openssl")
        .args([
            "s_client",
            "-connect",
            &format!("127.0.0.1:{port}"),
            "-servername",
            sni,
        ])
        .stdin(std::process::Stdio::null())
        .output()
        .expect("failed to run openssl s_client");

    let text = String::from_utf8_lossy(&s_client.stdout);
    let pem_start = text.find("-----BEGIN CERTIFICATE-----");
    let pem_end = text.find("-----END CERTIFICATE-----");
    let (Some(start), Some(end)) = (pem_start, pem_end) else {
        panic!("no certificate was presented for {sni}:\n{text}");
    };
    let pem = &text[start..end + "-----END CERTIFICATE-----".len()];

    let mut child = Command::new("openssl")
        .args(["x509", "-noout", "-subject"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .expect("failed to run openssl x509");
    {
        use std::io::Write;
        let stdin = child.stdin.as_mut().expect("stdin");
        stdin.write_all(pem.as_bytes()).expect("write pem");
    }
    let out = child.wait_with_output().expect("x509 output");
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

struct Harness {
    _prx: PrxProcess,
    _upstream: UpstreamServer,
    _tmp: TempDir,
    tls_port: u16,
    plain_port: u16,
}

fn start() -> Harness {
    let tmp = TempDir::new().expect("failed to create temp dir");
    let (a_cert, a_key) = make_cert(tmp.path(), "a", "a.example.com");
    let (b_cert, b_key) = make_cert(tmp.path(), "b", "b.example.com");
    let (w_cert, w_key) = make_cert(tmp.path(), "wild", "*.wild.example.com");

    let upstream_port = reserve_port();
    let upstream = UpstreamServer::spawn(upstream_port, "hello over tls");
    let plain_port = reserve_port();
    let tls_port = reserve_port();

    let cfg = format!(
        r#"[server]
listen = ["127.0.0.1:{plain_port}"]

[server.tls]
listen = "127.0.0.1:{tls_port}"
enable_h2 = true

[[server.tls.cert]]
domains = ["a.example.com"]
cert_path = "{a_cert}"
key_path = "{a_key}"
is_default = true

[[server.tls.cert]]
domains = ["b.example.com"]
cert_path = "{b_cert}"
key_path = "{b_key}"

[[server.tls.cert]]
domains = ["*.wild.example.com"]
cert_path = "{w_cert}"
key_path = "{w_key}"

[observability]
log_level = "error"
access_log = false

[[service]]
name = "app"

[[service.upstream]]
addr = "127.0.0.1:{upstream_port}"

[[route]]
name = "app"
service = "app"
path_prefix = "/"
is_default = true
"#,
        a_cert = a_cert.display(),
        a_key = a_key.display(),
        b_cert = b_cert.display(),
        b_key = b_key.display(),
        w_cert = w_cert.display(),
        w_key = w_key.display(),
    );

    let cfg_path = write_config(&tmp, &cfg);
    let prx = PrxProcess::spawn(&cfg_path, reserve_port());
    prx.wait_until_listening(tls_port);

    Harness {
        _prx: prx,
        _upstream: upstream,
        _tmp: tmp,
        tls_port,
        plain_port,
    }
}

#[test]
fn sni_selects_the_matching_certificate() {
    let harness = start();

    assert!(
        presented_subject(harness.tls_port, "a.example.com").contains("a.example.com"),
        "wrong certificate for a.example.com"
    );
    assert!(
        presented_subject(harness.tls_port, "b.example.com").contains("b.example.com"),
        "wrong certificate for b.example.com"
    );
    assert!(
        presented_subject(harness.tls_port, "anything.wild.example.com")
            .contains("wild.example.com"),
        "the wildcard certificate should cover a subdomain"
    );
    // An unknown name falls back to the certificate marked default rather than
    // failing the handshake.
    assert!(
        presented_subject(harness.tls_port, "unknown.test").contains("a.example.com"),
        "an unknown SNI should get the default certificate"
    );
}

/// A TLS listener must serve both HTTP/1.1 and HTTP/2 clients. Getting this
/// wrong is silent: the handshake succeeds and the request then fails.
#[test]
fn both_http_versions_work_over_tls() {
    let harness = start();

    for (args, label) in [(vec!["--http1.1"], "HTTP/1.1"), (vec!["--http2"], "HTTP/2")] {
        let output = Command::new("curl")
            .args(["-sk"])
            .args(&args)
            .arg(format!("https://127.0.0.1:{}/", harness.tls_port))
            .args(["-H", "Host: app.local"])
            .output()
            .expect("failed to run curl");
        let body = String::from_utf8_lossy(&output.stdout);
        assert!(
            body.contains("hello over tls"),
            "{label} over TLS failed, got: {body:?}"
        );
    }
}

/// The plaintext listener keeps working alongside TLS, including cleartext
/// HTTP/2 for gRPC clients.
#[test]
fn the_plaintext_listener_still_serves_both_versions() {
    let harness = start();

    for (args, label) in [
        (vec![], "HTTP/1.1"),
        (vec!["--http2-prior-knowledge"], "h2c"),
    ] {
        let output = Command::new("curl")
            .arg("-s")
            .args(&args)
            .arg(format!("http://127.0.0.1:{}/", harness.plain_port))
            .args(["-H", "Host: app.local"])
            .output()
            .expect("failed to run curl");
        let body = String::from_utf8_lossy(&output.stdout);
        assert!(
            body.contains("hello over tls"),
            "{label} on the plaintext listener failed, got: {body:?}"
        );
    }
}

/// A key that does not match its certificate breaks every handshake, so it must
/// stop startup instead.
#[test]
fn a_mismatched_key_is_rejected_at_startup() {
    let tmp = TempDir::new().expect("failed to create temp dir");
    let (cert, _key) = make_cert(tmp.path(), "a", "a.example.com");
    let (_other_cert, other_key) = make_cert(tmp.path(), "b", "b.example.com");

    let upstream_port = reserve_port();
    let plain_port = reserve_port();
    let tls_port = reserve_port();

    let cfg = format!(
        r#"[server]
listen = ["127.0.0.1:{plain_port}"]

[server.tls]
listen = "127.0.0.1:{tls_port}"

[[server.tls.cert]]
domains = ["a.example.com"]
cert_path = "{cert}"
key_path = "{other_key}"

[observability]
log_level = "error"
access_log = false

[[service]]
name = "app"

[[service.upstream]]
addr = "127.0.0.1:{upstream_port}"

[[route]]
name = "app"
service = "app"
path_prefix = "/"
is_default = true
"#,
        cert = cert.display(),
        other_key = other_key.display(),
    );
    let cfg_path = write_config(&tmp, &cfg);

    let output = Command::new(common::prx_binary())
        .env("PRX_CONFIG", &cfg_path)
        .env("PRX_ADMIN_LISTEN", format!("127.0.0.1:{}", reserve_port()))
        .env("RUST_LOG", "error")
        .output()
        .expect("failed to run prx");

    assert!(
        !output.status.success(),
        "prx started with a key that does not match its certificate"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("does not match"),
        "the error should say what is wrong, got: {stderr}"
    );
}
