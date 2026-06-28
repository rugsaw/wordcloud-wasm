//! Uniform spatial grid for broad-phase collision culling.
//!
//! For large clouds, testing a candidate position against *every* placed word is
//! O(n) per probe and O(n²) overall. A [`SpatialGrid`] buckets placed words by
//! the cells their bounding box overlaps; a candidate then looks up only the
//! cells *it* overlaps and tests against that small neighbor set
//! (see the spec's *Spatial Grid System*).
//!
//! This is the reusable broad phase. The narrow phase — exact overlap — stays
//! with the [`OccupancyBitmap`](crate::OccupancyBitmap) / [`WordMask`](crate::WordMask).
//! Balanced and Fast modes (Tasks 10/13) wire this in; Pretty mode does not need
//! it.
//!
//! ## Correctness contract
//!
//! [`SpatialGrid::neighbors`] returns a **superset** of the words whose bounding
//! boxes could overlap the query box: any word sharing a cell with the query is
//! reported, and two boxes that truly overlap always share a cell, so there are
//! **no false negatives**. False positives (nearby-but-not-touching words) are
//! expected and cheap — the narrow phase rejects them.
//!
//! ## Coordinates
//!
//! Boxes are given as a top-left `(x, y)` in pixel space plus `width`/`height`.
//! `x`/`y` are `i32` so a word may extend slightly off-canvas (negative or past
//! the edge); such overflow is clamped to the border cells, which keeps the
//! result a correct superset.
//!
//! ## Choosing `cell_size`
//!
//! The cell size trades insert cost against neighbor-set size:
//!
//! * **Too small** → each word spans many cells (expensive inserts, large
//!   neighbor lists from a single big word).
//! * **Too large** → every word lands in one shared cell and `neighbors`
//!   degenerates back toward "check everything".
//!
//! The sweet spot makes a *typical* word span roughly a 2×2 block of cells, so a
//! query touches ~9 cells regardless of total word count. See
//! [`SpatialGrid::recommended_cell_size`].

/// Identifies a placed word, by its index in the layout's placement list.
pub type WordId = usize;

/// A uniform grid that buckets placed words into fixed-size square cells.
#[derive(Debug, Clone)]
pub struct SpatialGrid {
    cell_size: u32,
    cols: usize,
    rows: usize,
    /// Row-major `cols * rows` buckets; `cells[row * cols + col]` holds the ids
    /// of words overlapping that cell.
    cells: Vec<Vec<WordId>>,
    /// Per-word "last seen generation" stamp, used by [`SpatialGrid::neighbors_into`]
    /// to deduplicate a query's result without sorting or allocating. Indexed by
    /// [`WordId`]; grown on [`insert`](SpatialGrid::insert).
    seen: Vec<u64>,
    /// Monotonic query counter; bumped once per `neighbors_into` call so a word
    /// pushed in the current query can be recognized in O(1).
    generation: u64,
}

impl SpatialGrid {
    /// Create a grid covering a `width`×`height` canvas with square cells of
    /// `cell_size` pixels.
    ///
    /// `cell_size` is clamped to at least 1, and the grid always has at least one
    /// cell, so degenerate inputs can't panic.
    pub fn new(width: u32, height: u32, cell_size: u32) -> Self {
        let cell_size = cell_size.max(1);
        let cols = div_ceil(width, cell_size).max(1);
        let rows = div_ceil(height, cell_size).max(1);
        SpatialGrid {
            cell_size,
            cols,
            rows,
            cells: vec![Vec::new(); cols * rows],
            seen: Vec::new(),
            generation: 0,
        }
    }

    /// Suggested `cell_size` for words whose typical footprint is
    /// `word_w`×`word_h` pixels.
    ///
    /// Heuristic: size cells to roughly half the larger word dimension so a
    /// typical word spans about a 2×2 block. This keeps both per-word insert
    /// cost and per-query neighbor counts small and (importantly) independent of
    /// the total number of words. Returns at least 1.
    pub fn recommended_cell_size(word_w: u32, word_h: u32) -> u32 {
        (word_w.max(word_h) / 2).max(1)
    }

    /// Number of columns (cells along the x axis).
    pub fn cols(&self) -> usize {
        self.cols
    }

    /// Number of rows (cells along the y axis).
    pub fn rows(&self) -> usize {
        self.rows
    }

    /// The cell edge length in pixels (after clamping).
    pub fn cell_size(&self) -> u32 {
        self.cell_size
    }

    /// Insert word `id` into every cell its bounding box overlaps.
    ///
    /// Boxes with zero `width` or `height` cover no cells and are ignored.
    pub fn insert(&mut self, id: WordId, x: i32, y: i32, width: u32, height: u32) {
        let Some((c0, c1, r0, r1)) = self.cell_span(x, y, width, height) else {
            return;
        };
        // Keep the dedup stamp table large enough to index any inserted id.
        if id >= self.seen.len() {
            self.seen.resize(id + 1, 0);
        }
        for row in r0..=r1 {
            let base = row * self.cols;
            for col in c0..=c1 {
                self.cells[base + col].push(id);
            }
        }
    }

    /// Return the ids of words sharing any cell with the query box, each id once.
    ///
    /// This is a superset of true overlaps (see the module-level correctness
    /// contract); callers run an exact test on the returned candidates.
    pub fn neighbors(&self, x: i32, y: i32, width: u32, height: u32) -> Vec<WordId> {
        let mut out = Vec::new();
        let Some((c0, c1, r0, r1)) = self.cell_span(x, y, width, height) else {
            return out;
        };
        for row in r0..=r1 {
            let base = row * self.cols;
            for col in c0..=c1 {
                out.extend_from_slice(&self.cells[base + col]);
            }
        }
        out.sort_unstable();
        out.dedup();
        out
    }

    /// Allocation-free [`neighbors`](SpatialGrid::neighbors) for hot loops.
    ///
    /// Writes the deduplicated neighbor ids into `out` (cleared first), reusing
    /// the caller's buffer so a query that runs millions of times (e.g. once per
    /// spiral candidate in Balanced mode) does **no** per-call heap allocation.
    /// Deduplication uses a per-word generation stamp instead of `sort`+`dedup`,
    /// so the cost is linear in the ids scanned. The result is the same *set* as
    /// [`neighbors`](SpatialGrid::neighbors) but **not sorted** (callers that run
    /// an order-independent exact test, like an AABB overlap, don't care).
    pub fn neighbors_into(&mut self, x: i32, y: i32, width: u32, height: u32, out: &mut Vec<WordId>) {
        out.clear();
        let Some((c0, c1, r0, r1)) = self.cell_span(x, y, width, height) else {
            return;
        };
        // A fresh generation: any id whose stamp already equals it was pushed by
        // *this* query and must be skipped.
        self.generation += 1;
        let gen = self.generation;
        for row in r0..=r1 {
            let base = row * self.cols;
            for col in c0..=c1 {
                for &id in &self.cells[base + col] {
                    // `id` was sized into `seen` on insert, so this never panics.
                    if self.seen[id] != gen {
                        self.seen[id] = gen;
                        out.push(id);
                    }
                }
            }
        }
    }

    /// Drop all inserted words, keeping the grid dimensions for reuse.
    pub fn clear(&mut self) {
        for cell in &mut self.cells {
            cell.clear();
        }
    }

    /// Inclusive `(col_min, col_max, row_min, row_max)` cell range a box covers,
    /// clamped to the grid. `None` if the box is empty (zero area).
    fn cell_span(&self, x: i32, y: i32, width: u32, height: u32) -> Option<(usize, usize, usize, usize)> {
        if width == 0 || height == 0 {
            return None;
        }
        // Last covered pixel is inclusive: [x, x + width - 1].
        let x_max = x + (width as i32 - 1);
        let y_max = y + (height as i32 - 1);
        let c0 = self.col_of(x);
        let c1 = self.col_of(x_max);
        let r0 = self.row_of(y);
        let r1 = self.row_of(y_max);
        Some((c0, c1, r0, r1))
    }

    /// Map an x pixel to a clamped column index.
    fn col_of(&self, x: i32) -> usize {
        if x <= 0 {
            0
        } else {
            ((x as u32 / self.cell_size) as usize).min(self.cols - 1)
        }
    }

    /// Map a y pixel to a clamped row index.
    fn row_of(&self, y: i32) -> usize {
        if y <= 0 {
            0
        } else {
            ((y as u32 / self.cell_size) as usize).min(self.rows - 1)
        }
    }
}

/// `ceil(a / b)` for `b >= 1` without overflow for canvas-sized values.
fn div_ceil(a: u32, b: u32) -> usize {
    a.div_ceil(b) as usize
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dimensions_round_up_and_stay_nonzero() {
        let g = SpatialGrid::new(100, 50, 32);
        assert_eq!(g.cols(), 4); // ceil(100/32)
        assert_eq!(g.rows(), 2); // ceil(50/32)

        // Degenerate inputs never produce a zero-cell grid.
        let g0 = SpatialGrid::new(0, 0, 0);
        assert_eq!(g0.cell_size(), 1);
        assert!(g0.cols() >= 1 && g0.rows() >= 1);
    }

    #[test]
    fn neighbor_query_finds_word_in_shared_cell() {
        let mut g = SpatialGrid::new(200, 200, 20);
        g.insert(7, 50, 50, 10, 10);
        // Query overlapping the same area returns the word.
        let near = g.neighbors(48, 48, 8, 8);
        assert_eq!(near, vec![7]);
    }

    #[test]
    fn fuzz_neighbors_is_superset_of_true_overlaps() {
        // The no-false-negatives guarantee, stress-tested: for many random box
        // sets, every pair that truly overlaps (AABB) must be reported by
        // `neighbors`. Boxes may straddle the canvas edges (negative / past-edge).
        fn overlap(a: (i32, i32, u32, u32), b: (i32, i32, u32, u32)) -> bool {
            a.0 < b.0 + b.2 as i32
                && a.0 + a.2 as i32 > b.0
                && a.1 < b.1 + b.3 as i32
                && a.1 + a.3 as i32 > b.1
        }
        // Cheap deterministic LCG so the test needs no rng dependency.
        let mut state: u64 = 0x9E3779B97F4A7C15;
        let mut rnd = |n: i64| {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            (((state >> 33) as i64) % n).unsigned_abs() as i64
        };

        for cell_size in [1u32, 7, 16, 50, 200] {
            let mut g = SpatialGrid::new(300, 200, cell_size);
            let mut boxes: Vec<(i32, i32, u32, u32)> = Vec::new();
            for id in 0..80usize {
                let x = rnd(360) as i32 - 30; // -30..330 (can straddle edges)
                let y = rnd(260) as i32 - 30;
                let w = 1 + rnd(40) as u32;
                let h = 1 + rnd(30) as u32;
                g.insert(id, x, y, w, h);
                boxes.push((x, y, w, h));
            }
            // Each box's neighbor query must include every other box it overlaps.
            for (i, &q) in boxes.iter().enumerate() {
                let near = g.neighbors(q.0, q.1, q.2, q.3);
                for (j, &b) in boxes.iter().enumerate() {
                    if i != j && overlap(q, b) {
                        assert!(
                            near.contains(&j),
                            "cell_size={cell_size}: box {i} {q:?} overlaps {j} {b:?} but neighbor query missed it (got {near:?})"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn neighbors_into_matches_neighbors_as_a_set() {
        // The allocation-free query must return the same *set* (order aside) as
        // the allocating one, across many random box sets, and dedup correctly.
        let mut state: u64 = 0x1234_5678_9ABC_DEF0;
        let mut rnd = |n: i64| {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            (((state >> 33) as i64) % n).unsigned_abs() as i64
        };
        for cell_size in [1u32, 7, 20, 64] {
            let mut g = SpatialGrid::new(300, 200, cell_size);
            let mut boxes = Vec::new();
            for id in 0..60usize {
                let (x, y) = (rnd(360) as i32 - 30, rnd(260) as i32 - 30);
                let (w, h) = (1 + rnd(50) as u32, 1 + rnd(40) as u32);
                g.insert(id, x, y, w, h);
                boxes.push((x, y, w, h));
            }
            // Reuse one buffer across queries, as the hot path does.
            let mut buf = Vec::new();
            for &(x, y, w, h) in &boxes {
                let want = g.neighbors(x, y, w, h); // sorted + deduped reference
                g.neighbors_into(x, y, w, h, &mut buf);
                // No duplicates.
                let mut got = buf.clone();
                got.sort_unstable();
                assert!(got.windows(2).all(|p| p[0] != p[1]), "neighbors_into produced duplicates");
                assert_eq!(got, want, "cell_size={cell_size}: set mismatch");
            }
        }
    }

    #[test]
    fn distant_word_is_excluded() {
        let mut g = SpatialGrid::new(400, 400, 20);
        g.insert(1, 0, 0, 10, 10); // top-left
        g.insert(2, 350, 350, 10, 10); // bottom-right
        // A query near the top-left sees word 1 only.
        let near = g.neighbors(5, 5, 5, 5);
        assert_eq!(near, vec![1]);
        assert!(!near.contains(&2));
    }

    #[test]
    fn overlapping_box_inserted_into_all_covered_cells() {
        // cell_size 10 → a 25px-wide box starting at x=5 spans columns 0,1,2,3.
        let mut g = SpatialGrid::new(100, 100, 10);
        g.insert(3, 5, 5, 25, 5);
        // Each of those columns (row 0) should report the word.
        for x in [0, 10, 20, 29] {
            assert!(g.neighbors(x, 0, 1, 5).contains(&3), "missing at x={x}");
        }
        // A column just past the box (x=30+) should not.
        assert!(!g.neighbors(40, 0, 1, 5).contains(&3));
    }

    #[test]
    fn neighbors_are_deduplicated() {
        // A big query box spanning many of the word's cells must still list it once.
        let mut g = SpatialGrid::new(200, 200, 10);
        g.insert(9, 0, 0, 50, 50); // spans a 5x5 block of cells
        let near = g.neighbors(0, 0, 60, 60);
        assert_eq!(near, vec![9]);
    }

    #[test]
    fn truly_overlapping_boxes_always_share_a_cell() {
        // No-false-negatives contract: two overlapping boxes are always neighbors,
        // regardless of how the grid lines fall between them.
        let mut g = SpatialGrid::new(300, 300, 16);
        g.insert(1, 100, 100, 40, 20);
        // A box that overlaps word 1's interior.
        let near = g.neighbors(120, 110, 30, 10);
        assert!(near.contains(&1));
    }

    #[test]
    fn off_canvas_overflow_clamps_to_border_cells() {
        let mut g = SpatialGrid::new(100, 100, 20);
        // Word straddling the left/top edge (negative origin).
        g.insert(4, -10, -10, 30, 30);
        // A query at the corner still finds it (clamped into cell (0,0)).
        assert!(g.neighbors(0, 0, 5, 5).contains(&4));
    }

    #[test]
    fn zero_area_box_covers_nothing() {
        let mut g = SpatialGrid::new(100, 100, 10);
        g.insert(1, 10, 10, 0, 10);
        assert!(g.neighbors(10, 10, 10, 10).is_empty());
        // And a zero-area query returns nothing.
        let mut g2 = SpatialGrid::new(100, 100, 10);
        g2.insert(1, 10, 10, 10, 10);
        assert!(g2.neighbors(10, 10, 0, 0).is_empty());
    }

    #[test]
    fn clear_empties_buckets_but_keeps_dims() {
        let mut g = SpatialGrid::new(100, 100, 10);
        g.insert(1, 10, 10, 10, 10);
        g.clear();
        assert!(g.neighbors(10, 10, 10, 10).is_empty());
        assert_eq!(g.cols(), 10);
        assert_eq!(g.rows(), 10);
    }

    #[test]
    fn recommended_cell_size_targets_two_by_two() {
        // Larger dimension halved; never zero.
        assert_eq!(SpatialGrid::recommended_cell_size(40, 16), 20);
        assert_eq!(SpatialGrid::recommended_cell_size(1, 1), 1);
        assert_eq!(SpatialGrid::recommended_cell_size(0, 0), 1);
    }
}
