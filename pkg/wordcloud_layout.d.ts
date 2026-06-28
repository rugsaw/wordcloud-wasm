/* tslint:disable */
/* eslint-disable */

/**
 * Layout strategy selector exposed to JavaScript.
 *
 * Mirrors the spec's *Layout Modes*. All three — [`LayoutMode::Pretty`],
 * [`LayoutMode::Balanced`], and [`LayoutMode::Fast`] — are implemented.
 */
export enum LayoutMode {
    Pretty = 0,
    Balanced = 1,
    Fast = 2,
}

/**
 * Lay out `input` as a **treemap**, returning an array of `TreemapRect`
 * (`{ text, value, x, y, width, height }`, top-left origin).
 *
 * A separate entry point from [`layout_words`] because the treemap strategy has
 * a different output shape (rectangles, not word placements); both go through
 * the same generic [`LayoutEngine`](engine::LayoutEngine) trait. `config` reuses
 * the *LayoutConfig* options object (only `width`/`height` are consulted).
 */
export function layout_treemap(input: any, config: any): any;

/**
 * Single entry point: lay out `input` words using `mode`, returning the spec's
 * *Output Model* array as a JS value.
 *
 * * `input` — the *Input Model* array of `{ text, weight }` objects.
 * * `mode` — strategy selector; see [`LayoutMode`].
 * * `config` — optional *LayoutConfig* options object; `null`/`undefined`
 *   yields defaults, and omitted fields fall back individually.
 *
 * Returns a rejected `Result` (surfaced to JS as a thrown exception) on
 * malformed input or an unimplemented mode. No `unwrap` is performed on
 * user-supplied data, so bad input can never panic the module.
 */
export function layout_words(input: any, mode: LayoutMode, config: any): any;

/**
 * Trivial liveness check used to verify the JS <-> WASM toolchain works.
 *
 * Returns a human-readable greeting. Reachable from JavaScript as `ping()`.
 */
export function ping(): string;

/**
 * Returns the crate version string, e.g. `"0.1.0"`.
 *
 * Reachable from JavaScript as `version()`.
 */
export function version(): string;

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly layout_treemap: (a: any, b: any) => [number, number, number];
    readonly layout_words: (a: any, b: number, c: any) => [number, number, number];
    readonly ping: () => [number, number];
    readonly version: () => [number, number];
    readonly __wbindgen_malloc: (a: number, b: number) => number;
    readonly __wbindgen_realloc: (a: number, b: number, c: number, d: number) => number;
    readonly __wbindgen_exn_store: (a: number) => void;
    readonly __externref_table_alloc: () => number;
    readonly __wbindgen_externrefs: WebAssembly.Table;
    readonly __externref_table_dealloc: (a: number) => void;
    readonly __wbindgen_free: (a: number, b: number, c: number) => void;
    readonly __wbindgen_start: () => void;
}

export type SyncInitInput = BufferSource | WebAssembly.Module;

/**
 * Instantiates the given `module`, which can either be bytes or
 * a precompiled `WebAssembly.Module`.
 *
 * @param {{ module: SyncInitInput }} module - Passing `SyncInitInput` directly is deprecated.
 *
 * @returns {InitOutput}
 */
export function initSync(module: { module: SyncInitInput } | SyncInitInput): InitOutput;

/**
 * If `module_or_path` is {RequestInfo} or {URL}, makes a request and
 * for everything else, calls `WebAssembly.instantiate` directly.
 *
 * @param {{ module_or_path: InitInput | Promise<InitInput> }} module_or_path - Passing `InitInput` directly is deprecated.
 *
 * @returns {Promise<InitOutput>}
 */
export default function __wbg_init (module_or_path?: { module_or_path: InitInput | Promise<InitInput> } | InitInput | Promise<InitInput>): Promise<InitOutput>;
