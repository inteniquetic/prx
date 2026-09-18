//! Benchmarks for config parsing, validation and runtime build (T003).
//!
//! `RuntimeConfig::from_config` runs on every hot reload, i.e. every time
//! someone presses Save in the WebUI, so its cost is a user-visible latency
//! and the budget stated in T102 (< 5ms for 1000 routes).

use std::hint::black_box;

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use prx::{config::PrxConfig, runtime::RuntimeConfig};

include!("support/common.rs");

fn bench_parse(c: &mut Criterion) {
    let mut group = c.benchmark_group("config_parse_validate");
    for n in [10usize, 100, 1000] {
        let toml = config_toml(n);
        group.bench_with_input(BenchmarkId::from_parameter(n), &toml, |b, toml| {
            b.iter(|| black_box(PrxConfig::from_toml_str(black_box(toml)).expect("valid config")))
        });
    }
    group.finish();
}

fn bench_runtime_build(c: &mut Criterion) {
    let mut group = c.benchmark_group("runtime_build");
    for n in [10usize, 100, 1000] {
        let config = PrxConfig::from_toml_str(&config_toml(n)).expect("valid config");
        group.bench_with_input(BenchmarkId::from_parameter(n), &config, |b, config| {
            b.iter(|| black_box(RuntimeConfig::from_config(black_box(config.clone()))))
        });
    }
    group.finish();
}

criterion_group!(benches, bench_parse, bench_runtime_build);
criterion_main!(benches);
