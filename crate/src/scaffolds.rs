//! Scaffolds and design notes for the remaining Phase-4 layouts (Task 16).
//!
//! Each is a real [`LayoutEngine`](crate::engine::LayoutEngine) implementation so
//! the wiring is in place and the type-checks pass, but the body is a stub that
//! returns an empty result. The point is to show *exactly* where each future
//! layout plugs in and which shared infrastructure it reuses — adding one is
//! "fill in the body," not "rearchitect." [`Treemap`](crate::treemap::Treemap) is
//! the fully-implemented example.
//!
//! Stubs return empty (never `todo!()`/`panic!`) so they can't crash the WASM
//! module if accidentally invoked.

use serde::{Deserialize, Serialize};

use crate::engine::LayoutEngine;
use crate::models::{Item, LayoutConfig, Placement};

/// A positioned circle — output shape for the circle-based layouts below.
/// `x`/`y` are the center; draw with `ctx.arc(x, y, radius, 0, 2π)`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Circle {
    pub text: String,
    pub value: f32,
    pub x: f32,
    pub y: f32,
    pub radius: f32,
}

/// **Circle packing** (scaffold).
///
/// Goal: pack weight-sized circles tightly without overlap. Plan:
/// 1. Radius from weight (`r ∝ sqrt(weight)`), heaviest first.
/// 2. Seed near the center, then place each circle tangent to two already-placed
///    ones (the "front-chain" algorithm), or relax from random seeds.
/// 3. Reuse the [`SpatialGrid`](crate::grid) broad phase to find candidate
///    neighbors for the tangency / overlap checks in O(nearby) instead of O(n²) —
///    this is precisely the caller the grid has been waiting for (see
///    `BENCHMARKS.md`, Finding 2). Narrow phase is a circle–circle distance test,
///    no bitmap needed.
#[derive(Debug, Clone, Copy, Default)]
pub struct CirclePacking;

impl LayoutEngine for CirclePacking {
    type Item = Item;
    type Config = LayoutConfig;
    type Output = Circle;
    fn layout(&self, _items: &[Item], _config: &LayoutConfig) -> Vec<Circle> {
        Vec::new() // scaffold — see module/struct docs
    }
}

/// **Bubble layout** (scaffold).
///
/// A force-directed cousin of circle packing: circles attracted toward a center
/// (or category anchors) and repelled from each other until they settle. Plan:
/// radius from weight; iterate position updates with collision resolution, again
/// using the [`SpatialGrid`](crate::grid) to bound neighbor lookups per tick.
/// Bounded iteration count keeps runtime predictable (cf. Fast mode).
#[derive(Debug, Clone, Copy, Default)]
pub struct BubbleLayout;

impl LayoutEngine for BubbleLayout {
    type Item = Item;
    type Config = LayoutConfig;
    type Output = Circle;
    fn layout(&self, _items: &[Item], _config: &LayoutConfig) -> Vec<Circle> {
        Vec::new() // scaffold
    }
}

/// **Label placement** (scaffold).
///
/// Place text labels at/near fixed anchor points (map pins, scatter points)
/// without overlap. Plan: rasterize each label to a [`WordMask`](crate::mask) (the
/// existing rasterizer), then try candidate offsets around its anchor, accepting
/// the first that doesn't collide in the [`OccupancyBitmap`](crate::bitmap) — the
/// exact word-cloud collision machinery, driven by anchors instead of a spiral.
/// Output reuses [`Placement`] (text + center + font size). Anchors would arrive
/// as an extended item type in a future revision.
#[derive(Debug, Clone, Copy, Default)]
pub struct LabelPlacement;

impl LayoutEngine for LabelPlacement {
    type Item = Item;
    type Config = LayoutConfig;
    type Output = Placement;
    fn layout(&self, _items: &[Item], _config: &LayoutConfig) -> Vec<Placement> {
        Vec::new() // scaffold
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scaffolds_implement_the_trait_and_are_safe_to_call() {
        let it = vec![Item { text: "a".into(), weight: 1.0 }];
        let c = LayoutConfig::default();
        // They compile as LayoutEngine strategies and return cleanly (empty).
        assert!(CirclePacking.layout(&it, &c).is_empty());
        assert!(BubbleLayout.layout(&it, &c).is_empty());
        assert!(LabelPlacement.layout(&it, &c).is_empty());
    }
}
