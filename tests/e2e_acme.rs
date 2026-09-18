//! T112: automatic certificate issuance over ACME, end to end.
//!
//! The test drives a real ACME server (Pebble) plus its DNS helper
//! (`pebble-challtestsrv`) so the whole path is exercised: account
//! registration, the HTTP-01 challenge served by prx's own listener,
//! finalization, and the hot swap of the issued certificate into the running
//! TLS listener without a restart.
//!
//! Pebble is not a build dependency, so the suite skips itself when the
//! binaries are absent. To run it:
//!
//! ```sh
//! go install github.com/letsencrypt/pebble/v2/cmd/pebble@latest
//! go install github.com/letsencrypt/pebble/v2/cmd/pebble-challtestsrv@latest
//! PATH="$(go env GOPATH)/bin:$PATH" cargo test --test e2e_acme
//! ```

mod common;

use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use tempfile::TempDir;

use common::{PrxProcess, UpstreamServer, reserve_port, send_get, write_config};

const DOMAIN: &str = "prx-test.example.com";

/// Looks up a Pebble binary on `PATH`, or under `$PRX_PEBBLE_BIN` when the
/// binaries were installed outside the shell's `PATH`.
fn pebble_binary(name: &str) -> Option<PathBuf> {
    if let Ok(dir) = std::env::var("PRX_PEBBLE_BIN") {
        let candidate = Path::new(&dir).join(name);
        if candidate.exists() {
            return Some(candidate);
        }
    }

    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(name))
        .find(|candidate| candidate.exists())
}

/// A tiny CA plus a leaf certificate for Pebble's own HTTPS interface.
/// Returns (leaf cert, leaf key, CA certificate). prx is pointed at the CA
/// through `ca_root_path`, which is exactly how a private ACME CA (step-ca,
/// Pebble, an internal Boulder) is configured in production.
///
/// A self-signed certificate will not do here: a TLS client rejects a CA
/// certificate presented as the end-entity certificate, so the CA and the leaf
/// have to be two different certificates.
fn make_server_cert(dir: &Path) -> (PathBuf, PathBuf, PathBuf) {
    let ca_cert = dir.join("ca.crt");
    let ca_key = dir.join("ca.key");
    let cert = dir.join("pebble.crt");
    let key = dir.join("pebble.key");
    let csr = dir.join("pebble.csr");
    let ext = dir.join("pebble.ext");
    fs::write(&ext, "subjectAltName=DNS:localhost,IP:127.0.0.1\n")
        .expect("failed to write the certificate extensions");

    run_openssl(&[
        "req",
        "-x509",
        "-newkey",
        "rsa:2048",
        "-keyout",
        ca_key.to_str().expect("ca key path"),
        "-out",
        ca_cert.to_str().expect("ca cert path"),
        "-days",
        "2",
        "-nodes",
        "-subj",
        "/CN=prx test CA",
    ]);
    run_openssl(&[
        "req",
        "-newkey",
        "rsa:2048",
        "-keyout",
        key.to_str().expect("key path"),
        "-out",
        csr.to_str().expect("csr path"),
        "-nodes",
        "-subj",
        "/CN=localhost",
    ]);
    run_openssl(&[
        "x509",
        "-req",
        "-in",
        csr.to_str().expect("csr path"),
        "-CA",
        ca_cert.to_str().expect("ca cert path"),
        "-CAkey",
        ca_key.to_str().expect("ca key path"),
        "-CAcreateserial",
        "-out",
        cert.to_str().expect("cert path"),
        "-days",
        "2",
        "-extfile",
        ext.to_str().expect("ext path"),
    ]);

    (cert, key, ca_cert)
}

fn run_openssl(args: &[&str]) {
    let out = Command::new("openssl")
        .args(args)
        .output()
        .expect("failed to run openssl; it is required for the ACME tests");
    assert!(
        out.status.success(),
        "openssl {:?} failed: {}",
        args.first(),
        String::from_utf8_lossy(&out.stderr)
    );
}

/// Owns the two Pebble processes so they are reaped when a test ends, however
/// it ends.
struct Pebble {
    challtestsrv: Child,
    server: Child,
    directory_url: String,
    ca_root: PathBuf,
    /// The port Pebble reaches for HTTP-01 validation; prx listens on it.
    http01_port: u16,
}

impl Drop for Pebble {
    fn drop(&mut self) {
        let _ = self.server.kill();
        let _ = self.server.wait();
        let _ = self.challtestsrv.kill();
        let _ = self.challtestsrv.wait();
    }
}

/// Starts Pebble and its DNS helper on free ports. Returns `None` when the
/// binaries are not installed, which skips the suite instead of failing it.
fn start_pebble(tmp: &Path) -> Option<Pebble> {
    let pebble = pebble_binary("pebble")?;
    let challtestsrv = pebble_binary("pebble-challtestsrv")?;

    let dir_port = reserve_port();
    let mgmt_port = reserve_port();
    let dns_port = reserve_port();
    let chall_mgmt_port = reserve_port();
    let tls_alpn_port = reserve_port();
    let http01_port = reserve_port();

    let (cert, key, ca_cert) = make_server_cert(tmp);

    // Resolve every name to 127.0.0.1 and nothing to IPv6: the container may
    // have no IPv6 route, and Pebble would otherwise dial ::1 and time out.
    let challtestsrv = Command::new(challtestsrv)
        .args([
            "-dnsserver",
            &format!("127.0.0.1:{dns_port}"),
            "-management",
            &format!("127.0.0.1:{chall_mgmt_port}"),
            "-defaultIPv4",
            "127.0.0.1",
            "-defaultIPv6",
            "",
            "-http01",
            "",
            "-https01",
            "",
            "-tlsalpn01",
            "",
            "-doh",
            "",
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("failed to start pebble-challtestsrv");

    let config_path = tmp.join("pebble-config.json");
    let config = format!(
        r#"{{
  "pebble": {{
    "listenAddress": "127.0.0.1:{dir_port}",
    "managementListenAddress": "127.0.0.1:{mgmt_port}",
    "certificate": "{cert}",
    "privateKey": "{key}",
    "httpPort": {http01_port},
    "tlsPort": {tls_alpn_port},
    "ocspResponderURL": "",
    "externalAccountBindingRequired": false
  }}
}}"#,
        cert = cert.display(),
        key = key.display(),
    );
    fs::write(&config_path, config).expect("failed to write the Pebble config");

    let server = Command::new(pebble)
        .args([
            "-config",
            config_path.to_str().expect("config path"),
            "-dnsserver",
            &format!("127.0.0.1:{dns_port}"),
        ])
        // Keep the run deterministic: no random validation sleeps and no
        // deliberate nonce rejections.
        .env("PEBBLE_VA_NOSLEEP", "1")
        .env("PEBBLE_WFE_NONCEREJECT", "0")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("failed to start pebble");

    let handle = Pebble {
        challtestsrv,
        server,
        directory_url: format!("https://127.0.0.1:{dir_port}/dir"),
        ca_root: ca_cert,
        http01_port,
    };

    wait_for_tcp(dir_port, Duration::from_secs(10));
    Some(handle)
}

fn wait_for_tcp(port: u16, timeout: Duration) {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if std::net::TcpStream::connect(("127.0.0.1", port)).is_ok() {
            return;
        }
        thread::sleep(Duration::from_millis(50));
    }
    panic!("nothing started listening on port {port} within {timeout:?}");
}

/// Reads `/web/tls/status` off the admin API.
fn tls_status(admin_port: u16) -> String {
    let response = send_get(admin_port, "127.0.0.1", "/web/tls/status");
    let (_, body) = response
        .split_once("\r\n\r\n")
        .expect("admin response had no body");
    body.to_string()
}

/// Polls the admin API until ACME reports a success, so the test never sleeps
/// for a fixed guess at how long issuance takes.
fn wait_for_issuance(admin_port: u16) -> String {
    let deadline = Instant::now() + Duration::from_secs(60);
    let mut last = String::new();
    while Instant::now() < deadline {
        last = tls_status(admin_port);
        if last.contains("\"last_success_epoch_s\":")
            && !last.contains("\"last_success_epoch_s\":null")
        {
            return last;
        }
        thread::sleep(Duration::from_millis(200));
    }
    panic!("ACME never reported a successful issuance; last status was: {last}");
}

/// Returns the certificate chain prx presents for `sni`, as openssl prints it.
fn presented_chain(port: u16, sni: &str) -> String {
    let out = Command::new("openssl")
        .args([
            "s_client",
            "-connect",
            &format!("127.0.0.1:{port}"),
            "-servername",
            sni,
            "-showcerts",
        ])
        .stdin(Stdio::null())
        .output()
        .expect("failed to run openssl s_client");
    String::from_utf8_lossy(&out.stdout).to_string()
}

/// Pipes a PEM certificate through `openssl x509` with the given flags.
fn describe_cert(pem: &str, args: &[&str]) -> String {
    let mut child = Command::new("openssl")
        .arg("x509")
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("failed to run openssl x509");
    child
        .stdin
        .as_mut()
        .expect("stdin")
        .write_all(pem.as_bytes())
        .expect("failed to write the certificate");
    let out = child.wait_with_output().expect("openssl x509 output");
    String::from_utf8_lossy(&out.stdout).to_string()
}

fn first_pem(text: &str) -> String {
    let start = text
        .find("-----BEGIN CERTIFICATE-----")
        .expect("no certificate in the output");
    let end = text
        .find("-----END CERTIFICATE-----")
        .expect("truncated certificate in the output");
    text[start..end + "-----END CERTIFICATE-----".len()].to_string()
}

#[test]
fn acme_issues_and_installs_a_certificate_without_a_restart() {
    let tmp = TempDir::new().expect("failed to create temp dir");
    let Some(pebble) = start_pebble(tmp.path()) else {
        eprintln!(
            "skipping: pebble and pebble-challtestsrv are not installed \
             (see the module docs for the two `go install` lines)"
        );
        return;
    };

    let upstream_port = reserve_port();
    let _upstream = UpstreamServer::spawn(upstream_port, "hello from acme");
    let tls_port = reserve_port();
    let admin_port = reserve_port();
    let storage = tmp.path().join("acme");

    // The plaintext listener runs on the port Pebble validates against, which
    // is what makes the HTTP-01 responder inside prx the thing under test.
    let cfg = format!(
        r#"[server]
listen = ["127.0.0.1:{plain}"]

[server.tls]
listen = "127.0.0.1:{tls_port}"

[server.tls.acme]
enabled = true
email = ["ops@example.com"]
directory_url = "{directory}"
domains = ["{DOMAIN}"]
storage_dir = "{storage}"
ca_root_path = "{ca_root}"
renew_before_days = 30

[observability]
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
        plain = pebble.http01_port,
        directory = pebble.directory_url,
        storage = storage.display(),
        ca_root = pebble.ca_root.display(),
    );

    let config_path = write_config(&tmp, &cfg);
    let prx = PrxProcess::spawn(&config_path, admin_port);
    prx.wait_until_listening(pebble.http01_port);
    prx.wait_until_listening(tls_port);

    let status = wait_for_issuance(admin_port);
    assert!(
        status.contains("\"last_error\":null"),
        "issuance succeeded but an error was still reported: {status}"
    );
    assert!(
        status.contains(DOMAIN),
        "the issued certificate was not reported as being served: {status}"
    );

    // The certificate and its key are on disk, and the key is not world
    // readable.
    let cert_pem = fs::read_to_string(storage.join("cert.pem")).expect("cert.pem was not written");
    assert!(
        storage.join("key.pem").exists(),
        "key.pem was not written next to the certificate"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        for name in ["key.pem", "account.json"] {
            let mode = fs::metadata(storage.join(name))
                .expect("secret file missing")
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(mode, 0o600, "{name} should be readable only by prx");
        }
    }
    assert_eq!(
        cert_pem.matches("-----BEGIN CERTIFICATE-----").count(),
        2,
        "the stored certificate should carry its issuing chain, not just the leaf"
    );

    // The running listener serves the freshly issued certificate: prx was
    // never restarted after startup, so this is the hot swap working.
    let chain = presented_chain(tls_port, DOMAIN);
    let leaf = first_pem(&chain);
    let text = describe_cert(&leaf, &["-noout", "-text"]);
    assert!(
        text.contains(&format!("DNS:{DOMAIN}")),
        "the presented certificate is not for {DOMAIN}:\n{text}"
    );
    assert!(
        text.contains("Pebble Intermediate CA"),
        "the presented certificate was not issued by the ACME server:\n{text}"
    );

    // An unknown challenge token is a 404 rather than a hang or a proxied
    // request, so a stale validation attempt cannot reach an upstream.
    let response = send_get(
        pebble.http01_port,
        DOMAIN,
        "/.well-known/acme-challenge/not-a-real-token",
    );
    assert!(
        response.starts_with("HTTP/1.1 404"),
        "an unknown ACME challenge token should 404, got: {response}"
    );

    // Ordinary traffic still flows on the same listener.
    let response = send_get(pebble.http01_port, DOMAIN, "/");
    assert!(
        response.contains("hello from acme"),
        "the HTTP-01 responder should not shadow normal routing: {response}"
    );
}

#[test]
fn acme_failure_keeps_the_proxy_serving() {
    let tmp = TempDir::new().expect("failed to create temp dir");
    let upstream_port = reserve_port();
    let _upstream = UpstreamServer::spawn(upstream_port, "still serving");
    let plain_port = reserve_port();
    let tls_port = reserve_port();
    let admin_port = reserve_port();
    // Nothing is listening here, so every ACME attempt fails to connect.
    let dead_acme_port = reserve_port();
    let storage = tmp.path().join("acme");

    let cfg = format!(
        r#"[server]
listen = ["127.0.0.1:{plain_port}"]

[server.tls]
listen = "127.0.0.1:{tls_port}"

[server.tls.acme]
enabled = true
directory_url = "https://127.0.0.1:{dead_acme_port}/dir"
domains = ["{DOMAIN}"]
storage_dir = "{storage}"

[observability]
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
        storage = storage.display(),
    );

    let config_path = write_config(&tmp, &cfg);
    let prx = PrxProcess::spawn(&config_path, admin_port);
    prx.wait_until_listening(plain_port);

    // The ACME loop is failing in the background; requests keep being served.
    let response = send_get(plain_port, DOMAIN, "/");
    assert!(
        response.contains("still serving"),
        "a failing ACME loop must not take the proxy down: {response}"
    );

    // And the failure is visible rather than silent.
    let deadline = Instant::now() + Duration::from_secs(20);
    let mut status = String::new();
    while Instant::now() < deadline {
        status = tls_status(admin_port);
        if !status.contains("\"last_error\":null") && status.contains("\"last_error\":") {
            break;
        }
        thread::sleep(Duration::from_millis(200));
    }
    assert!(
        !status.contains("\"last_error\":null"),
        "a failing ACME attempt should be reported on /web/tls/status: {status}"
    );
}
