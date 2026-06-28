//! High-Performance WASM Layout Engine.
//!
//! Reusable Rust + WebAssembly layout engine for word clouds and, in later
//! phases, general visualization layouts. See `wasm_layout_engine_v2.md` for
//! the full design.
//!
//! This file currently contains only the scaffolding required to prove the
//! build toolchain end-to-end (Task 01). Layout data models, the occupancy
//! bitmap, word masks, and the placement strategies are added in subsequent
//! tasks.

use wasm_bindgen::prelude::*;

pub mod bitmap;
pub mod datasets;
pub mod engine;
pub mod grid;
pub mod layout;
pub mod mask;
pub mod models;
pub mod raster;
pub mod scaffolds;
pub mod simd;
pub mod treemap;

pub use bitmap::OccupancyBitmap;
pub use engine::LayoutEngine;
pub use grid::{SpatialGrid, WordId};
pub use layout::{layout_balanced, layout_fast, layout_pretty, Balanced, Fast, Pretty};
pub use mask::WordMask;
pub use models::{Item, LayoutConfig, Placement, WordPlacement};
pub use raster::{rasterize_word, MetricsRasterizer, Rasterizer, Rotation};
pub use treemap::{Treemap, TreemapRect};

/// Crate version, sourced from `Cargo.toml` at compile time.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Initialize the Rayon Web Worker thread pool (threaded wasm build only).
///
/// Re-exported from `wasm-bindgen-rayon` as `initThreadPool(numThreads)` for JS;
/// it must be `await`ed once, in a cross-origin-isolated context, before any
/// parallel layout call. Present only when built with the `wasm-threads` feature
/// (which requires a threaded toolchain — see the README). The default build
/// omits it, and JS detects its absence to fall back to single-threaded.
#[cfg(all(target_arch = "wasm32", feature = "wasm-threads"))]
pub use wasm_bindgen_rayon::init_thread_pool;

/// Deserialize the JS input value into a `Vec<Item>`.
///
/// Accepts the spec's *Input Model* array. Errors (rather than panicking) on
/// malformed input so the WASM boundary can surface a clean exception to JS.
pub fn items_from_js(input: JsValue) -> Result<Vec<Item>, JsValue> {
    serde_wasm_bindgen::from_value(input)
        .map_err(|e| JsValue::from_str(&format!("invalid input items: {e}")))
}

/// Deserialize a JS options object into a [`LayoutConfig`].
///
/// A `null`/`undefined` value yields [`LayoutConfig::default`], and any omitted
/// fields fall back to their defaults.
pub fn config_from_js(config: JsValue) -> Result<LayoutConfig, JsValue> {
    if config.is_null() || config.is_undefined() {
        return Ok(LayoutConfig::default());
    }
    serde_wasm_bindgen::from_value(config)
        .map_err(|e| JsValue::from_str(&format!("invalid layout config: {e}")))
}

/// Serialize layout output into a JS value matching the spec's *Output Model*.
pub fn placements_to_js(placements: &[Placement]) -> Result<JsValue, JsValue> {
    serde_wasm_bindgen::to_value(placements)
        .map_err(|e| JsValue::from_str(&format!("failed to serialize placements: {e}")))
}

/// Layout strategy selector exposed to JavaScript.
///
/// Mirrors the spec's *Layout Modes*. All three — [`LayoutMode::Pretty`],
/// [`LayoutMode::Balanced`], and [`LayoutMode::Fast`] — are implemented.
#[wasm_bindgen]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutMode {
    Pretty,
    Balanced,
    Fast,
}

/// Single entry point: lay out `input` words using `mode`, returning the spec's
/// *Output Model* array as a JS value.
///
/// * `input` — the *Input Model* array of `{ text, weight }` objects.
/// * `mode` — strategy selector; see [`LayoutMode`].
/// * `config` — optional *LayoutConfig* options object; `null`/`undefined`
///   yields defaults, and omitted fields fall back individually.
///
/// Returns a rejected `Result` (surfaced to JS as a thrown exception) on
/// malformed input or an unimplemented mode. No `unwrap` is performed on
/// user-supplied data, so bad input can never panic the module.
#[wasm_bindgen]
pub fn layout_words(input: JsValue, mode: LayoutMode, config: JsValue) -> Result<JsValue, JsValue> {
    let items = items_from_js(input)?;
    let cfg = config_from_js(config)?;

    // Dispatch through the generic `LayoutEngine` trait — each mode is a strategy.
    let placements = match mode {
        LayoutMode::Pretty => layout::Pretty.layout(&items, &cfg),
        LayoutMode::Balanced => layout::Balanced.layout(&items, &cfg),
        LayoutMode::Fast => layout::Fast.layout(&items, &cfg),
    };

    placements_to_js(&placements)
}

/// Lay out `input` as a **treemap**, returning an array of `TreemapRect`
/// (`{ text, value, x, y, width, height }`, top-left origin).
///
/// A separate entry point from [`layout_words`] because the treemap strategy has
/// a different output shape (rectangles, not word placements); both go through
/// the same generic [`LayoutEngine`](engine::LayoutEngine) trait. `config` reuses
/// the *LayoutConfig* options object (only `width`/`height` are consulted).
#[wasm_bindgen]
pub fn layout_treemap(input: JsValue, config: JsValue) -> Result<JsValue, JsValue> {
    let items = items_from_js(input)?;
    let cfg = config_from_js(config)?;
    let rects = treemap::Treemap.layout(&items, &cfg);
    serde_wasm_bindgen::to_value(&rects)
        .map_err(|e| JsValue::from_str(&format!("failed to serialize treemap: {e}")))
}

/// Trivial liveness check used to verify the JS <-> WASM toolchain works.
///
/// Returns a human-readable greeting. Reachable from JavaScript as `ping()`.
#[wasm_bindgen]
pub fn ping() -> String {
    format!("wordcloud_layout v{VERSION} ready")
}

/// Returns the crate version string, e.g. `"0.1.0"`.
///
/// Reachable from JavaScript as `version()`.
#[wasm_bindgen]
pub fn version() -> String {
    VERSION.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ping_mentions_crate_and_version() {
        let msg = ping();
        assert!(msg.contains("wordcloud_layout"));
        assert!(msg.contains(VERSION));
    }

    #[test]
    fn version_matches_cargo() {
        assert_eq!(version(), env!("CARGO_PKG_VERSION"));
    }
}
