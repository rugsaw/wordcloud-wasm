// Main-thread client for the layout Web Worker (`js/worker.js`).
//
// Wraps the worker's `postMessage` protocol in a promise-based API with request
// id matching, so the UI thread never blocks on layout:
//
//   import { createLayoutClient } from "./js/client.js";
//   const client = createLayoutClient();
//   const placements = await client.layout(items, { mode: "pretty", width, height });
//   // ...later, to free the worker:
//   client.terminate();
//
// Each `layout()` call gets a unique id; the matching `result`/`error` message
// resolves/rejects that call's promise. Concurrent calls are supported — replies
// can arrive in any order.

/**
 * Create a layout client backed by a dedicated Web Worker.
 *
 * @param {object} [opts]
 * @param {Worker} [opts.worker] Bring your own Worker (e.g. a custom URL or a
 *   classic worker). If omitted, a module worker is spawned from `worker.js`
 *   next to this file.
 * @returns {{ layout(items: Array, options?: object): Promise<Array>, terminate(): void, worker: Worker }}
 */
export function createLayoutClient(opts = {}) {
  const worker =
    opts.worker ??
    new Worker(new URL("./worker.js", import.meta.url), { type: "module" });

  /** @type {Map<number, { resolve: Function, reject: Function }>} */
  const pending = new Map();
  let nextId = 1;

  worker.addEventListener("message", (event) => {
    const msg = event.data;
    if (!msg || (msg.type !== "result" && msg.type !== "error")) return;

    const entry = pending.get(msg.id);
    if (!entry) return; // unknown/stale id — ignore
    pending.delete(msg.id);

    if (msg.type === "result") {
      entry.resolve(msg.placements);
    } else {
      entry.reject(new Error(msg.message || "layout worker error"));
    }
  });

  worker.addEventListener("error", (event) => {
    // A worker-level error (e.g. failed import) has no request id — fail every
    // in-flight call so callers don't hang forever.
    const err = new Error(event.message || "layout worker crashed");
    for (const { reject } of pending.values()) reject(err);
    pending.clear();
  });

  return {
    worker,

    /**
     * Lay out weighted words on the worker.
     *
     * @param {Array<{text: string, weight: number}>} items
     * @param {object} [options] Same options as the wrapper's `layout()`.
     * @returns {Promise<Array<{text: string, x: number, y: number, rotation: number, fontSize: number}>>}
     */
    layout(items, options = {}) {
      const id = nextId++;
      return new Promise((resolve, reject) => {
        pending.set(id, { resolve, reject });
        worker.postMessage({ type: "layout", id, items, options });
      });
    },

    /** Terminate the worker and reject any in-flight calls. */
    terminate() {
      const err = new Error("layout client terminated");
      for (const { reject } of pending.values()) reject(err);
      pending.clear();
      worker.terminate();
    },
  };
}
