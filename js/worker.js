// Web Worker that runs layout off the main thread.
//
// Loaded as a *module* worker so it can `import` the Task 07 wrapper directly:
//
//   new Worker(new URL("./js/worker.js", import.meta.url), { type: "module" });
//
// Protocol (see `js/client.js` for the promise-based main-thread side):
//
//   in:  { type: "layout", id, items, options }
//   out: { type: "result", id, placements }
//        { type: "error",  id, message }
//
// Only `type: "layout"` messages are handled; anything else is ignored so the
// worker can coexist with other channels on the same port. The wrapper's
// `layout()` already initializes the WASM module on first use (idempotent), so
// the worker stays stateless beyond that cached module.

import { layout } from "./index.js";

self.addEventListener("message", async (event) => {
  const msg = event.data;
  if (!msg || msg.type !== "layout") return;

  const { id, items, options } = msg;
  try {
    const placements = await layout(items, options);
    // Placements are plain objects → structured-clone-friendly, no transfer
    // list needed.
    self.postMessage({ type: "result", id, placements });
  } catch (err) {
    self.postMessage({
      type: "error",
      id,
      message: err && err.message ? err.message : String(err),
    });
  }
});
