// Type definitions for the layout Web Worker client (`js/client.js`).

import type { Item, LayoutOptions, Placement } from "./index";

export interface LayoutClientOptions {
  /**
   * Bring your own Worker. If omitted, a module worker is spawned from
   * `worker.js` next to `client.js`.
   */
  worker?: Worker;
}

export interface LayoutClient {
  /** The underlying worker, exposed for advanced use. */
  readonly worker: Worker;
  /** Lay out weighted words on the worker. Resolves with placements. */
  layout(items: Item[], options?: LayoutOptions): Promise<Placement[]>;
  /** Terminate the worker and reject any in-flight calls. */
  terminate(): void;
}

/** Create a layout client backed by a dedicated Web Worker. */
export function createLayoutClient(opts?: LayoutClientOptions): LayoutClient;
