// Type definitions for the JS wrapper (`js/index.js`).

/** A single weighted input word. Matches the engine's *Input Model*. */
export interface Item {
  text: string;
  weight: number;
}

/**
 * A placed word. Matches the engine's *Output Model*. `x`/`y` are the
 * bounding-box center — render with `textAlign: "center"` and
 * `textBaseline: "middle"`.
 */
export interface Placement {
  text: string;
  x: number;
  y: number;
  /** Rotation in degrees (0 or 90). */
  rotation: number;
  fontSize: number;
}

/**
 * A treemap tile. `x`/`y` are the **top-left** corner (unlike {@link Placement},
 * which is centered) — render with `ctx.fillRect(x, y, width, height)`.
 */
export interface TreemapRect {
  text: string;
  /** The item's weight (drives its area share). */
  value: number;
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface TreemapOptions {
  /** Canvas width in pixels. Defaults to 1024. */
  width?: number;
  /** Canvas height in pixels. Defaults to 768. */
  height?: number;
  /** Optional init input (custom wasm URL/bytes) forwarded to {@link ready}. */
  wasm?: unknown;
}

/** Friendly layout mode names accepted by {@link layout}. */
export type LayoutModeName = "pretty" | "balanced" | "fast";

export interface LayoutOptions {
  /** Placement strategy. Defaults to `"pretty"`. */
  mode?: LayoutModeName;
  /** Canvas width in pixels. Defaults to 1024. */
  width?: number;
  /** Canvas height in pixels. Defaults to 768. */
  height?: number;
  /** Smallest font size (px) for the lowest-weight word. */
  minFontSize?: number;
  /** Largest font size (px) for the highest-weight word. */
  maxFontSize?: number;
  /** Padding (px) added around each word to reduce crowding. */
  padding?: number;
  /** Optional init input (custom wasm URL/bytes) forwarded to {@link ready}. */
  wasm?: unknown;
}

/** The wasm `LayoutMode` enum, re-exported for advanced callers. */
export const LayoutMode: {
  readonly Pretty: number;
  readonly Balanced: number;
  readonly Fast: number;
};

/** Initialize the WASM module (idempotent). Usually you can just call {@link layout}. */
export function ready(input?: unknown): Promise<unknown>;

/** Lay out weighted words and resolve to their placements. */
export function layout(items: Item[], options?: LayoutOptions): Promise<Placement[]>;

/** Lay out weighted items as a squarified treemap (rectangles tiling the canvas). */
export function treemap(items: Item[], options?: TreemapOptions): Promise<TreemapRect[]>;
