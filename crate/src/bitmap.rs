//! The occupancy bitmap — the engine's primary collision structure.
//!
//! The canvas is represented as a 2D grid of pixels packed into `u64` words
//! (`Vec<u64>`), so a collision test ANDs 64 occupancy bits per machine
//! operation rather than checking pixels one at a time (spec: *Occupancy
//! Bitmap*). A non-zero AND result means the candidate word overlaps something
//! already placed.
//!
//! ## Placement math
//!
//! A [`WordMask`] is tested/written at an integer offset `(ox, oy)` on the
//! canvas. Mask rows map directly to canvas rows (`oy + r`). Along x, the mask
//! is shifted by `shift = ox & 63` so its word-aligned bits line up with the
//! canvas's word grid: each mask word contributes a low part to canvas word
//! `base + mw` and (when `shift > 0`) a high part to canvas word
//! `base + mw + 1`, where `base = ox >> 6`.
//!
//! Candidates that fall partly off-canvas are treated as collisions, so they
//! are never chosen for placement.
//!
//! ## SIMD
//!
//! The per-row work is split in two: a cheap scalar step *expands* the mask row
//! into a buffer aligned to the canvas word grid (applying the `shift`), then a
//! bulk bitwise kernel ([`crate::simd`]) runs the AND-scan / OR-write over
//! contiguous canvas memory. That kernel is SIMD-accelerated (`v128`, two `u64`
//! lanes per instruction) when `simd128` is enabled and scalar otherwise — the
//! result is identical either way.

use crate::mask::{words_per_row, WordMask, BITS};
use crate::simd;

/// Stack buffer capacity (in `u64`) for an expanded mask row; masks wider than
/// `SCRATCH_CAP * 64` pixels fall back to a heap buffer. Word masks are small,
/// so the stack path is taken in practice.
const SCRATCH_CAP: usize = 40;

/// Packed occupancy grid for the whole canvas.
#[derive(Debug, Clone)]
pub struct OccupancyBitmap {
    width: u32,
    height: u32,
    words_per_row: usize,
    bits: Vec<u64>,
}

impl OccupancyBitmap {
    /// Create an empty (all-free) occupancy bitmap of `width` × `height` pixels.
    pub fn new(width: u32, height: u32) -> Self {
        let wpr = words_per_row(width);
        OccupancyBitmap {
            width,
            height,
            words_per_row: wpr,
            bits: vec![0; wpr * height as usize],
        }
    }

    #[inline]
    pub fn width(&self) -> u32 {
        self.width
    }

    #[inline]
    pub fn height(&self) -> u32 {
        self.height
    }

    /// Read the occupancy bit at canvas pixel `(x, y)`; out-of-bounds → `false`.
    #[inline]
    pub fn get(&self, x: u32, y: u32) -> bool {
        if x >= self.width || y >= self.height {
            return false;
        }
        let idx = y as usize * self.words_per_row + (x >> 6) as usize;
        (self.bits[idx] >> (x & (BITS - 1))) & 1 == 1
    }

    /// Returns `true` if placing `mask` at `(ox, oy)` is off-canvas or overlaps
    /// any already-occupied pixel.
    ///
    /// Each row is expanded to canvas-word alignment, then [`simd::and_any`]
    /// scans it against the canvas. Returns early on the first overlapping row.
    pub fn collides(&self, mask: &WordMask, ox: i32, oy: i32) -> bool {
        if !self.fits(mask, ox, oy) {
            return true;
        }
        if mask.height() == 0 {
            return false;
        }
        let wpr = self.words_per_row;
        let base_word = (ox >> 6) as usize;
        let shift = (ox & (BITS as i32 - 1)) as u32;
        let max_span = wpr - base_word;

        let mut stack = [0u64; SCRATCH_CAP];
        let mut heap = Vec::new();
        let buf = scratch(&mut stack, &mut heap, mask.row(0).len() + 1);

        for r in 0..mask.height() {
            let span = expand_row(buf, mask.row(r), shift, max_span);
            let start = (oy + r as i32) as usize * wpr + base_word;
            if simd::and_any(&self.bits[start..start + span], &buf[..span]) {
                return true;
            }
        }
        false
    }

    /// Mark every set bit of `mask`, placed at `(ox, oy)`, as occupied.
    ///
    /// No-op for candidates that do not fully fit on-canvas; callers should
    /// only `occupy` positions that previously passed [`collides`].
    pub fn occupy(&mut self, mask: &WordMask, ox: i32, oy: i32) {
        if !self.fits(mask, ox, oy) || mask.height() == 0 {
            return;
        }
        let wpr = self.words_per_row;
        let base_word = (ox >> 6) as usize;
        let shift = (ox & (BITS as i32 - 1)) as u32;
        let max_span = wpr - base_word;

        let mut stack = [0u64; SCRATCH_CAP];
        let mut heap = Vec::new();
        let buf = scratch(&mut stack, &mut heap, mask.row(0).len() + 1);

        for r in 0..mask.height() {
            let span = expand_row(buf, mask.row(r), shift, max_span);
            let start = (oy + r as i32) as usize * wpr + base_word;
            simd::or_assign(&mut self.bits[start..start + span], &buf[..span]);
        }
    }

    /// True if `mask` placed at `(ox, oy)` lies fully within the canvas.
    #[inline]
    fn fits(&self, mask: &WordMask, ox: i32, oy: i32) -> bool {
        ox >= 0
            && oy >= 0
            && (ox as i64 + mask.width() as i64) <= self.width as i64
            && (oy as i64 + mask.height() as i64) <= self.height as i64
    }
}

/// Pick a working buffer of length `cap`: the stack array when it fits,
/// otherwise a freshly sized heap vector. `heap` owns any allocation so the
/// returned slice outlives the call.
#[inline]
fn scratch<'a>(stack: &'a mut [u64; SCRATCH_CAP], heap: &'a mut Vec<u64>, cap: usize) -> &'a mut [u64] {
    if cap <= SCRATCH_CAP {
        &mut stack[..cap]
    } else {
        heap.resize(cap, 0);
        &mut heap[..]
    }
}

/// Expand one mask row into `buf`, aligned to the canvas word grid, applying the
/// horizontal `shift`. Returns the number of canvas words the row spans
/// (`<= max_span`), i.e. the slice length to AND/OR against the canvas.
///
/// Mirrors the original per-word scatter exactly: each mask word contributes a
/// low part `m << shift` and (when `shift > 0`) a high part `m >> (64 - shift)`
/// to the next word — except where that next word would fall outside the canvas
/// row (`max_span`), whose bits are guaranteed zero because the placement fits.
#[inline]
fn expand_row(buf: &mut [u64], mask_row: &[u64], shift: u32, max_span: usize) -> usize {
    let mask_wpr = mask_row.len();
    let span = if shift > 0 && mask_wpr < max_span {
        mask_wpr + 1
    } else {
        mask_wpr.min(max_span)
    };
    for b in buf[..span].iter_mut() {
        *b = 0;
    }
    if shift == 0 {
        buf[..span].copy_from_slice(&mask_row[..span]);
    } else {
        for (i, &m) in mask_row.iter().enumerate() {
            buf[i] |= m << shift;
            if i + 1 < span {
                buf[i + 1] |= m >> (BITS - shift);
            }
        }
    }
    span
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block(width: u32, height: u32) -> WordMask {
        // A fully-filled rectangular mask.
        let pixels = (0..height).flat_map(|y| (0..width).map(move |x| (x, y)));
        WordMask::from_pixels(width, height, pixels)
    }

    #[test]
    fn disjoint_placements_do_not_collide() {
        let mut occ = OccupancyBitmap::new(200, 100);
        let m = block(10, 10);
        assert!(!occ.collides(&m, 0, 0));
        occ.occupy(&m, 0, 0);
        // Far away — no overlap.
        assert!(!occ.collides(&m, 50, 50));
    }

    #[test]
    fn overlapping_placement_collides() {
        let mut occ = OccupancyBitmap::new(200, 100);
        let m = block(10, 10);
        occ.occupy(&m, 20, 20);
        assert!(occ.collides(&m, 25, 25)); // overlaps the 20..30 block
        assert!(occ.collides(&m, 20, 20)); // exact overlap
    }

    #[test]
    fn adjacent_but_touching_does_not_collide() {
        let mut occ = OccupancyBitmap::new(200, 100);
        let m = block(10, 10);
        occ.occupy(&m, 0, 0); // occupies columns 0..9, rows 0..9
        // Placed immediately to the right (cols 10..19) — no shared pixel.
        assert!(!occ.collides(&m, 10, 0));
        // Placed immediately below (rows 10..19) — no shared pixel.
        assert!(!occ.collides(&m, 0, 10));
    }

    #[test]
    fn off_canvas_is_a_collision() {
        let occ = OccupancyBitmap::new(50, 50);
        let m = block(10, 10);
        assert!(occ.collides(&m, -1, 0));
        assert!(occ.collides(&m, 0, -1));
        assert!(occ.collides(&m, 45, 0)); // 45 + 10 > 50
        assert!(occ.collides(&m, 0, 45));
        assert!(!occ.collides(&m, 40, 40)); // 40 + 10 == 50, just fits
    }

    #[test]
    fn occupy_then_get_marks_all_pixels() {
        let mut occ = OccupancyBitmap::new(100, 20);
        let m = block(5, 5);
        occ.occupy(&m, 3, 4);
        for y in 0..20 {
            for x in 0..100 {
                let expect = (3..8).contains(&x) && (4..9).contains(&y);
                assert_eq!(occ.get(x, y), expect, "pixel ({x},{y})");
            }
        }
    }

    #[test]
    fn collision_works_across_word_boundary_with_shift() {
        // Place a wide block straddling the 64-bit word boundary, then test
        // overlaps near the seam to exercise the low/high shifted parts.
        let mut occ = OccupancyBitmap::new(256, 6);
        let m = block(20, 2);
        occ.occupy(&m, 60, 1); // rows 1..2, columns 60..79 cross the 64-bit boundary
        assert!(occ.collides(&m, 70, 1)); // overlaps 70..79
        assert!(occ.collides(&m, 55, 0)); // overlaps rows 1, cols 60..74
        assert!(!occ.collides(&m, 80, 1)); // starts right after, cols 80..99
        assert!(!occ.collides(&m, 60, 3)); // rows 3..4, no overlap with rows 1..2
    }

    /// Brute-force per-pixel reference: does `mask` at `(ox, oy)` overlap any set
    /// pixel, or fall off-canvas?
    fn brute_collides(occ: &OccupancyBitmap, mask: &WordMask, ox: i32, oy: i32) -> bool {
        if ox < 0
            || oy < 0
            || ox as i64 + mask.width() as i64 > occ.width() as i64
            || oy as i64 + mask.height() as i64 > occ.height() as i64
        {
            return true;
        }
        for my in 0..mask.height() {
            for mx in 0..mask.width() {
                if mask.get(mx, my) && occ.get((ox + mx as i32) as u32, (oy + my as i32) as u32) {
                    return true;
                }
            }
        }
        false
    }

    #[test]
    fn fuzz_collide_and_occupy_match_pixel_reference() {
        // Random masks placed at random shifts (across word boundaries) must agree
        // with a brute-force pixel scan — exercising the expand-row + kernel path
        // (whichever kernel is compiled) against ground truth.
        let mut state: u64 = 0x1234_5678_9ABC_DEF0;
        let mut next = |n: u32| {
            state = state
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            ((state >> 33) as u32) % n
        };

        let mut occ = OccupancyBitmap::new(200, 80);
        for _ in 0..400 {
            // Random small mask with random set pixels.
            let w = 1 + next(90);
            let h = 1 + next(20);
            let mut pix = Vec::new();
            for y in 0..h {
                for x in 0..w {
                    if next(2) == 0 {
                        pix.push((x, y));
                    }
                }
            }
            let mask = WordMask::from_pixels(w, h, pix);
            let ox = next(210) as i32 - 5; // can go off either edge
            let oy = next(90) as i32 - 5;

            assert_eq!(
                occ.collides(&mask, ox, oy),
                brute_collides(&occ, &mask, ox, oy),
                "collide mismatch at ({ox},{oy}) size {w}x{h}"
            );

            // Occupy only valid, non-colliding spots, then confirm those pixels read back.
            if !occ.collides(&mask, ox, oy) {
                occ.occupy(&mask, ox, oy);
                for my in 0..h {
                    for mx in 0..w {
                        if mask.get(mx, my) {
                            assert!(
                                occ.get((ox + mx as i32) as u32, (oy + my as i32) as u32),
                                "occupy left pixel unset at ({},{})",
                                ox + mx as i32,
                                oy + my as i32
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn single_pixel_overlap_detected() {
        let mut occ = OccupancyBitmap::new(128, 128);
        let dot = WordMask::from_pixels(1, 1, [(0, 0)]);
        occ.occupy(&dot, 100, 100);
        assert!(occ.collides(&dot, 100, 100));
        assert!(!occ.collides(&dot, 100, 101));
        assert!(!occ.collides(&dot, 99, 100));
    }
}
