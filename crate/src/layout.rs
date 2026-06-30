//! Layout strategies.
//!
//! * **Pretty mode** ([`layout_pretty`]) — Archimedean-spiral placement verified
//!   directly against the [`OccupancyBitmap`]. Tuned for 50–500 words and a
//!   d3-cloud-like organic look (spec: *Pretty Mode*).
//! * **Balanced mode** ([`layout_balanced`]) — the same spiral, plus a
//!   [`SpatialGrid`] broad phase that narrows each candidate's collision test to
//!   the *nearby* placed words (spec: *Balanced Mode*). Produces output identical
//!   to Pretty. See the performance note below: in this engine the grid does not
//!   beat Pretty's bitmap, so Pretty is the better choice today.
//! * **Fast mode** ([`layout_fast`]) — row (shelf) packing for massive clouds
//!   (5000–50000+ words; spec: *Fast Mode*). No per-word search at all: words are
//!   packed into full-width rows in a single linear pass, so runtime is bounded
//!   and predictable. A justified block rather than an organic cloud — the trade
//!   chosen for throughput at scale.
//!
//! ## Spiral algorithm (shared)
//!
//! 1. Sort words by weight, descending (stable, so equal weights keep input
//!    order — the layout is fully deterministic).
//! 2. Map each weight to a font size by linear interpolation across the
//!    `[min_font_size, max_font_size]` range from [`LayoutConfig`].
//! 3. Rasterize each word to a [`WordMask`] (Task 04), padded per the config.
//! 4. Place the heaviest word at the canvas center; for every subsequent word,
//!    walk an Archimedean spiral outward from the center, testing each candidate
//!    until a collision-free spot is found.
//! 5. On success, record the placement and mark it occupied.
//!
//! ## Pretty vs. Balanced collision test
//!
//! Both walk the identical spiral and produce identical placements. They differ
//! only in how a candidate position is judged free:
//!
//! * **Pretty** runs the precise [`OccupancyBitmap::collides`] test on *every*
//!   candidate.
//! * **Balanced** first asks the [`SpatialGrid`] (via the allocation-free
//!   [`SpatialGrid::neighbors_into`]) for the words near the candidate's bounding
//!   box. If an in-bounds candidate has *no* nearby word whose bbox overlaps, it
//!   is accepted with no bitmap test; otherwise the precise bitmap test decides.
//!   This is the spec's *Spatial Grid → Nearby Words → Collision Test* pipeline.
//!
//! Both modes are correct (no overlaps) and produce *identical* output: the grid
//! is a superset of true bbox overlaps, disjoint bounding boxes can't have
//! overlapping masks, and the in-bounds guard mirrors the bitmap's treatment of
//! the canvas edge.
//!
//! **Performance note (see `BENCHMARKS.md`):** the spatial-grid broad phase does
//! *not* make Balanced faster than Pretty in this engine, and is in fact ~2×
//! slower. The reason is that [`OccupancyBitmap::collides`] is already O(mask
//! area) and **independent of the number of placed words** — the bitmap collapses
//! all placed words into one bitfield — so Pretty never performs the O(placed)
//! "compare against every word" scan the grid is designed to replace. The grid's
//! genuine payoff is in Fast mode (Task 13), which packs by grid cell without a
//! per-pixel spiral or occupancy bitmap. Balanced is retained for spec
//! completeness and shares this infrastructure. The dominant cost of *both* modes
//! is the number of spiral candidates, which the [`Spiral`]'s word-scaled step
//! keeps low.
//!
//! ## Coordinate convention
//!
//! Internally placement works in integer top-left offsets `(ox, oy)` to match
//! the occupancy bitmap. The emitted [`Placement`] reports `x`/`y` as the
//! **center** of the word's bounding box, so a JS renderer can draw with
//! `textAlign = "center"` / `textBaseline = "middle"`.
//!
//! ## No-fit handling
//!
//! If the spiral reaches beyond the canvas's corner radius without finding a
//! gap, the word is **skipped** (not placed). The canvas is never resized, which
//! keeps output dimensions stable and the run deterministic. Consequently the
//! output count is `≤` the input count.

use crate::bitmap::OccupancyBitmap;
use crate::engine::LayoutEngine;
use crate::grid::SpatialGrid;
use crate::mask::WordMask;
use crate::models::{Item, LayoutConfig, Placement, WordPlacement};
use crate::raster::rasterize_word;

use std::f32::consts::PI;

/// Instrumentation gathered during a layout run.
///
/// Used by tests to compare collision cost between modes; the public layout
/// functions discard it.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct LayoutStats {
    /// Number of precise per-pixel [`OccupancyBitmap::collides`] calls made.
    pub bitmap_tests: usize,
    /// (Balanced only) Total word-vs-word bbox comparisons performed via the
    /// grid's neighbor sets — the cost the spatial grid is meant to bound.
    pub broad_comparisons: usize,
    /// (Balanced only) Comparisons a naive "test against every placed word"
    /// spiral would have made for the same candidates (the baseline the grid
    /// improves on). Always `>= broad_comparisons`.
    pub naive_comparisons: usize,
    /// Number of words successfully placed.
    pub placed: usize,
}

/// Lay out `items` using Pretty mode, returning placements ordered heaviest
/// first. Deterministic for a given `items` + `config`.
pub fn layout_pretty(items: &[Item], config: &LayoutConfig) -> Vec<Placement> {
    layout_pretty_counted(items, config).0
}

/// Lay out `items` using Balanced mode (spiral + spatial-grid broad phase).
/// Deterministic for a given `items` + `config`.
pub fn layout_balanced(items: &[Item], config: &LayoutConfig) -> Vec<Placement> {
    layout_balanced_counted(items, config).0
}

// --- Strategies (the generic `LayoutEngine` abstraction) ---------------------
//
// The three word-cloud modes as pluggable strategies over `Item → Placement`.
// They carry no state; the heavy lifting lives in the shared services
// (`Prepared`/`Spiral`/`OccupancyBitmap`/`SpatialGrid`). `layout_words` dispatches
// through these, and the free `layout_*` functions above delegate to them too, so
// there is a single code path per mode.

/// Pretty-mode strategy: per-pixel-verified Archimedean spiral.
#[derive(Debug, Clone, Copy, Default)]
pub struct Pretty;

/// Balanced-mode strategy: spiral with a spatial-grid broad phase.
#[derive(Debug, Clone, Copy, Default)]
pub struct Balanced;

/// Fast-mode strategy: row (shelf) packing for massive clouds.
#[derive(Debug, Clone, Copy, Default)]
pub struct Fast;

impl LayoutEngine for Pretty {
    type Item = Item;
    type Config = LayoutConfig;
    type Output = Placement;
    fn layout(&self, items: &[Item], config: &LayoutConfig) -> Vec<Placement> {
        layout_pretty_counted(items, config).0
    }
}

impl LayoutEngine for Balanced {
    type Item = Item;
    type Config = LayoutConfig;
    type Output = Placement;
    fn layout(&self, items: &[Item], config: &LayoutConfig) -> Vec<Placement> {
        layout_balanced_counted(items, config).0
    }
}

impl LayoutEngine for Fast {
    type Item = Item;
    type Config = LayoutConfig;
    type Output = Placement;
    fn layout(&self, items: &[Item], config: &LayoutConfig) -> Vec<Placement> {
        layout_fast_counted(items, config).0
    }
}

/// [`layout_pretty`] plus collision-check instrumentation.
pub(crate) fn layout_pretty_counted(
    items: &[Item],
    config: &LayoutConfig,
) -> (Vec<Placement>, LayoutStats) {
    let prepared = match Prepared::new(items, config) {
        Some(p) => p,
        None => return (Vec::new(), LayoutStats::default()),
    };

    let mut occ = OccupancyBitmap::new(prepared.canvas_w, prepared.canvas_h);
    let mut stats = LayoutStats::default();
    let mut placements = Vec::with_capacity(prepared.words.len());

    for word in &prepared.words {
        let spot = find_spot_pretty(&occ, &word.mask, &prepared, &mut stats);
        if let Some((ox, oy)) = spot {
            occ.occupy(&word.mask, ox, oy);
            placements.push(word.to_placement(ox, oy));
        }
    }

    stats.placed = placements.len();
    (placements, stats)
}

/// [`layout_balanced`] plus collision-check instrumentation.
pub(crate) fn layout_balanced_counted(
    items: &[Item],
    config: &LayoutConfig,
) -> (Vec<Placement>, LayoutStats) {
    let prepared = match Prepared::new(items, config) {
        Some(p) => p,
        None => return (Vec::new(), LayoutStats::default()),
    };

    let mut occ = OccupancyBitmap::new(prepared.canvas_w, prepared.canvas_h);

    // Size grid cells so a typical word spans ~2x2 cells (Task 09 heuristic).
    let (avg_w, avg_h) = prepared.average_mask_dims();
    let cell_size = SpatialGrid::recommended_cell_size(avg_w, avg_h);
    let mut grid = SpatialGrid::new(prepared.canvas_w, prepared.canvas_h, cell_size);

    // Bounding box of each placed word, indexed by WordId == placement index, so
    // the grid's neighbor ids can be resolved to boxes for the narrow bbox test.
    let mut placed_boxes: Vec<BBox> = Vec::with_capacity(prepared.words.len());
    let mut stats = LayoutStats::default();
    let mut placements = Vec::with_capacity(prepared.words.len());
    // One neighbor buffer, reused for every candidate of every word (the grid
    // fills it without allocating — see SpatialGrid::neighbors_into).
    let mut near: Vec<crate::grid::WordId> = Vec::new();

    for word in &prepared.words {
        let spot = find_spot_balanced(
            &occ,
            &mut grid,
            &placed_boxes,
            &mut near,
            &word.mask,
            &prepared,
            &mut stats,
        );
        if let Some((ox, oy)) = spot {
            occ.occupy(&word.mask, ox, oy);
            let bbox = BBox { x: ox, y: oy, w: word.mask.width(), h: word.mask.height() };
            grid.insert(placements.len(), bbox.x, bbox.y, bbox.w, bbox.h);
            placed_boxes.push(bbox);
            placements.push(word.to_placement(ox, oy));
        }
    }

    stats.placed = placements.len();
    (placements, stats)
}

/// A word ready to place: its font size, rasterized mask, and source text.
struct PreparedWord {
    text: String,
    font_size: f32,
    mask: WordMask,
}

impl PreparedWord {
    fn to_placement(&self, ox: i32, oy: i32) -> Placement {
        let wp = WordPlacement {
            x: ox as f32 + self.mask.width() as f32 / 2.0,
            y: oy as f32 + self.mask.height() as f32 / 2.0,
            width: self.mask.width() as f32,
            height: self.mask.height() as f32,
            font_size: self.font_size,
            rotation: 0, // spiral modes keep words upright
        };
        Placement::from_word_placement(self.text.clone(), &wp)
    }
}

/// Rasterize one input item into a [`PreparedWord`]. Pure and side-effect free
/// (the rasterizer holds no shared state), so it is safe to call concurrently —
/// which is what lets `Prepared::new` fan mask generation across Rayon.
fn prepare_word(
    item: &Item,
    min_w: f32,
    max_w: f32,
    config: &LayoutConfig,
    padding: u32,
) -> PreparedWord {
    let font_size = font_size_for(item.weight, min_w, max_w, config);
    let mask = rasterize_word(&item.text, font_size, 0, padding);
    PreparedWord { text: item.text.clone(), font_size, mask }
}

/// Shared setup common to every spiral mode: sorted+rasterized words plus canvas
/// geometry. `None` when there is nothing to lay out.
struct Prepared {
    words: Vec<PreparedWord>,
    canvas_w: u32,
    canvas_h: u32,
    cx: f32,
    cy: f32,
    max_r: f32,
}

impl Prepared {
    fn new(items: &[Item], config: &LayoutConfig) -> Option<Self> {
        if items.is_empty() {
            return None;
        }

        // Stable sort by descending weight.
        let mut order: Vec<usize> = (0..items.len()).collect();
        order.sort_by(|&a, &b| {
            items[b]
                .weight
                .partial_cmp(&items[a].weight)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let (min_w, max_w) = weight_bounds(items);
        let padding = config.padding.round().max(0.0) as u32;

        // Rasterizing each word into its mask is independent work — the single
        // biggest cost for large clouds and the prime parallelization target
        // (spec: *Threading Strategy*). With the `parallel` feature this fans out
        // across Rayon's thread pool (OS threads natively; a Web Worker pool on
        // wasm via `wasm-threads`); otherwise it's a plain sequential map. The
        // closure is identical either way — see [`prepare_word`].
        let words: Vec<PreparedWord> = {
            #[cfg(feature = "parallel")]
            {
                use rayon::prelude::*;
                order
                    .par_iter()
                    .map(|&i| prepare_word(&items[i], min_w, max_w, config, padding))
                    .collect()
            }
            #[cfg(not(feature = "parallel"))]
            {
                order
                    .iter()
                    .map(|&i| prepare_word(&items[i], min_w, max_w, config, padding))
                    .collect()
            }
        };

        Some(Prepared {
            words,
            canvas_w: config.width.round().max(1.0) as u32,
            canvas_h: config.height.round().max(1.0) as u32,
            cx: config.width / 2.0,
            cy: config.height / 2.0,
            // Half the canvas diagonal — far enough for the spiral to reach any corner.
            max_r: 0.5 * config.width.hypot(config.height),
        })
    }

    /// Average mask width/height across all prepared words (at least 1 each).
    fn average_mask_dims(&self) -> (u32, u32) {
        let n = self.words.len().max(1) as u64;
        let (mut sw, mut sh) = (0u64, 0u64);
        for w in &self.words {
            sw += w.mask.width() as u64;
            sh += w.mask.height() as u64;
        }
        ((sw / n).max(1) as u32, (sh / n).max(1) as u32)
    }
}

/// An axis-aligned bounding box in pixel space (top-left origin).
#[derive(Debug, Clone, Copy)]
struct BBox {
    x: i32,
    y: i32,
    w: u32,
    h: u32,
}

/// Standard AABB overlap test (touching edges do not count as overlapping).
fn bboxes_overlap(a: &BBox, b: &BBox) -> bool {
    let (ax1, ay1) = (a.x, a.y);
    let (ax2, ay2) = (a.x + a.w as i32, a.y + a.h as i32);
    let (bx1, by1) = (b.x, b.y);
    let (bx2, by2) = (b.x + b.w as i32, b.y + b.h as i32);
    ax1 < bx2 && ax2 > bx1 && ay1 < by2 && ay2 > by1
}

/// Min and max weight across `items` (equal when all weights match).
fn weight_bounds(items: &[Item]) -> (f32, f32) {
    let mut min_w = f32::INFINITY;
    let mut max_w = f32::NEG_INFINITY;
    for it in items {
        min_w = min_w.min(it.weight);
        max_w = max_w.max(it.weight);
    }
    (min_w, max_w)
}

/// Linearly map `weight` from `[min_w, max_w]` onto the config's font-size
/// range. When all weights are equal, the largest font size is used.
fn font_size_for(weight: f32, min_w: f32, max_w: f32, config: &LayoutConfig) -> f32 {
    if max_w <= min_w {
        return config.max_font_size;
    }
    let t = ((weight - min_w) / (max_w - min_w)).clamp(0.0, 1.0);
    config.min_font_size + t * (config.max_font_size - config.min_font_size)
}

/// Fraction of a word's *smaller* dimension used as the spiral step. Probing
/// finer than this wastes work — a gap smaller than the word can't hold it
/// anyway — so the step scales with word size instead of being a fixed pixel
/// count. Smaller = denser packing but more candidates; larger = faster but
/// looser. ~0.5 keeps packing tight while cutting candidates by orders of
/// magnitude versus a 1–2 px step.
const SPIRAL_STEP_FACTOR: f32 = 0.5;
/// Floor on the spiral step so tiny words (or degenerate masks) still advance.
const SPIRAL_MIN_STEP: f32 = 3.0;

/// Archimedean spiral candidate generator. Yields integer top-left offsets for a
/// mask of the given half-extents, walking outward from `(cx, cy)` until the
/// radius exceeds `max_r`.
///
/// The step size (`step_px`) scales with the word's smaller dimension, and the
/// ring spacing is matched to it (`b = step_px / 2π`), so candidates are spaced
/// roughly `step_px` apart both along and across the arms. Total candidates is
/// therefore ~`π·max_r² / step_px²` — for a word-sized step this is dramatically
/// fewer than a fixed 1–2 px spiral, which is the dominant layout cost (see
/// `BENCHMARKS.md`). The exact canvas center is always emitted first so the
/// heaviest word lands dead-center.
struct Spiral {
    cx: f32,
    cy: f32,
    half_w: f32,
    half_h: f32,
    max_r: f32,
    b: f32,
    step_px: f32,
    theta: f32,
    emitted_center: bool,
}

impl Spiral {
    fn new(cx: f32, cy: f32, mask: &WordMask, max_r: f32) -> Self {
        let min_dim = mask.width().min(mask.height()) as f32;
        let step_px = (min_dim * SPIRAL_STEP_FACTOR).max(SPIRAL_MIN_STEP);
        Spiral {
            cx,
            cy,
            half_w: mask.width() as f32 / 2.0,
            half_h: mask.height() as f32 / 2.0,
            max_r,
            // Ring spacing (radial gap per full turn) ≈ step_px.
            b: step_px / (2.0 * PI),
            step_px,
            theta: 0.0,
            emitted_center: false,
        }
    }

    fn candidate_at(&self, r: f32, theta: f32) -> (i32, i32) {
        let px = self.cx + r * theta.cos();
        let py = self.cy + r * theta.sin();
        ((px - self.half_w).round() as i32, (py - self.half_h).round() as i32)
    }
}

impl Iterator for Spiral {
    type Item = (i32, i32);

    fn next(&mut self) -> Option<(i32, i32)> {
        // First candidate: the exact center (r = 0). Then jump to θ = 2π, where
        // r = b·2π = step_px, so the inner ring sits one step out and dθ = step/r
        // never divides by a near-zero radius.
        if !self.emitted_center {
            self.emitted_center = true;
            self.theta = 2.0 * PI;
            return Some(self.candidate_at(0.0, 0.0));
        }
        let r = self.b * self.theta;
        if r > self.max_r {
            return None;
        }
        let candidate = self.candidate_at(r, self.theta);
        // Constant arc-length step along the curve (r ≥ step_px here, so safe).
        self.theta += self.step_px / r;
        Some(candidate)
    }
}

/// Pretty mode: accept the first candidate that passes the precise bitmap test.
fn find_spot_pretty(
    occ: &OccupancyBitmap,
    mask: &WordMask,
    prep: &Prepared,
    stats: &mut LayoutStats,
) -> Option<(i32, i32)> {
    for (ox, oy) in Spiral::new(prep.cx, prep.cy, mask, prep.max_r) {
        stats.bitmap_tests += 1;
        if !occ.collides(mask, ox, oy) {
            return Some((ox, oy));
        }
    }
    None
}

/// Balanced mode: use the grid to skip the bitmap test wherever the candidate's
/// bbox has no nearby placed word.
fn find_spot_balanced(
    occ: &OccupancyBitmap,
    grid: &mut SpatialGrid,
    placed_boxes: &[BBox],
    near: &mut Vec<crate::grid::WordId>,
    mask: &WordMask,
    prep: &Prepared,
    stats: &mut LayoutStats,
) -> Option<(i32, i32)> {
    let (w, h) = (mask.width(), mask.height());
    let placed_count = placed_boxes.len();
    for (ox, oy) in Spiral::new(prep.cx, prep.cy, mask, prep.max_r) {
        let cand = BBox { x: ox, y: oy, w, h };

        // The candidate must sit fully on the canvas; the bitmap counts any
        // off-canvas pixel as a collision, so Pretty never places past an edge
        // and Balanced must match. Off-canvas candidates fall through to the
        // precise test (which rejects them), keeping the two modes identical.
        let in_bounds = ox >= 0
            && oy >= 0
            && ox + w as i32 <= prep.canvas_w as i32
            && oy + h as i32 <= prep.canvas_h as i32;

        // Broad phase: of the placed words, the grid hands back only those near
        // the candidate (into our reused buffer, no allocation). We bbox-test
        // just those, vs. the `placed_count` a naive spiral would test — that
        // gap is the grid's payoff.
        grid.neighbors_into(ox, oy, w, h, near);
        stats.broad_comparisons += near.len();
        stats.naive_comparisons += placed_count;
        let bbox_hit = near
            .iter()
            .any(|&id| bboxes_overlap(&cand, &placed_boxes[id]));

        if in_bounds && !bbox_hit {
            // In bounds with no overlapping bbox → masks can't overlap → free,
            // no per-pixel test needed.
            debug_assert!(
                !occ.collides(mask, ox, oy),
                "free-path accepted ({ox},{oy}) but bitmap reports a collision"
            );
            return Some((ox, oy));
        }

        // Narrow phase: precise per-pixel verification (also rejects off-canvas).
        stats.bitmap_tests += 1;
        if !occ.collides(mask, ox, oy) {
            return Some((ox, oy));
        }
    }
    None
}

/// Lay out `items` using Fast mode: row (shelf) packing for massive clouds
/// (5000–50000+ words). Deterministic for a given `items` + `config`.
///
/// Unlike the spiral modes, this never searches per word. Words (heaviest first)
/// are packed left-to-right into full-width **rows**; when the next word wouldn't
/// fit the current row, a new row is started. Each row's height is its tallest
/// word. Rows are then stacked outward from the vertical center (so the heaviest
/// words sit in the middle band) and each row is centered horizontally. This is
/// the classic shelf bin-packing for variable-width rectangles — the right shape
/// for *text*, which is wide and short — and it tiles the canvas densely.
///
/// Properties matching the spec's *Fast Mode* / *Large Cloud* targets:
/// * **Bounded, predictable runtime** — building rows and placing words are both
///   a single linear pass, O(words); there is no per-word search to blow up.
/// * **No overlaps by construction** — rows occupy disjoint horizontal bands and
///   words within a row occupy disjoint x-spans, so no per-pixel collision search
///   is needed. (Each placement is still bitmap-verified as a cheap safety net,
///   which also rejects anything that would fall off-canvas.)
/// * **Graceful skipping** — a word wider than the canvas, or in a row that would
///   fall outside the canvas vertically, is skipped (output count is `≤` input).
///
/// The trade is artistry: the result is a justified block, not an organic cloud.
/// That is the spec's intended Fast-mode bargain — throughput over looks.
pub fn layout_fast(items: &[Item], config: &LayoutConfig) -> Vec<Placement> {
    layout_fast_counted(items, config).0
}

/// One packed row: its words (indices into the prepared list), total width, and
/// height (the tallest word in it).
struct Shelf {
    words: Vec<usize>,
    width: i32,
    height: i32,
}

/// [`layout_fast`] plus collision-check instrumentation (`bitmap_tests`).
pub(crate) fn layout_fast_counted(
    items: &[Item],
    config: &LayoutConfig,
) -> (Vec<Placement>, LayoutStats) {
    let prepared = match Prepared::new(items, config) {
        Some(p) => p,
        None => return (Vec::new(), LayoutStats::default()),
    };

    let canvas_w = prepared.canvas_w as i32;
    let canvas_h = prepared.canvas_h as i32;
    let mut occ = OccupancyBitmap::new(prepared.canvas_w, prepared.canvas_h);

    // Pass 1: greedily pack words (already heaviest-first) into full-width rows.
    let mut shelves: Vec<Shelf> = Vec::new();
    let mut cur = Shelf { words: Vec::new(), width: 0, height: 0 };
    for (i, word) in prepared.words.iter().enumerate() {
        let w = word.mask.width() as i32;
        let h = word.mask.height() as i32;
        if w > canvas_w {
            continue; // can never fit on a row
        }
        if !cur.words.is_empty() && cur.width + w > canvas_w {
            shelves.push(std::mem::replace(&mut cur, Shelf { words: Vec::new(), width: 0, height: 0 }));
        }
        cur.width += w;
        cur.height = cur.height.max(h);
        cur.words.push(i);
    }
    if !cur.words.is_empty() {
        shelves.push(cur);
    }

    // Pass 2: stack rows outward from the vertical center (heaviest row centered),
    // each row centered horizontally, and place its words left-to-right.
    let mut stats = LayoutStats::default();
    let mut placements = Vec::with_capacity(prepared.words.len());

    // Running top edges for the next row below / above the center band.
    //
    // The alternating pattern places row 0 at `center_top`, odd rows below it,
    // and even rows (≥ 2) above it. To center the whole block the midpoint of
    // the block's vertical span must equal canvas_h / 2:
    //
    //   block_top    = center_top - up_h          (top of highest "up" row)
    //   block_bottom = center_top + h0 + down_h   (bottom of lowest "down" row)
    //   midpoint     = center_top + (h0 + down_h - up_h) / 2
    //
    // Setting midpoint = canvas_h / 2 gives:
    //   center_top = canvas_h / 2 - (h0 + down_h - up_h) / 2
    //
    // When content overflows the canvas center_top can be negative; the
    // existing `top < 0` guard in the placement loop already clips those rows.
    let center_top = if shelves.is_empty() {
        0
    } else {
        let h0 = shelves[0].height;
        let down_h: i32 = shelves.iter().enumerate()
            .filter(|(i, _)| i % 2 == 1)
            .map(|(_, s)| s.height)
            .sum();
        let up_h: i32 = shelves.iter().enumerate()
            .filter(|(i, _)| *i >= 2 && i % 2 == 0)
            .map(|(_, s)| s.height)
            .sum();
        canvas_h / 2 - (h0 + down_h - up_h) / 2
    };
    let mut next_down = center_top; // top of the next row to place going down
    let mut next_up = center_top; // bottom of the next row to place going up

    for (idx, shelf) in shelves.iter().enumerate() {
        // Row 0 sits at the center band; then alternate below, above, below, …
        let top = if idx == 0 {
            next_down = center_top + shelf.height;
            center_top
        } else if idx % 2 == 1 {
            let t = next_down;
            next_down += shelf.height;
            t
        } else {
            next_up -= shelf.height;
            next_up
        };

        // Skip rows that would fall off the canvas vertically.
        if top < 0 || top + shelf.height > canvas_h {
            continue;
        }

        let mut x = (canvas_w - shelf.width) / 2; // ≥ 0 since width ≤ canvas_w
        for &wi in &shelf.words {
            let word = &prepared.words[wi];
            let w = word.mask.width() as i32;
            let h = word.mask.height() as i32;
            let ox = x;
            let oy = top + (shelf.height - h) / 2; // vertically center within the row
            x += w;

            // Safety verify (also rejects off-canvas). Construction already keeps
            // rows/words disjoint, so this passes for every in-bounds placement.
            stats.bitmap_tests += 1;
            if !occ.collides(&word.mask, ox, oy) {
                occ.occupy(&word.mask, ox, oy);
                placements.push(word.to_placement(ox, oy));
            }
        }
    }

    stats.placed = placements.len();
    (placements, stats)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> LayoutConfig {
        LayoutConfig {
            width: 800.0,
            height: 600.0,
            min_font_size: 10.0,
            max_font_size: 60.0,
            padding: 1.0,
        }
    }

    fn items(n: usize) -> Vec<Item> {
        // Distinct descending-ish weights so font sizes vary.
        (0..n)
            .map(|i| Item {
                text: format!("word{i}"),
                weight: (n - i) as f32,
            })
            .collect()
    }

    /// Re-run a result's masks through a fresh bitmap and assert none overlap.
    fn assert_no_overlap(out: &[Placement], it: &[Item], config: &LayoutConfig) {
        let mut occ = OccupancyBitmap::new(config.width as u32, config.height as u32);
        let (min_w, max_w) = weight_bounds(it);
        for p in out {
            let item = it.iter().find(|i| i.text == p.text).unwrap();
            let fs = font_size_for(item.weight, min_w, max_w, config);
            let mask = rasterize_word(&item.text, fs, 0, config.padding as u32);
            let ox = (p.x - mask.width() as f32 / 2.0).round() as i32;
            let oy = (p.y - mask.height() as f32 / 2.0).round() as i32;
            assert!(
                !occ.collides(&mask, ox, oy),
                "placement for {:?} overlaps an earlier word",
                p.text
            );
            occ.occupy(&mask, ox, oy);
        }
    }

    #[test]
    fn empty_input_yields_empty_output() {
        assert!(layout_pretty(&[], &cfg()).is_empty());
        assert!(layout_balanced(&[], &cfg()).is_empty());
    }

    #[test]
    fn output_count_does_not_exceed_input() {
        let it = items(120);
        let out = layout_pretty(&it, &cfg());
        assert!(out.len() <= it.len());
        assert!(!out.is_empty());
    }

    #[test]
    fn larger_weight_gets_larger_font() {
        let it = vec![
            Item { text: "big".into(), weight: 100.0 },
            Item { text: "small".into(), weight: 1.0 },
        ];
        let out = layout_pretty(&it, &cfg());
        assert_eq!(out[0].text, "big");
        let big = out.iter().find(|p| p.text == "big").unwrap();
        let small = out.iter().find(|p| p.text == "small").unwrap();
        assert!(big.font_size > small.font_size);
    }

    #[test]
    fn placements_do_not_overlap() {
        let config = cfg();
        let it = items(150);
        let out = layout_pretty(&it, &config);
        assert_no_overlap(&out, &it, &config);
    }

    #[test]
    fn deterministic_across_runs() {
        let it = items(80);
        assert_eq!(layout_pretty(&it, &cfg()), layout_pretty(&it, &cfg()));
    }

    #[test]
    fn heaviest_word_is_centered() {
        let config = cfg();
        let it = items(30);
        let out = layout_pretty(&it, &config);
        let center = &out[0];
        assert!((center.x - config.width / 2.0).abs() <= 1.5);
        assert!((center.y - config.height / 2.0).abs() <= 1.5);
    }

    // --- Balanced mode -------------------------------------------------------

    #[test]
    fn balanced_placements_do_not_overlap() {
        let config = cfg();
        let it = items(60);
        let out = layout_balanced(&it, &config);
        assert!(!out.is_empty());
        assert_no_overlap(&out, &it, &config);
    }

    #[test]
    fn balanced_is_deterministic() {
        let it = items(60);
        assert_eq!(layout_balanced(&it, &cfg()), layout_balanced(&it, &cfg()));
    }

    #[test]
    fn balanced_matches_pretty_placement() {
        // Same spiral + same correctness ⇒ the grid only *skips redundant tests*,
        // it must not change where words land. Results should be identical.
        let it = items(60);
        let pretty = layout_pretty(&it, &cfg());
        let balanced = layout_balanced(&it, &cfg());
        assert_eq!(pretty, balanced);
    }

    #[test]
    fn balanced_checks_far_fewer_pairs_than_checking_every_word() {
        // The spatial grid's purpose (spec: *Spatial Grid System*): instead of
        // testing each spiral candidate against *every* placed word, test it
        // against only nearby words. So the actual word-vs-word comparisons the
        // grid performs must be a small fraction of the naive all-pairs baseline.
        let config = LayoutConfig { width: 900.0, height: 680.0, ..cfg() };
        let it = items(80);
        let (_, stats) = layout_balanced_counted(&it, &config);
        assert!(stats.broad_comparisons <= stats.naive_comparisons);
        assert!(
            stats.broad_comparisons * 4 < stats.naive_comparisons,
            "grid should examine far fewer pairs than all-pairs: grid={}, naive={}",
            stats.broad_comparisons,
            stats.naive_comparisons
        );
    }

    #[test]
    fn balanced_neighbor_cost_stays_bounded_as_words_grow() {
        // As more words are placed, a naive spiral's per-candidate comparison cost
        // grows with the word count; the grid keeps it roughly constant (a query
        // returns only the handful of words near the candidate). So the grid's
        // advantage ratio should *widen* with scale rather than hold flat.
        let cfg_for = |n| LayoutConfig { width: 600.0 + n as f32 * 4.0, height: 440.0 + n as f32 * 3.0, ..cfg() };
        let (_, small) = layout_balanced_counted(&items(25), &cfg_for(25));
        let (_, large) = layout_balanced_counted(&items(70), &cfg_for(70));

        let ratio = |s: &LayoutStats| s.naive_comparisons as f64 / s.broad_comparisons.max(1) as f64;
        assert!(
            ratio(&large) > ratio(&small),
            "grid advantage should grow with word count: small={:.1}x, large={:.1}x",
            ratio(&small),
            ratio(&large)
        );
    }

    #[test]
    fn balanced_handles_larger_input_and_stays_correct() {
        // A mid-size cloud sized so (nearly) every word fits — full 5000-word
        // timing lives in the release-build benchmark (Task 12); debug-mode spiral
        // walking is too slow at that scale. Verify it places most words without
        // overlap and that the grid keeps comparison cost well under all-pairs.
        let config = LayoutConfig { width: 1100.0, height: 820.0, ..cfg() };
        let it = items(70);
        let (out, stats) = layout_balanced_counted(&it, &config);
        assert!(out.len() >= 64, "expected most words to fit, placed {}", out.len());
        assert_no_overlap(&out, &it, &config);
        assert!(
            stats.broad_comparisons * 4 < stats.naive_comparisons,
            "grid should examine far fewer pairs than all-pairs: grid={}, naive={}",
            stats.broad_comparisons,
            stats.naive_comparisons
        );
    }

    // --- Fast mode -----------------------------------------------------------

    #[test]
    fn fast_empty_input_yields_empty_output() {
        assert!(layout_fast(&[], &cfg()).is_empty());
    }

    #[test]
    fn fast_output_count_does_not_exceed_input() {
        let it = items(300);
        let out = layout_fast(&it, &cfg());
        assert!(out.len() <= it.len());
        assert!(!out.is_empty());
    }

    #[test]
    fn fast_placements_do_not_overlap() {
        let config = LayoutConfig { width: 1400.0, height: 1000.0, ..cfg() };
        let it = items(300);
        let out = layout_fast(&it, &config);
        assert!(!out.is_empty());
        assert_no_overlap(&out, &it, &config);
    }

    #[test]
    fn fast_is_deterministic() {
        let it = items(300);
        assert_eq!(layout_fast(&it, &cfg()), layout_fast(&it, &cfg()));
    }

    #[test]
    fn fast_heaviest_word_lands_in_center_band() {
        // The heaviest word is in row 0, which is stacked at the vertical center.
        // (Shelf packing left-aligns within a row, so it's not horizontally
        // centered — Fast mode trades artistry for throughput.)
        let config = LayoutConfig { width: 1400.0, height: 1000.0, ..cfg() };
        let it = items(300);
        let out = layout_fast(&it, &config);
        let first = &out[0];
        assert!(
            (first.y - config.height / 2.0).abs() < config.height * 0.25,
            "heaviest y={} not in the center band (H={})",
            first.y,
            config.height
        );
    }

    #[test]
    fn fast_block_is_vertically_centered_for_small_counts() {
        // With few words (≤ ~50) the entire content block fits well inside the
        // canvas. The fix ensures center_top is computed from the full block
        // height so the block midpoint sits near canvas_h/2, not in the lower
        // half (which was the bug: center_top used only first.height).
        let config = LayoutConfig { width: 800.0, height: 600.0, min_font_size: 6.0, max_font_size: 16.0, padding: 1.0 };
        let it = items(50);
        let out = layout_fast(&it, &config);
        assert!(!out.is_empty());
        let min_y = out.iter().map(|p| p.y).fold(f32::INFINITY, f32::min);
        let max_y = out.iter().map(|p| p.y).fold(f32::NEG_INFINITY, f32::max);
        let block_mid = (min_y + max_y) / 2.0;
        let canvas_mid = config.height / 2.0;
        assert!(
            (block_mid - canvas_mid).abs() < config.height * 0.15,
            "block midpoint {block_mid:.1} is too far from canvas center {canvas_mid:.1}",
        );
    }

    #[test]
    fn fast_places_most_words_when_space_is_ample() {
        // Generous canvas ⇒ every row fits vertically ⇒ shelf packing places all.
        let config = LayoutConfig { width: 2400.0, height: 1800.0, ..cfg() };
        let it = items(400);
        let out = layout_fast(&it, &config);
        assert!(out.len() >= 360, "expected ≥90% placed, got {}/400", out.len());
    }

    #[test]
    fn fast_search_is_linear() {
        // The defining property: a single linear pass — each word is examined
        // (bitmap-verified) at most once, so runtime is predictable and bounded.
        let config = LayoutConfig { width: 1400.0, height: 1000.0, ..cfg() };
        let it = items(500);
        let (_, stats) = layout_fast_counted(&it, &config);
        assert!(
            stats.bitmap_tests <= it.len(),
            "expected ≤ one probe per word, got {} for {} words",
            stats.bitmap_tests,
            it.len()
        );
    }

    #[test]
    #[ignore = "stress; run explicitly: cargo test --release fast_stress_50000 -- --ignored"]
    fn fast_stress_50000_no_overlap_and_bounded() {
        // Massive-cloud acceptance: 50000 words place without overlap in bounded
        // time. Sized small fonts + a large canvas so most words fit.
        let config = LayoutConfig {
            width: 9000.0,
            height: 6500.0,
            min_font_size: 6.0,
            max_font_size: 16.0,
            padding: 1.0,
        };
        let it = items(50000);
        let (out, stats) = layout_fast_counted(&it, &config);
        assert!(out.len() >= 40000, "placed only {}/50000", out.len());
        assert!(stats.bitmap_tests <= it.len(), "search was not linear");
        assert_no_overlap(&out, &it, &config);
    }

    // --- Generic engine ------------------------------------------------------

    #[test]
    fn strategies_match_free_functions_through_the_trait() {
        // All three modes run through `LayoutEngine::layout` and must produce the
        // same result as the free functions — no behavior regression from the
        // generic abstraction.
        use crate::engine::LayoutEngine;
        let it = items(60);
        let c = cfg();
        assert_eq!(Pretty.layout(&it, &c), layout_pretty(&it, &c));
        assert_eq!(Balanced.layout(&it, &c), layout_balanced(&it, &c));
        assert_eq!(Fast.layout(&it, &c), layout_fast(&it, &c));
        assert!(!Pretty.layout(&it, &c).is_empty());
    }
}
