//! Criterion benchmarks (master prompt §12). CI compares a pull request against its base on the
//! same runner and fails on a > 15 % slowdown (scripts/check_bench_regression.py).
//! Run: cargo bench -p sim-scenario --bench engine

#![allow(clippy::unwrap_used)] // benchmark harness

use criterion::{criterion_group, criterion_main, Criterion};
use sim_geo::{direct, inverse, GeoPoint};
use std::hint::black_box;

fn geodesy(c: &mut Criterion) {
    let a = GeoPoint::new(-37.950_1, 144.424_9);
    let b = GeoPoint::new(-37.652_8, 143.926_5);
    c.bench_function("geodesy/vincenty_inverse", |bench| {
        bench.iter(|| inverse(black_box(a), black_box(b)));
    });
    c.bench_function("geodesy/vincenty_direct", |bench| {
        bench.iter(|| direct(black_box(a), black_box(306.868), black_box(54_972.271)));
    });
}

/// One full sensor cycle (5 ticks: 5 × movement + 1 × radar detection) for 1,000 units carrying
/// five radars each: the 1,000-entity budget workload.
fn engine(c: &mut Criterion) {
    let db = sim_db::embedded::open_platform_db().unwrap();
    let scenario = sim_scenario::synthetic::synthetic_scenario(1000, "GEN-FFG", 1);
    let mut sim = scenario.build(None, &db).unwrap();
    sim.step();
    let mut group = c.benchmark_group("engine");
    group.sample_size(20);
    group.bench_function("1000_units_one_sensor_cycle", |bench| {
        bench.iter(|| sim.run_ticks(5));
    });
    group.finish();
}

criterion_group!(benches, geodesy, engine);
criterion_main!(benches);
