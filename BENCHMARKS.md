# Benchmarks

Performance measurements for the layout engine. All numbers are reproducible
from source — see *How to reproduce* below.

All datasets are generated deterministically by
[`crate/src/datasets.rs`](./crate/src/datasets.rs) (and its byte-compatible JS
port [`js/datasets.js`](./js/datasets.js)), so every harness below measures the
**same inputs**. Unless noted, the weight distribution is **Zipf** (a few
dominant words, long tail — the realistic, collision-heavy case) and the canvas
is sized by the shared `config_for(n)` (≈28% fill, 4:3).

## Spec targets

| Bucket | Words        | Target                              |
| ------ | ------------ | ----------------------------------- |
| Small  | 50 – 500     | < 10 ms                             |
| Medium | 500 – 5000   | < 50 ms                             |
| Large  | 5000 – 50000 | Interactive, no main-thread freeze  |

## How to reproduce

```sh
# Native, statistically rigorous (criterion). A few minutes.
cd crate && cargo bench                    # all, or: cargo bench -- balanced/100

# Native, quick one-shot medians (the source of the tables below):
cd crate && cargo test --release --test timing -- --ignored --nocapture

# In-browser end-to-end (WASM boundary), + optional d3-cloud comparison:
cd crate && wasm-pack build --target web --out-dir ../pkg
python -m http.server                      # open /examples/benchmark.html

# SIMD on/off, headless via Node (see "SIMD" below).
```

## Results (host CPU, release, scalar kernels)

Median wall-clock for one full `layout()` call; all words placed (`placed = n`).
This is the *algorithmic* cost — no WASM boundary, no SIMD (the `simd128` kernels
exist only on `wasm32`; see below). The "before" column is the engine as it stood
after Task 11; "after" includes the two optimizations described in *Findings*.

| n    | Pretty before | **Pretty after** | Balanced before | **Balanced after** |
| ---- | ------------- | ---------------- | --------------- | ------------------ |
| 50   | 112 ms        | **4.3 ms** ✅    | 517 ms          | 6.0 ms             |
| 100  | 336 ms        | **11.4 ms**      | 1337 ms         | 18.5 ms            |
| 200  | 984 ms        | **36.9 ms** ✅†  | 4350 ms         | 62.1 ms            |
| 500  | 4814 ms       | **175 ms**       | 22920 ms        | 356 ms             |
| 1000 | —             | 437 ms           | —               | 925 ms             |
| 5000 | —             | 10448 ms         | —               | 24477 ms           |

✅ meets the small-cloud target (< 10 ms). † `pretty/200` (37 ms) meets the
*medium* target (< 50 ms) even though it's in the small bucket.

> Measured with `cargo test --release --test timing -- --ignored --nocapture`.
> Absolute ms are hardware-dependent (±20% run-to-run); the **ratios** are the
> point. The two changes together sped Balanced up ~60–70× at n = 200–500.

### Fast mode (massive clouds)

Row/shelf packing — a single linear pass, no per-word search. Same datasets and
canvases as above; Fast packs them far more densely, so it places **every** word.

| n     | Fast median | placed       | vs Pretty (same n) |
| ----- | ----------- | ------------ | ------------------ |
| 500   | 1.6 ms      | 500 / 500    | ~90×               |
| 1000  | 3.1 ms      | 1000 / 1000  | ~120×              |
| 5000  | 11.8 ms     | 5000 / 5000  | ~720×              |
| 50000 | 88 ms       | 50000 / 50000| —                  |

At 50000 words Fast finishes in ~88 ms — comfortably interactive (and off the
main thread via the worker). This is the spec's *Large Cloud* path; the trade is
appearance (a justified block, not an organic cloud). The 50000-word
no-overlap + linearity stress test (`fast_stress_50000…`) passes in ~3 s
including its brute-force re-verification.

## Findings

### 1. The spiral candidate count was the bottleneck — now fixed (~25× both modes)

The original spiral stepped ~2 px with ~1 px ring spacing, evaluating on the
order of `(π/2)·R²` candidate positions for a cloud of radius `R`. The fix
([`Spiral` in `layout.rs`](./crate/src/layout.rs)) scales the step to the word's
smaller dimension (probing finer than a word can't help — a sub-word gap can't
hold it) and matches the ring spacing to it, so the candidate count drops to
~`π·R²/step²`. Both modes share the spiral, so both get ~25× faster and their
placements stay identical to each other. **Pretty now meets the small-cloud
target up to ~70 words and the medium target up to ~220 words**, with every word
still placed.

### 2. Balanced's spatial grid is redundant with the occupancy bitmap

This is the deeper result, and it reframes the spec's premise. Balanced still
runs ~2× slower than Pretty even after the fixes — and it structurally **cannot**
beat it with this collision model:

- Pretty's collision test, `OccupancyBitmap::collides`, is a bitmap AND-scan over
  the candidate's footprint — **O(mask area), independent of how many words are
  already placed**. The bitmap collapses all placed words into one fixed-size
  bitfield.
- The spatial grid exists to replace an **O(placed)** "compare against every
  word" scan with an **O(nearby)** one. But Pretty never does an O(placed) scan —
  the bitmap already gave it word-count-independent collision. So the grid's
  broad phase solves a problem this engine doesn't have.
- Making the grid query allocation-free (this task: `neighbors_into` with a
  generation-stamped dedup, reusing one buffer) cut Balanced's per-candidate cost
  ~2.5× (it was 4–5× slower; now ~2×). But in the dense central region, where
  most spiral candidates land and fail, Balanced pays the grid query **and** the
  bitmap test, so it stays slower than Pretty's bitmap test alone.

Balanced is kept: it's correct, produces output identical to Pretty, and is now
~60× faster than before. But **Pretty is the better choice today** at every size
the spiral modes target. (Notably, Task 13's Fast mode did *not* end up using the
spatial grid either — see Finding 4 — so the `SpatialGrid` broad phase is, in this
engine, infrastructure without a winning caller. It's kept for spec completeness
and its thorough tests.)

### 3. SIMD gives no end-to-end speedup on this workload

Scalar vs `+simd128` WASM builds, timed headless in Node, are within noise (see
table below). The Task-11 kernels are correct (`SIMD == scalar` is proven), but
the vectorized AND-scan / OR-write isn't the bottleneck: per-word masks are only a
few `u64` wide, so `v128` load/store overhead cancels the two-lane gain, and the
spiral search — now the dominant cost — is scalar. SIMD would pay off in a
kernel-bound loop (wide masks, long contiguous scans); here, Finding 1 is the
lever that moved the needle.

### 4. Fast mode meets the large-cloud target with room to spare

Row (shelf) packing places **100% of words** with a bounded, single linear pass:
50000 words in ~88 ms, and ~720× faster than Pretty at 5000. There's no per-word
search to blow up and no overlap is possible by construction (disjoint rows and
disjoint x-spans within a row), so it scales predictably where the O(n²)-ish
spiral cannot. The trade is appearance — a justified block, not an organic cloud.
Worth noting: the natural data structure for packing variable-width *text* turned
out to be rows, not a 2-D cell grid (uniform cells waste too much on wide, short
rectangles), which is why Fast mode does not use `SpatialGrid`.

## SIMD (WASM `v128` vs scalar)

```sh
cd crate
wasm-pack build --target web --out-dir ../pkg                                              # scalar
RUSTFLAGS="-C target-feature=+simd128" wasm-pack build --target web --out-dir ../pkg-simd  # SIMD
cd ..
node scripts/_simd_bench.mjs pkg       pretty 200
node scripts/_simd_bench.mjs pkg-simd  pretty 200
```

Measured (Node, n = 200, Pretty, Zipf, median of 9, current engine):

| Build              | median   |
| ------------------ | -------- |
| scalar (`pkg`)     | 37.5 ms  |
| SIMD (`pkg-simd`)  | 37.4 ms  |
| **speedup**        | ≈ 1.00×  |

(The wasm number matches the native 37 ms, so the boundary adds negligible
overhead.) Before the Finding-1 spiral fix these were ~1050 ms each — same ≈1×
ratio, just slower; the fix makes the bitmap kernel an even smaller share of
total time, so SIMD matters even less now.

## Threads (parallel mask generation)

Mask rasterization is independent per word and is fanned out across cores with
[Rayon](https://github.com/rayon-rs/rayon) — `--features parallel` natively (OS
threads), or `wasm-threads` + a SharedArrayBuffer Web Worker pool in the browser.
Placement stays sequential. Measured natively on a multi-core host, **Fast** mode
(where mask generation dominates):

| n (Fast) | single-threaded | `--features parallel` | speedup |
| -------- | --------------- | --------------------- | ------- |
| 5000     | 11.8 ms         | 5.8 ms                | ~2.0×   |
| 50000    | 88 ms           | 46 ms                 | ~1.9×   |

**Finding 5 — parallelism helps mask generation, not placement.** The spiral
modes are placement-bound (a sequential spiral walk), so parallel mask gen barely
moves them — e.g. `pretty/5000` is unchanged within noise. The win is at scale in
Fast mode, exactly where rasterizing tens of thousands of words is the bottleneck.
In the browser the same Rayon code runs on a Web Worker pool; that needs a
cross-origin-isolated page (COOP/COEP) and a nightly threaded build (see README),
and falls back to single-threaded automatically when unavailable.

> Reproduce: `cd crate && cargo test --release --features parallel --test timing -- --ignored --nocapture`
> and compare the `fast` rows against a plain (no `--features`) run.

## Comparison vs. a JS library (d3-cloud)

[`d3-cloud`](https://github.com/jasondavies/d3-cloud) is the canonical JS
word-cloud library: a single-threaded spiral + sprite/bitmap collision mask
running on the main thread, so it's the natural baseline.

### Measured (host CPU, same inputs)

Median of 3 runs, **identical inputs for both engines**: the `datasets.rs`
generator (LCG, Zipf weights, seed 1), the shared `config_for(n)` canvas size,
`padding = 2`, `rotate(0)`, `sans-serif`. d3-cloud was driven headless under
Node v22 with `node-canvas` for glyph measurement; `cloud.start()` is
synchronous there, so wall-clock is timed directly around it. Engine rows are
the host scalar numbers from the tables above. Lower is better.

| Words | Fast        | Pretty   | Balanced  | **d3-cloud** | Fast vs d3-cloud |
| ----- | ----------- | -------- | --------- | ------------ | ---------------- |
| 100   | **0.97 ms** | 11.3 ms  | 18.4 ms   | 112 ms       | ~120× faster     |
| 500   | **2.6 ms**  | 173 ms   | 335 ms    | 384 ms       | ~150× faster     |
| 5000  | **14.3 ms** | 9858 ms  | 22493 ms  | 2551 ms      | ~180× faster     |

**Reading it honestly:**

- **Fast mode beats d3-cloud at every scale** — ~120× at 100, ~150× at 500,
  ~180× at 5000 (14 ms vs. 2.55 s). This is the mode built for big clouds.
- **Pretty is faster than d3-cloud at small N** (11 ms vs. 112 ms at 100) and
  comparable at 500, but its O(n²) spiral makes it *slower* than d3-cloud at
  5000 (9.9 s vs. 2.6 s). Pretty targets 50–500 words; past a few hundred,
  reach for Fast. Balanced tracks ~2× Pretty (Finding 2).
- **Fairness caveat:** d3-cloud measures *real glyph outlines*; this engine
  uses approximate advance-width metric boxes (`MetricsRasterizer`), so part of
  d3-cloud's cost is genuine text measurement this engine currently skips. Both
  run single-threaded here. A glyph-accurate rasterizer is on the roadmap.

> Reproduce: install `d3-cloud` + `canvas` in a scratch project, reimplement the
> `datasets.rs` generator and `weight → font_size` mapping in JS (faithful port),
> feed the words to `d3.layout.cloud().canvas(makeCanvas)...`, and time
> `.start()`. [`examples/benchmark.html`](./examples/benchmark.html) also times
> d3-cloud in the browser on the same datasets (checkbox "also time d3-cloud";
> loaded from a CDN, so it needs network) — handy for end-to-end WASM-boundary
> numbers rather than the native algorithm cost shown above.

## Files

- [`crate/src/datasets.rs`](./crate/src/datasets.rs) — dataset + `config_for`
  generators (shared source of truth).
- [`js/datasets.js`](./js/datasets.js) — byte-compatible JS port for the harness.
- [`crate/benches/layout.rs`](./crate/benches/layout.rs) — criterion benchmarks.
- [`crate/tests/timing.rs`](./crate/tests/timing.rs) — quick one-shot timing probe.
- [`examples/benchmark.html`](./examples/benchmark.html) — in-browser end-to-end
  + d3-cloud harness.
- [`scripts/_simd_bench.mjs`](./scripts/_simd_bench.mjs) — Node SIMD-vs-scalar timer.
- [`js/threads.js`](./js/threads.js) — thread-pool init + capability detection
  (threaded build).
