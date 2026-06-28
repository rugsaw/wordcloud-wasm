// Optional multi-threading for the *threaded* WASM build (Task 14).
//
// The default build is single-threaded and needs none of this. To use threads:
//   1. Build the threaded pkg (README → "Multi-threaded build").
//   2. Serve the page cross-origin isolated (COOP/COEP headers — see README).
//   3. `await initThreads()` once before calling `layout()`.
//
// Everything degrades gracefully: if the page isn't cross-origin isolated, or
// SharedArrayBuffer is unavailable, or the loaded pkg is the single-threaded
// build (no `initThreadPool` export), `initThreads()` returns 0 and `layout()`
// simply runs single-threaded. No caller code needs to branch.

import * as wasm from "../pkg/wordcloud_layout.js";
import { ready } from "./index.js";

/**
 * Whether the runtime can host WASM threads: SharedArrayBuffer must exist (which
 * browsers gate behind cross-origin isolation) and, when the flag is present, the
 * context must actually be cross-origin isolated.
 *
 * @returns {boolean}
 */
export function threadsSupported() {
  if (typeof SharedArrayBuffer === "undefined") return false;
  // `crossOriginIsolated` is the browser's COOP/COEP gate. Undefined off-browser
  // (e.g. Node) — don't treat that as a failure.
  return globalThis.crossOriginIsolated !== false;
}

/**
 * Start the WASM Rayon thread pool if possible. Idempotent-friendly: returns the
 * number of threads started, or 0 when threading is unavailable (in which case
 * layout runs single-threaded). Call once, before the first `layout()`.
 *
 * @param {number} [threads] desired pool size; defaults to `hardwareConcurrency`.
 * @returns {Promise<number>}
 */
export async function initThreads(threads) {
  await ready();
  // The single-threaded build doesn't export this; absence ⇒ no threads.
  if (typeof wasm.initThreadPool !== "function") return 0;
  if (!threadsSupported()) return 0;
  const n = Math.max(1, threads ?? globalThis.navigator?.hardwareConcurrency ?? 4);
  await wasm.initThreadPool(n);
  return n;
}
