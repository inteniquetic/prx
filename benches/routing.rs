//! Micro-benchmarks for the request-path hot spots (T003).
//!
//! These numbers are the regression signal for T101 (allocation removal) and
//! T102 (route index): both tasks must show a large win on `select_route/*`
//! without regressing the small-config cases.

use std::hint::black_box;

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use prx::{
    config::PrxConfig,
    runtime::{RuntimeConfig, hash_key, normalize_host},
};

include!("support/common.rs");

fn runtime_with(n: usize) -> RuntimeConfig {
    RuntimeConfig::from_config(PrxConfig::from_toml_str(&config_toml(n)).expect("valid config"))
}

fn wildcard_runtime_with(n: usize) -> RuntimeConfig {
    RuntimeConfig::from_config(
        PrxConfig::from_toml_str(&wildcard_config_toml(n)).expect("valid config"),
    )
}

fn bench_select_route(c: &mut Criterion) {
    let mut group = c.benchmark_group("select_route");

    for n in [10usize, 100, 1000] {
        let runtime = runtime_with(n);
        let last = n - 1;

        // Best case for a linear scan: the route sits at the front.
        group.bench_with_input(BenchmarkId::new("first_route", n), &n, |b, _| {
            b.iter(|| {
                black_box(
                    runtime.select_route(black_box("svc0.bench.local"), black_box("/api/v0/users")),
                )
            })
        });

        // Worst case: the route sits at the very end of the list.
        group.bench_with_input(BenchmarkId::new("last_route", n), &n, |b, _| {
            let host = format!("svc{last}.bench.local");
            let path = format!("/api/v{last}/users");
            b.iter(|| black_box(runtime.select_route(black_box(&host), black_box(&path))))
        });

        // No host matches: the whole table is scanned before falling back.
        group.bench_with_input(BenchmarkId::new("default_fallback", n), &n, |b, _| {
            b.iter(|| {
                black_box(
                    runtime.select_route(black_box("unknown.bench.local"), black_box("/nothing")),
                )
            })
        });

        // Host header arrives with a port and mixed case, i.e. the normalize path.
        group.bench_with_input(BenchmarkId::new("host_with_port", n), &n, |b, _| {
            b.iter(|| {
                black_box(runtime.select_route(
                    black_box("SVC0.Bench.Local:8080"),
                    black_box("/api/v0/users"),
                ))
            })
        });
    }

    for n in [10usize, 100, 1000] {
        let runtime = wildcard_runtime_with(n);
        let last = n - 1;
        group.bench_with_input(BenchmarkId::new("wildcard_last", n), &n, |b, _| {
            let host = format!("a.tenant{last}.bench.local");
            b.iter(|| black_box(runtime.select_route(black_box(&host), black_box("/api/x"))))
        });
    }

    group.finish();
}

fn bench_host_and_hash(c: &mut Criterion) {
    let mut group = c.benchmark_group("request_path_helpers");

    group.bench_function("normalize_host/plain", |b| {
        b.iter(|| black_box(normalize_host(black_box("svc0.bench.local"))))
    });
    group.bench_function("normalize_host/with_port", |b| {
        b.iter(|| black_box(normalize_host(black_box("SVC0.Bench.Local:8080"))))
    });
    group.bench_function("normalize_host/ipv6", |b| {
        b.iter(|| black_box(normalize_host(black_box("[2001:db8::1]:8443"))))
    });
    group.bench_function("hash_key/host_path", |b| {
        b.iter(|| black_box(hash_key(black_box(&["svc0.bench.local", "/api/v0/users"]))))
    });

    group.finish();
}

criterion_group!(benches, bench_select_route, bench_host_and_hash);
criterion_main!(benches);
