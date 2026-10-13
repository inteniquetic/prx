//! T204: applying a config while traffic is flowing must not cost a request.
//!
//! The swap is atomic by construction; this proves it through the real binary:
//! clients hammer the proxy while the admin API replaces the config over and
//! over, flipping the one upstream between two servers. Every request has to
//! be answered 200 by one of them.

mod common;

use std::{
    io::{Read, Write},
    net::TcpStream,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    thread,
    time::Duration,
};

use common::{PrxProcess, UpstreamServer, reserve_port, write_config};
use tempfile::tempdir;

fn config(proxy_port: u16, upstream_port: u16, tag: usize) -> String {
    format!(
        r#"[server]
listen = ["127.0.0.1:{proxy_port}"]
[observability]
log_level = "error"
[[service]]
name = "s"
max_retries = 0
[[service.upstream]]
addr = "127.0.0.1:{upstream_port}"
[[route]]
name = "r{tag}"
service = "s"
is_default = true
"#
    )
}

fn get(port: u16) -> Result<String, std::io::Error> {
    let mut stream = TcpStream::connect(("127.0.0.1", port))?;
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    stream.write_all(b"GET /x HTTP/1.1\r\nHost: t\r\nConnection: close\r\n\r\n")?;
    let mut out = String::new();
    stream.read_to_string(&mut out)?;
    Ok(out)
}

fn put_config(admin_port: u16, body: &str) -> String {
    let mut stream = TcpStream::connect(("127.0.0.1", admin_port)).expect("admin connect");
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    let request = format!(
        "PUT /web/config HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(request.as_bytes()).unwrap();
    let mut out = String::new();
    let _ = stream.read_to_string(&mut out);
    out
}

#[test]
fn applying_configs_under_load_loses_no_request() {
    let (proxy, admin) = (reserve_port(), reserve_port());
    let (up_a, up_b) = (reserve_port(), reserve_port());
    let _a = UpstreamServer::spawn(up_a, "A");
    let _b = UpstreamServer::spawn(up_b, "B");
    let dir = tempdir().unwrap();
    let path = write_config(&dir, &config(proxy, up_a, 0));
    let prx = PrxProcess::spawn(&path, admin);
    prx.wait_until_listening(proxy);
    prx.wait_until_listening(admin);

    let stop = Arc::new(AtomicBool::new(false));
    let ok = Arc::new(AtomicUsize::new(0));
    let bad = Arc::new(AtomicUsize::new(0));
    let clients: Vec<_> = (0..4)
        .map(|_| {
            let (stop, ok, bad) = (stop.clone(), ok.clone(), bad.clone());
            thread::spawn(move || {
                while !stop.load(Ordering::Relaxed) {
                    match get(proxy) {
                        Ok(r) if r.starts_with("HTTP/1.1 200") => {
                            ok.fetch_add(1, Ordering::Relaxed)
                        }
                        _ => bad.fetch_add(1, Ordering::Relaxed),
                    };
                }
            })
        })
        .collect();

    thread::sleep(Duration::from_millis(300));
    for round in 1..=12 {
        let upstream = if round % 2 == 0 { up_a } else { up_b };
        let reply = put_config(admin, &config(proxy, upstream, round));
        assert!(reply.starts_with("HTTP/1.1 200"), "round {round}: {reply}");
        thread::sleep(Duration::from_millis(60));
    }
    thread::sleep(Duration::from_millis(300));
    stop.store(true, Ordering::Relaxed);
    for client in clients {
        client.join().unwrap();
    }

    let (ok, bad) = (ok.load(Ordering::Relaxed), bad.load(Ordering::Relaxed));
    assert!(ok > 200, "the load was too light to mean anything: {ok}");
    assert_eq!(
        bad,
        0,
        "{bad} of {} requests failed during applies",
        ok + bad
    );
}
