//! Native criterion benchmarks for the layout core (spec: *Performance
//! Targets*, Task 12).
//!
//! These measure the placement algorithms on the host CPU — pure algorithmic
//! cost, independent of the WASM boundary. The native build uses the *scalar*
//! collision kernels (the `simd128` path only exists on `wasm32`), so the SIMD
//! on/off speedup is measured separately by the in-browser harness
//! (`examples/benchmark.html`); see `BENCHMARKS.md`.
//!
//! Datasets and canvas sizing come from [`wordcloud_layout::datasets`], the same
//! source the browser harness uses, so the two harnesses exercise identical
//! inputs.
//!
//! Pretty mode is O(n²) in spiral candidates and gets expensive past a few
//! hundred words, and Balanced currently pays a per-candidate allocation in its
//! grid query (see `BENCHMARKS.md` → Findings), so the benchmarked sizes are
//! deliberately modest to keep `cargo bench` to a few minutes. For one-shot
//! timing at larger sizes use the probe in `tests/timing.rs`:
//!
//!   cargo test --release --test timing -- --ignored --nocapture
//!
//! Run the benchmarks:  `cargo bench`            (from `crate/`)
//! A subset:            `cargo bench -- pretty/100`

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use std::hint::black_box;
use std::time::Duration;
use wordcloud_layout::datasets::{config_for, generate, WeightDist};
use wordcloud_layout::{layout_balanced, layout_pretty};

/// Pretty mode across small sizes (its design sweet spot; O(n²) beyond).
fn bench_pretty(c: &mut Criterion) {
    let mut group = c.benchmark_group("pretty");
    group.sample_size(10).warm_up_time(Duration::from_millis(500));
    for &n in &[50usize, 100, 200] {
        let items = generate(n, WeightDist::Zipf, 1);
        let cfg = config_for(n);
        group.bench_with_input(BenchmarkId::from_parameter(n), &n, |b, _| {
            b.iter(|| layout_pretty(black_box(&items), black_box(&cfg)));
        });
    }
    group.finish();
}

/// Balanced mode across small/medium sizes.
fn bench_balanced(c: &mut Criterion) {
    let mut group = c.benchmark_group("balanced");
    group.sample_size(10).warm_up_time(Duration::from_millis(500));
    for &n in &[50usize, 100, 200, 500] {
        let items = generate(n, WeightDist::Zipf, 1);
        let cfg = config_for(n);
        group.bench_with_input(BenchmarkId::from_parameter(n), &n, |b, _| {
            b.iter(|| layout_balanced(black_box(&items), black_box(&cfg)));
        });
    }
    group.finish();
}

/// How the weight distribution affects cost at a fixed size (skewed Zipf clouds
/// cluster large words centrally and stress collisions differently from an even
/// spread).
fn bench_distributions(c: &mut Criterion) {
    let n = 200;
    let cfg = config_for(n);
    let mut group = c.benchmark_group("pretty-dist-200");
    group.sample_size(10).warm_up_time(Duration::from_millis(500));
    for dist in [WeightDist::Uniform, WeightDist::Linear, WeightDist::Zipf] {
        let items = generate(n, dist, 1);
        group.bench_with_input(BenchmarkId::from_parameter(dist.label()), &dist, |b, _| {
            b.iter(|| layout_pretty(black_box(&items), black_box(&cfg)));
        });
    }
    group.finish();
}

criterion_group!(benches, bench_pretty, bench_balanced, bench_distributions);
criterion_main!(benches);
