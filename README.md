# wordcloud-wasm

> A Rust + WebAssembly layout engine that places word clouds and other
> visualizations **off the main thread**, scaling from 50 to 50 000+ words.
> Designed as a generic geometry engine, not a single-purpose tool.

The engine **only computes geometry** — where each word goes. Your code does
the drawing, so it works with any renderer: Canvas, SVG, D3, React, dashboards.

---

## Why it exists

The canonical JS word-cloud library (`d3-cloud`) is single-threaded, struggles
past ~1000 words, and hasn't had a major release in years. This project takes a
different approach:

- **Off the main thread** — layout runs in a Web Worker; your UI never freezes.
- **Bitmap collision** — all placed words collapse into one `Vec<u64>` occupancy
  bitmap, so collision cost is O(mask area), independent of how many words are
  already placed.
- **Three placement strategies** — Archimedean spiral for aesthetics (50–500
  words), shelf/row packing for scale (5 000–50 000+ words), and an optional
  spatial-grid mode in between.
- **Generic layout framework** — a `LayoutEngine` trait means treemap, circle
  packing, label placement, and bubble charts can all share the same collision
  and rasterization infrastructure. Treemap is already implemented.

## Benchmarks

Measured on the host CPU (scalar kernels, release build), same deterministic
datasets for both engines. Full methodology in [`BENCHMARKS.md`](./BENCHMARKS.md).

### vs. d3-cloud (single-threaded, same inputs)

| Words | Fast mode   | Pretty mode | d3-cloud | Fast vs d3-cloud |
|-------|-------------|-------------|----------|------------------|
| 100   | **0.97 ms** | 11 ms       | 112 ms   | ~120×            |
| 500   | **2.6 ms**  | 173 ms      | 384 ms   | ~150×            |
| 5 000 | **14 ms**   | 9 858 ms    | 2 551 ms | ~180×            |

**Reading it honestly:**
- Fast mode beats d3-cloud at every scale — including at 5 000 words where
  Pretty's O(n²) spiral falls behind.
- Pretty is faster than d3-cloud at small N (11 ms vs 112 ms at 100 words) and
  is the better-looking mode for small clouds.
- d3-cloud measures real glyph outlines; this engine currently uses approximate
  advance-width boxes. Part of d3-cloud's cost is work this engine skips.

### Fast mode scaling

| Words  | Time   | Placed        |
|--------|--------|---------------|
| 500    | 1.6 ms | 500 / 500     |
| 1 000  | 3.1 ms | 1 000 / 1 000 |
| 5 000  | 12 ms  | 5 000 / 5 000 |
| 50 000 | 88 ms  | 50 000 / 50 000 |

100% placement at every scale. The trade is appearance — a justified grid, not
an organic spiral cloud.

---

## Project layout

```
crate/    Rust source (wordcloud_layout cdylib + rlib)
pkg/      Generated wasm-pack output (committed for GitHub Pages; rebuild with wasm-pack)
js/       JS/TS wrapper around pkg/
examples/ Browser usage examples
scripts/  Dev tooling (SIMD/scalar timing script)
index.html  GitHub Pages showcase
```

## GitHub Pages

A showcase page (`index.html`) with live demos and code samples is served from the
repo root. To host it yourself:

1. Build the WASM package into `pkg/`:
   ```sh
   cd crate
   wasm-pack build --target web --out-dir ../pkg
   ```
2. Commit `pkg/` (the four generated files are not git-ignored).
3. Push to GitHub, then in **Settings → Pages** set source to **"Deploy from branch"**,
   branch `main`, folder `/ (root)`.

The live site is at: `https://rugsaw.github.io/wordcloud-wasm/`

## Prerequisites

- Rust toolchain (`rustc` / `cargo`)
- `wasm32-unknown-unknown` target: `rustup target add wasm32-unknown-unknown`
- [`wasm-pack`](https://rustwasm.github.io/wasm-pack/)

## Building

```sh
cd crate
wasm-pack build --target web --out-dir ../pkg
```

This emits `pkg/wordcloud_layout.js`, `pkg/wordcloud_layout_bg.wasm`, and
TypeScript definitions. Then serve the project root over HTTP — browsers block
WASM/ES modules over `file://`:

```sh
python -m http.server     # open http://localhost:8000/examples/canvas.html
```

### SIMD build (optional)

A `v128` kernel exists for the bitmap AND-scan / OR-write hot path:

```sh
cd crate
RUSTFLAGS="-C target-feature=+simd128" wasm-pack build --target web --out-dir ../pkg
```

**Honest note:** SIMD currently gives no measurable end-to-end speedup (≈1×,
within noise). The spiral search is the bottleneck for Pretty/Balanced, not the
bitmap kernel. The `v128` code is correct and tested, but the leverage point
is the spiral candidate count, not vectorized memory scans. SIMD would help in a
kernel-bound loop with wide masks; for now, spiral step-scaling (~25× win) moved
the needle.

### Multi-threaded build (optional, experimental)

Mask rasterization is independent per word and fans out across threads with
[Rayon](https://github.com/rayon-rs/rayon). Measured speedup:

| Words (Fast) | Single-threaded | Parallel | Speedup |
|--------------|-----------------|----------|---------|
| 5 000        | 12 ms           | 5.8 ms   | ~2×     |
| 50 000       | 88 ms           | 46 ms    | ~1.9×   |

The threaded build requires a nightly toolchain and a **cross-origin-isolated**
page (`Cross-Origin-Opener-Policy: same-origin` + `Cross-Origin-Embedder-Policy:
require-corp`):

```sh
cd crate
rustup toolchain install nightly
rustup component add rust-src --toolchain nightly
RUSTFLAGS="-C target-feature=+atomics,+bulk-memory,+mutable-globals" \
  CARGO_UNSTABLE_BUILD_STD="panic_abort,std" \
  rustup run nightly \
  wasm-pack build --target web --out-dir ../pkg-threads --features wasm-threads
```

Then initialize the thread pool before the first layout call:

```js
import { initThreads } from "./js/threads.js";
import { layout } from "./js/index.js";

const n = await initThreads();   // 0 → single-threaded fallback
const placements = await layout(items, { mode: "fast", width, height });
```

Falls back cleanly to single-threaded if the page isn't isolated or
`SharedArrayBuffer` is unavailable.

---

## Usage

### Word cloud layout

```js
import { layout } from "./js/index.js";

const items = [
  { text: "WebAssembly", weight: 100 },
  { text: "Rust",        weight: 80  },
  { text: "layout",      weight: 40  },
];

const placements = await layout(items, {
  mode:        "pretty",   // "pretty" | "balanced" | "fast"
  width:       960,
  height:      600,
  minFontSize: 14,
  maxFontSize: 88,
  padding:     2,
});
// → [{ text, x, y, rotation, fontSize }, ...]
// x/y are the bounding-box center — render with center anchoring.
```

Render to a `<canvas>`:

```js
const ctx = canvas.getContext("2d");
ctx.textAlign    = "center";
ctx.textBaseline = "middle";
for (const p of placements) {
  ctx.save();
  ctx.translate(p.x, p.y);
  if (p.rotation) ctx.rotate((p.rotation * Math.PI) / 180);
  ctx.font = `${p.fontSize}px sans-serif`;
  ctx.fillText(p.text, 0, 0);
  ctx.restore();
}
```

A full working page is in [`examples/canvas.html`](./examples/canvas.html).

### Choosing a mode

| Mode | Words | Look | Notes |
|------|-------|------|-------|
| **`pretty`** *(default)* | 50–500 | Organic, spiral | Recommended default. Faster than d3-cloud at small N. |
| **`balanced`** | 500–5 000 | Same as pretty | Adds a spatial-grid broad phase that doesn't beat the bitmap — ~2× *slower* than `pretty`. Kept for completeness. |
| **`fast`** | 5 000–50 000+ | Grid-packed | Bounded linear time, 100% placement. Trade: less organic appearance. |

Prefer `pretty` for ≤ 500 words and `fast` for everything larger. Skip
`balanced` unless you have a specific reason.

### Running off the main thread (Web Worker)

```js
import { createLayoutClient } from "./js/client.js";

const client     = createLayoutClient();   // spawns js/worker.js as a module worker
const placements = await client.layout(items, { mode: "pretty", width, height });
// render exactly as above
client.terminate();
```

The worker loads WASM once and reuses it across calls. Concurrent calls are
safe — each gets its own request ID. A working demo (with a spinner that keeps
animating to prove the main thread stays responsive) is in
[`examples/worker.html`](./examples/worker.html).

### Treemap

The engine isn't word-cloud-only. `treemap()` tiles the canvas into rectangles
whose areas are proportional to weight (squarified algorithm):

```js
import { treemap } from "./js/index.js";

const rects = await treemap(
  [{ text: "Rust", weight: 78 }, { text: "wasm", weight: 40 }],
  { width: 960, height: 600 },
);
// → [{ text, value, x, y, width, height }, ...]  — x/y are top-left.
```

A working page is in [`examples/treemap.html`](./examples/treemap.html).

---

## Architecture

The engine is a generic framework, not a single-purpose word-cloud. A **layout
strategy** implements the `LayoutEngine` trait:

```rust
pub trait LayoutEngine {
    type Item;
    type Config;
    type Output;
    fn layout(&self, items: &[Self::Item], config: &Self::Config) -> Vec<Self::Output>;
}
```

The three word-cloud modes are strategies over `Item → Placement`
([`Pretty`](./crate/src/layout.rs), `Balanced`, `Fast`).
[`Treemap`](./crate/src/treemap.rs) is a fully-implemented strategy with its own
output type, proving the framework generalizes beyond word clouds.

The expensive infrastructure — occupancy bitmap, spatial grid, mask
rasterization, SIMD kernels — lives in standalone modules (`bitmap`, `grid`,
`mask`, `raster`, `simd`) that any strategy can reuse.

**Adding a new layout:** circle packing, bubble, and label placement are
scaffolded in [`crate/src/scaffolds.rs`](./crate/src/scaffolds.rs) — each is a
real `impl LayoutEngine` (returning empty for now) with design notes on its
algorithm. Filling one in touches no existing strategy and no shared module.

---

## Running the tests

```sh
cd crate
cargo test                           # unit + integration tests
cargo test --features parallel       # also tests the parallel rasterization path
```

---

## Known limitations

- **Approximate glyph metrics, not real font contours.** Collision masks are
  filled boxes sized by a per-character advance-width table, not actual glyph
  outlines. Consequence: words can't nest into ascender/descender gaps, and the
  collision footprint doesn't exactly match what the browser renders. The
  `Rasterizer` trait is the seam for a future glyph-accurate backend.

- **Rotation not yet implemented.** The output carries a `rotation` field and
  the rasterizer supports 90°, but the spiral modes currently emit every word
  upright (`rotation: 0`).

- **Fast mode may skip words** when no gap exists in any row. In practice this
  doesn't happen until the canvas is extremely dense — all 50 000 words are
  placed in the benchmarks above.

---

## Verifying the WASM build

```js
import init, { ping, version } from "./pkg/wordcloud_layout.js";
await init();
console.log(ping());     // "wordcloud_layout v0.1.0 ready"
console.log(version());  // "0.1.0"
```

---

## Contributing

Contributions are welcome! See [`CONTRIBUTING.md`](./CONTRIBUTING.md) for build
setup, testing, and the (recommended) commit message style.
