//! Treemap layout — the first non-word-cloud strategy, proving the
//! [`LayoutEngine`](crate::engine::LayoutEngine) framework generalizes (Task 16).
//!
//! A treemap tiles a rectangle into sub-rectangles whose **areas are proportional
//! to each item's weight**. This implementation uses the *squarified* algorithm
//! (Bruls, Huizing & van Wijk, 2000), which greedily groups items into rows along
//! the shorter edge of the remaining space so the rectangles stay close to square
//! (good aspect ratios) instead of degenerating into thin slivers.
//!
//! Why Treemap as the demonstrator (vs. circle packing): it is fully
//! deterministic and its correctness is exactly checkable — the output must tile
//! the canvas with **no overlaps and no gaps**, and each area must match its
//! weight share. That makes it an unambiguous proof that the engine is a generic
//! layout framework. Circle packing (iterative relaxation) and the other Phase-4
//! layouts are scaffolded in [`crate::scaffolds`].
//!
//! It reuses the engine's input ([`Item`]) and canvas config ([`LayoutConfig`]),
//! but defines its own output type ([`TreemapRect`]) — rectangles, not word
//! placements — which is exactly what associated `Output` types on the trait are
//! for.

use serde::{Deserialize, Serialize};

use crate::engine::LayoutEngine;
use crate::models::{Item, LayoutConfig};

/// One tile of the treemap: a rectangle whose area is proportional to `value`.
///
/// Serializes to the camelCase shape JS renderers expect. `x`/`y` are the
/// **top-left** corner (unlike word-cloud [`Placement`](crate::models::Placement),
/// which centers) — draw with `ctx.fillRect(x, y, width, height)`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TreemapRect {
    /// The item's label.
    pub text: String,
    /// The item's weight (its area share driver).
    pub value: f32,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

/// Squarified-treemap layout strategy.
#[derive(Debug, Clone, Copy, Default)]
pub struct Treemap;

impl LayoutEngine for Treemap {
    type Item = Item;
    type Config = LayoutConfig;
    type Output = TreemapRect;

    fn layout(&self, items: &[Item], config: &LayoutConfig) -> Vec<TreemapRect> {
        let (cw, ch) = (config.width.max(0.0) as f64, config.height.max(0.0) as f64);
        if cw <= 0.0 || ch <= 0.0 {
            return Vec::new();
        }

        // Only positive weights have an area; sort descending (stable on index
        // for determinism) as squarified expects.
        let mut weighted: Vec<(usize, f64)> = items
            .iter()
            .enumerate()
            .filter(|(_, it)| it.weight > 0.0)
            .map(|(i, it)| (i, it.weight as f64))
            .collect();
        weighted.sort_by(|a, b| {
            b.1.partial_cmp(&a.1)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(a.0.cmp(&b.0))
        });

        let total: f64 = weighted.iter().map(|(_, v)| v).sum();
        if total <= 0.0 {
            return Vec::new();
        }

        // Scale weights to canvas area so the tiles fill the rectangle exactly.
        let area = cw * ch;
        let scaled: Vec<(usize, f64)> = weighted.iter().map(|&(i, v)| (i, v / total * area)).collect();

        let placed = squarify(&scaled, Rect { x: 0.0, y: 0.0, w: cw, h: ch });

        placed
            .into_iter()
            .map(|(i, r)| TreemapRect {
                text: items[i].text.clone(),
                value: items[i].weight,
                x: r.x as f32,
                y: r.y as f32,
                width: r.w as f32,
                height: r.h as f32,
            })
            .collect()
    }
}

/// A rectangle in canvas pixel space (f64 for tiling precision).
#[derive(Debug, Clone, Copy)]
struct Rect {
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}

/// Worst (largest) aspect ratio in a candidate row of `areas` laid along an edge
/// of length `side`. Lower is squarer. (The squarified-treemap heuristic.)
fn worst_ratio(areas: &[f64], side: f64) -> f64 {
    let sum: f64 = areas.iter().sum();
    if sum <= 0.0 || side <= 0.0 {
        return f64::INFINITY;
    }
    let max = areas.iter().cloned().fold(f64::MIN, f64::max);
    let min = areas.iter().cloned().fold(f64::MAX, f64::min);
    let s2 = side * side;
    let sum2 = sum * sum;
    (s2 * max / sum2).max(sum2 / (s2 * min))
}

/// Squarified tiling of `items` (each `(index, area)`, areas summing to the
/// rect's area) into `rect`. Returns `(index, rect)` pairs that tile `rect`
/// exactly with no overlaps.
fn squarify(items: &[(usize, f64)], mut rect: Rect) -> Vec<(usize, Rect)> {
    let mut out = Vec::with_capacity(items.len());
    let mut i = 0;
    while i < items.len() {
        let side = rect.w.min(rect.h);

        // Grow the current row while it keeps (or improves) squareness.
        let mut row_areas: Vec<f64> = vec![items[i].1];
        let mut j = i + 1;
        while j < items.len() {
            let current = worst_ratio(&row_areas, side);
            row_areas.push(items[j].1);
            let extended = worst_ratio(&row_areas, side);
            if extended <= current {
                j += 1; // keep items[j] in the row
            } else {
                row_areas.pop(); // adding it made things worse — close the row
                break;
            }
        }

        lay_row(&items[i..j], &mut rect, &mut out);
        i = j;
    }
    out
}

/// Place one finished row as a strip along the shorter edge of `rect`, then
/// shrink `rect` to the remaining space.
fn lay_row(row: &[(usize, f64)], rect: &mut Rect, out: &mut Vec<(usize, Rect)>) {
    let row_area: f64 = row.iter().map(|&(_, a)| a).sum();
    if row_area <= 0.0 {
        return;
    }

    if rect.w <= rect.h {
        // Horizontal strip across the top; its height holds the whole row's area.
        let strip_h = row_area / rect.w;
        let mut x = rect.x;
        for &(idx, a) in row {
            let w = a / row_area * rect.w;
            out.push((idx, Rect { x, y: rect.y, w, h: strip_h }));
            x += w;
        }
        rect.y += strip_h;
        rect.h -= strip_h;
    } else {
        // Vertical strip down the left.
        let strip_w = row_area / rect.h;
        let mut y = rect.y;
        for &(idx, a) in row {
            let h = a / row_area * rect.h;
            out.push((idx, Rect { x: rect.x, y, w: strip_w, h }));
            y += h;
        }
        rect.x += strip_w;
        rect.w -= strip_w;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> LayoutConfig {
        LayoutConfig { width: 800.0, height: 600.0, ..LayoutConfig::default() }
    }

    fn items(weights: &[f32]) -> Vec<Item> {
        weights
            .iter()
            .enumerate()
            .map(|(i, &w)| Item { text: format!("t{i}"), weight: w })
            .collect()
    }

    fn rects_overlap(a: &TreemapRect, b: &TreemapRect) -> bool {
        // Half-open; touching edges don't count. Allow a tiny epsilon for floats.
        let eps = 1e-3_f32;
        a.x + eps < b.x + b.width
            && a.x + a.width > b.x + eps
            && a.y + eps < b.y + b.height
            && a.y + a.height > b.y + eps
    }

    #[test]
    fn empty_input_yields_empty_output() {
        assert!(Treemap.layout(&[], &cfg()).is_empty());
        // All-zero weights → nothing to size.
        assert!(Treemap.layout(&items(&[0.0, 0.0]), &cfg()).is_empty());
    }

    #[test]
    fn single_item_fills_the_canvas() {
        let out = Treemap.layout(&items(&[5.0]), &cfg());
        assert_eq!(out.len(), 1);
        let r = &out[0];
        assert!((r.x).abs() < 1e-3 && (r.y).abs() < 1e-3);
        assert!((r.width - 800.0).abs() < 1e-2);
        assert!((r.height - 600.0).abs() < 1e-2);
    }

    #[test]
    fn tiles_do_not_overlap() {
        let config = cfg();
        let it = items(&[10.0, 7.0, 6.0, 4.0, 3.0, 2.0, 2.0, 1.0, 1.0, 1.0]);
        let out = Treemap.layout(&it, &config);
        assert_eq!(out.len(), it.len());
        for a in 0..out.len() {
            for b in (a + 1)..out.len() {
                assert!(
                    !rects_overlap(&out[a], &out[b]),
                    "tiles {a} {:?} and {b} {:?} overlap",
                    out[a],
                    out[b]
                );
            }
        }
    }

    #[test]
    fn tiles_stay_within_canvas_and_fill_it() {
        let config = cfg();
        let it = items(&[5.0, 4.0, 3.0, 2.0, 2.0, 1.0, 1.0]);
        let out = Treemap.layout(&it, &config);
        let mut area = 0.0_f64;
        for r in &out {
            assert!(r.x >= -1e-3 && r.y >= -1e-3);
            assert!(r.x + r.width <= config.width + 1e-2);
            assert!(r.y + r.height <= config.height + 1e-2);
            area += r.width as f64 * r.height as f64;
        }
        // Squarified tiles the whole canvas.
        assert!((area - (config.width * config.height) as f64).abs() < 1.0, "area {area}");
    }

    #[test]
    fn area_is_proportional_to_weight() {
        let config = cfg();
        let it = items(&[8.0, 4.0, 2.0, 2.0]); // total 16
        let out = Treemap.layout(&it, &config);
        let total_area = (config.width * config.height) as f64;
        for r in &out {
            let expected = r.value as f64 / 16.0 * total_area;
            let actual = r.width as f64 * r.height as f64;
            let rel = (actual - expected).abs() / expected;
            assert!(rel < 1e-3, "{} area off: expected {expected}, got {actual}", r.text);
        }
    }

    #[test]
    fn deterministic_across_runs() {
        let it = items(&[3.0, 1.0, 4.0, 1.0, 5.0, 9.0, 2.0, 6.0]);
        assert_eq!(Treemap.layout(&it, &cfg()), Treemap.layout(&it, &cfg()));
    }

    #[test]
    fn ignores_non_positive_weights() {
        let it = items(&[5.0, 0.0, 3.0, -1.0]);
        let out = Treemap.layout(&it, &cfg());
        // Only the two positive-weight items are tiled.
        assert_eq!(out.len(), 2);
        assert!(out.iter().all(|r| r.value > 0.0));
    }
}
