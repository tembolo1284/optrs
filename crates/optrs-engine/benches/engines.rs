// benches/engines.rs
//! criterion benchmarks. Run with `cargo bench`. The interesting comparison is
//! not raw speed but time-to-accuracy: configure each engine to hit 1e-6 and
//! then compare. `european_at_tolerance` does exactly that.

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use optrs_core::analytic::{BsmInputs, OptionType};
use optrs_core::instrument::PriceRequest;
use optrs_engine::{Config, Engine};
use std::hint::black_box;

fn inputs() -> BsmInputs {
    BsmInputs { spot: 100.0, strike: 100.0, rate: 0.05, div_yield: 0.02, vol: 0.25, time: 1.0 }
}

fn european(c: &mut Criterion) {
    let req = PriceRequest::european(inputs(), OptionType::Call);
    let cfg = Config::default();
    let mut group = c.benchmark_group("european");
    for engine in Engine::ALL.iter().filter(|e| e.supports(&req)) {
        group.bench_with_input(BenchmarkId::from_parameter(engine.name()), engine, |b, &e| {
            b.iter(|| e.price_raw(black_box(&req), black_box(&cfg)).unwrap());
        });
    }
    group.finish();
}

fn american(c: &mut Criterion) {
    let req = PriceRequest::american(inputs(), OptionType::Put);
    let mut cfg = Config::default();
    cfg.mc.paths = 20_000; // MC is the slow one; keep the bench tolerable
    let mut group = c.benchmark_group("american");
    for engine in Engine::ALL.iter().filter(|e| e.supports(&req)) {
        group.bench_with_input(BenchmarkId::from_parameter(engine.name()), engine, |b, &e| {
            b.iter(|| e.price_raw(black_box(&req), black_box(&cfg)).unwrap());
        });
    }
    group.finish();
}

fn european_at_tolerance(c: &mut Criterion) {
    let req = PriceRequest::european(inputs(), OptionType::Call);
    let mut cfg = Config::default();
    cfg.convergence.tolerance = 1e-6;
    let mut group = c.benchmark_group("european_1e-6");
    for engine in [Engine::Cos, Engine::TreeLr, Engine::FiniteDifference] {
        group.bench_with_input(BenchmarkId::from_parameter(engine.name()), &engine, |b, &e| {
            b.iter(|| optrs_engine::price_converged(e, black_box(&req), black_box(&cfg)).unwrap());
        });
    }
    group.finish();
}

criterion_group!(benches, european, american, european_at_tolerance);
criterion_main!(benches);
