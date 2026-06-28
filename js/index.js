// Ergonomic wrapper around the generated `pkg/` WebAssembly bindings.
//
// Hides the async `init()` dance and the raw `LayoutMode` enum behind a single
// promise-based `layout(items, options)` call that returns placements. The WASM
// module only *computes* placements — all rendering stays in caller code.
//
// Browser usage (no bundler needed — serve the project over http and):
//
//   import { layout } from "./js/index.js";
//   const placements = await layout(items, { mode: "pretty", width, height });
//
// See `js/index.d.ts` for full type definitions and `examples/canvas.html` for
// a working Canvas renderer.

import init, { layout_words, layout_treemap, LayoutMode } from "../pkg/wordcloud_layout.js";

/** Friendly mode names → the wasm `LayoutMode` enum. */
const MODES = {
  pretty: LayoutMode.Pretty,
  balanced: LayoutMode.Balanced,
  fast: LayoutMode.Fast,
};

/** Cached init promise so the wasm module is instantiated exactly once. */
let _ready = null;

/**
 * Ensure the WASM module is initialized. Idempotent: the first call starts
 * initialization, later calls await the same promise.
 *
 * @param {*} [input] Optional init input forwarded to the generated `init()`
 *   (e.g. a custom wasm URL/bytes). In a browser, omit it to fetch the wasm
 *   sitting next to the generated JS automatically.
 * @returns {Promise<unknown>}
 */
export function ready(input) {
  if (!_ready) {
    _ready = init(input === undefined ? undefined : { module_or_path: input });
  }
  return _ready;
}

/**
 * Lay out weighted words and return their placements.
 *
 * @param {Array<{text: string, weight: number}>} items
 * @param {object} [options]
 * @param {"pretty"|"balanced"|"fast"} [options.mode="pretty"]
 * @param {number} [options.width=1024]
 * @param {number} [options.height=768]
 * @param {number} [options.minFontSize]
 * @param {number} [options.maxFontSize]
 * @param {number} [options.padding]
 * @param {*} [options.wasm] Optional init input (see {@link ready}).
 * @returns {Promise<Array<{text: string, x: number, y: number, rotation: number, fontSize: number}>>}
 *   Placements; `x`/`y` are the bounding-box center (render with
 *   `textAlign: "center"`, `textBaseline: "middle"`).
 */
export async function layout(items, options = {}) {
  if (!Array.isArray(items)) {
    throw new TypeError("layout(items, options): `items` must be an array");
  }
  await ready(options.wasm);

  const {
    mode = "pretty",
    width = 1024,
    height = 768,
    minFontSize,
    maxFontSize,
    padding,
  } = options;

  const layoutMode = MODES[String(mode).toLowerCase()];
  if (layoutMode === undefined) {
    throw new RangeError(
      `unknown layout mode: ${JSON.stringify(mode)} (expected "pretty", "balanced", or "fast")`,
    );
  }

  // Only forward provided fields; omitted ones fall back to the engine's
  // per-field defaults (Task 02 `LayoutConfig`).
  const config = { width, height };
  if (minFontSize != null) config.minFontSize = minFontSize;
  if (maxFontSize != null) config.maxFontSize = maxFontSize;
  if (padding != null) config.padding = padding;

  return layout_words(items, layoutMode, config);
}

/**
 * Lay out weighted items as a **treemap** — rectangles whose areas are
 * proportional to weight, tiling the canvas with no overlaps (squarified).
 *
 * Demonstrates the engine generalizing beyond word clouds: same `{ text, weight }`
 * input, but a rectangle output. Render with `ctx.fillRect(x, y, width, height)`
 * (top-left origin).
 *
 * @param {Array<{text: string, weight: number}>} items
 * @param {object} [options]
 * @param {number} [options.width=1024]
 * @param {number} [options.height=768]
 * @param {*} [options.wasm] Optional init input (see {@link ready}).
 * @returns {Promise<Array<{text: string, value: number, x: number, y: number, width: number, height: number}>>}
 */
export async function treemap(items, options = {}) {
  if (!Array.isArray(items)) {
    throw new TypeError("treemap(items, options): `items` must be an array");
  }
  await ready(options.wasm);
  const { width = 1024, height = 768 } = options;
  return layout_treemap(items, { width, height });
}

export { LayoutMode };
