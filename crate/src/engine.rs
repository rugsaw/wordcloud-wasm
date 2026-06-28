//! Generic layout-engine abstraction (spec: *Generic Layout Engine*).
//!
//! The expensive infrastructure — the [`OccupancyBitmap`](crate::bitmap),
//! [`SpatialGrid`](crate::grid), [`WordMask`](crate::mask) rasterization, and the
//! SIMD kernels — is deliberately *not* tied to word clouds. A **layout strategy**
//! is anything that turns a slice of input items into a list of placements, and is
//! expressed as an implementation of [`LayoutEngine`].
//!
//! The three word-cloud modes (Pretty, Balanced, Fast) are strategies over
//! `Item → Placement` (see [`crate::layout`]). A future visualization — a treemap,
//! a circle packing — is just another `impl LayoutEngine` with its own item,
//! config, and output types, reusing whichever shared services it needs. Adding
//! one touches no existing strategy and no shared module.
//!
//! Associated types (rather than hard-wiring `Item`/`Placement`) are what make the
//! trait reusable across visualization families that don't share an input or
//! output shape.

/// A pluggable layout strategy: place [`items`](LayoutEngine::layout) onto a
/// canvas, returning their computed positions.
///
/// Implementors choose their own input/config/output types, so the same trait
/// serves word clouds (`Item`/`LayoutConfig`/`Placement`) and, later, other
/// visualizations. Rendering is always the caller's job — a strategy only
/// computes geometry.
pub trait LayoutEngine {
    /// The input element type (e.g. a weighted word).
    type Item;
    /// Canvas / tuning parameters for a run.
    type Config;
    /// The computed placement type handed back to the renderer.
    type Output;

    /// Compute placements for `items` under `config`. Deterministic for a given
    /// input is encouraged but not required by the trait.
    fn layout(&self, items: &[Self::Item], config: &Self::Config) -> Vec<Self::Output>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Item, LayoutConfig, Placement};

    /// A brand-new strategy defined entirely in this test module — it imports no
    /// shared layout internals, demonstrating that strategies are pluggable
    /// without touching the shared infrastructure (acceptance criterion #3).
    struct StackAtOrigin;

    impl LayoutEngine for StackAtOrigin {
        type Item = Item;
        type Config = LayoutConfig;
        type Output = Placement;

        fn layout(&self, items: &[Item], _config: &LayoutConfig) -> Vec<Placement> {
            items
                .iter()
                .map(|it| Placement {
                    text: it.text.clone(),
                    x: 0.0,
                    y: 0.0,
                    rotation: 0,
                    font_size: 12.0,
                })
                .collect()
        }
    }

    #[test]
    fn a_new_strategy_plugs_in_without_touching_shared_code() {
        let items = vec![
            Item { text: "a".into(), weight: 1.0 },
            Item { text: "b".into(), weight: 2.0 },
        ];
        let out = StackAtOrigin.layout(&items, &LayoutConfig::default());
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].text, "a");
    }
}
