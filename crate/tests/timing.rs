// Ground-truth timing probe (reproducible, but `#[ignore]`d so it never slows a
// normal `cargo test`). Run it explicitly in release with output shown:
//
//   cargo test --release --test timing -- --ignored --nocapture
//
// Prints median ms and placed count per (mode, n). This is the source of the
// host numbers in ../../BENCHMARKS.md; `cargo bench` gives the same picture with
// statistical rigor but takes much longer.

use std::time::Instant;
use wordcloud_layout::datasets::{config_for, generate, WeightDist};
use wordcloud_layout::{layout_balanced, layout_fast, layout_pretty};

fn median(mut v: Vec<f64>) -> f64 {
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let m = v.len() / 2;
    if v.len() % 2 == 1 { v[m] } else { (v[m - 1] + v[m]) / 2.0 }
}

fn probe(mode: &str, n: usize, reps: usize) {
    let items = generate(n, WeightDist::Zipf, 1);
    let cfg = config_for(n);
    let f = match mode {
        "pretty" => layout_pretty,
        "balanced" => layout_balanced,
        "fast" => layout_fast,
        other => panic!("unknown mode {other}"),
    };
    let mut placed = 0;
    let mut times = Vec::new();
    for _ in 0..reps {
        let t0 = Instant::now();
        let out = f(&items, &cfg);
        times.push(t0.elapsed().as_secs_f64() * 1000.0);
        placed = out.len();
    }
    println!(
        "TIMING {:<9} n={:<6} placed={:<6} median={:>10.3} ms  (canvas {}x{})",
        mode, n, placed, median(times), cfg.width as u32, cfg.height as u32
    );
}

#[test]
#[ignore = "run explicitly with --release --ignored --nocapture"]
fn timing_probe() {
    println!();
    // 50k+ is Fast mode / browser territory (Task 13); the spiral modes are
    // exercised up to 5000 here.
    for &n in &[50usize, 100, 200, 500, 1000, 5000] {
        probe("pretty", n, 3);
    }
    for &n in &[50usize, 100, 200, 500, 1000, 5000] {
        probe("balanced", n, 3);
    }
    // Fast mode is the massive-cloud path; push it to 50000.
    for &n in &[500usize, 1000, 5000, 50000] {
        probe("fast", n, 3);
    }
}
