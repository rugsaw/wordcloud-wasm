// Node-based SIMD vs scalar timing for the WASM layout build.
//
//   node _simd_bench.mjs <pkg-dir> <mode> <n> [reps]
//
// Loads the generated wasm from <pkg-dir>, runs layout_words on a deterministic
// dataset (identical to the native bench / browser harness), and prints the
// median latency. Run it once against a scalar pkg and once against a +simd128
// pkg to quantify the SIMD speedup. Node ≥16 supports wasm SIMD.

import { readFile } from "node:fs/promises";
import { pathToFileURL } from "node:url";
import { generate, configFor, WeightDist } from "./js/datasets.js";

const [pkgDir, mode = "pretty", nStr = "100", repsStr = "7"] = process.argv.slice(2);
if (!pkgDir) {
  console.error("usage: node _simd_bench.mjs <pkg-dir> <mode> <n> [reps]");
  process.exit(1);
}
const n = Number(nStr);
const reps = Number(repsStr);

const jsUrl = pathToFileURL(`${pkgDir}/wordcloud_layout.js`).href;
const { default: init, layout_words, LayoutMode } = await import(jsUrl);
const wasmBytes = await readFile(new URL(`${pkgDir}/wordcloud_layout_bg.wasm`, pathToFileURL(process.cwd() + "/")));
await init({ module_or_path: wasmBytes });

const modeEnum = { pretty: LayoutMode.Pretty, balanced: LayoutMode.Balanced, fast: LayoutMode.Fast }[mode];
const items = generate(n, WeightDist.Zipf, 1);
const cfg = configFor(n);

// Warm-up.
const warm = layout_words(items, modeEnum, cfg);

const times = [];
for (let r = 0; r < reps; r++) {
  const t0 = performance.now();
  layout_words(items, modeEnum, cfg);
  times.push(performance.now() - t0);
}
times.sort((a, b) => a - b);
const median = times[times.length >> 1];
console.log(
  `RESULT pkg=${pkgDir} mode=${mode} n=${n} placed=${warm.length} median=${median.toFixed(2)}ms reps=${reps}`,
);
