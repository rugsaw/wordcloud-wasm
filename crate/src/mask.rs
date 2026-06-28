//! Bit-packed word masks.
//!
//! A [`WordMask`] is a small binary bitmap describing a single word's footprint
//! (`1` = occupied pixel, `0` = empty), packed into `u64` words for fast
//! intersection against the [`OccupancyBitmap`](crate::bitmap::OccupancyBitmap).
//!
//! This module owns the **storage layer** only: dimensions, bit layout, and
//! per-pixel get/set. Rasterizing actual glyph shapes from `(text, font_size,
//! rotation)` is layered on top of this struct in Task 04.
//!
//! ## Bit layout
//!
//! Row-major. Each row holds `words_per_row = ceil(width / 64)` `u64` words.
//! Pixel `(x, y)` lives in `bits[y * words_per_row + (x >> 6)]` at bit
//! `x & 63`. Bits in the final word of a row beyond `width` are always kept
//! zero so masks can be OR/AND-ed without bleeding past their declared width.

/// Number of bits in a packed word.
pub(crate) const BITS: u32 = 64;

/// Number of `u64` words needed to hold `width` bits.
#[inline]
pub(crate) fn words_per_row(width: u32) -> usize {
    width.div_ceil(BITS) as usize
}

/// A bit-packed binary mask for a single word.
#[derive(Debug, Clone, PartialEq)]
pub struct WordMask {
    width: u32,
    height: u32,
    words_per_row: usize,
    bits: Vec<u64>,
}

impl WordMask {
    /// Create an all-empty mask of the given pixel dimensions.
    pub fn new(width: u32, height: u32) -> Self {
        let wpr = words_per_row(width);
        WordMask {
            width,
            height,
            words_per_row: wpr,
            bits: vec![0; wpr * height as usize],
        }
    }

    /// Build a mask from an iterator of occupied `(x, y)` pixel coordinates.
    ///
    /// Coordinates outside `width`/`height` are ignored. Primarily a test and
    /// construction helper for higher layers.
    pub fn from_pixels<I>(width: u32, height: u32, pixels: I) -> Self
    where
        I: IntoIterator<Item = (u32, u32)>,
    {
        let mut m = WordMask::new(width, height);
        for (x, y) in pixels {
            m.set(x, y, true);
        }
        m
    }

    #[inline]
    pub fn width(&self) -> u32 {
        self.width
    }

    #[inline]
    pub fn height(&self) -> u32 {
        self.height
    }

    /// Borrow the packed bits of mask row `y`.
    #[inline]
    pub(crate) fn row(&self, y: u32) -> &[u64] {
        let start = y as usize * self.words_per_row;
        &self.bits[start..start + self.words_per_row]
    }

    /// Set or clear the pixel at `(x, y)`. Out-of-bounds coordinates are
    /// ignored.
    #[inline]
    pub fn set(&mut self, x: u32, y: u32, value: bool) {
        if x >= self.width || y >= self.height {
            return;
        }
        let idx = y as usize * self.words_per_row + (x >> 6) as usize;
        let bit = 1u64 << (x & (BITS - 1));
        if value {
            self.bits[idx] |= bit;
        } else {
            self.bits[idx] &= !bit;
        }
    }

    /// Read the pixel at `(x, y)`. Out-of-bounds reads return `false`.
    #[inline]
    pub fn get(&self, x: u32, y: u32) -> bool {
        if x >= self.width || y >= self.height {
            return false;
        }
        let idx = y as usize * self.words_per_row + (x >> 6) as usize;
        (self.bits[idx] >> (x & (BITS - 1))) & 1 == 1
    }

    /// Total number of occupied pixels.
    pub fn count_ones(&self) -> u32 {
        self.bits.iter().map(|w| w.count_ones()).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn words_per_row_rounds_up() {
        assert_eq!(words_per_row(1), 1);
        assert_eq!(words_per_row(64), 1);
        assert_eq!(words_per_row(65), 2);
        assert_eq!(words_per_row(128), 2);
    }

    #[test]
    fn set_and_get_roundtrip_across_word_boundary() {
        let mut m = WordMask::new(130, 3);
        m.set(0, 0, true);
        m.set(63, 0, true); // last bit of first word
        m.set(64, 1, true); // first bit of second word
        m.set(129, 2, true); // last column
        assert!(m.get(0, 0));
        assert!(m.get(63, 0));
        assert!(m.get(64, 1));
        assert!(m.get(129, 2));
        assert!(!m.get(1, 0));
        assert_eq!(m.count_ones(), 4);
    }

    #[test]
    fn out_of_bounds_is_ignored() {
        let mut m = WordMask::new(10, 10);
        m.set(100, 100, true); // ignored
        assert!(!m.get(100, 100));
        assert_eq!(m.count_ones(), 0);
    }

    #[test]
    fn from_pixels_builds_expected_mask() {
        let m = WordMask::from_pixels(8, 2, [(0, 0), (7, 1)]);
        assert!(m.get(0, 0));
        assert!(m.get(7, 1));
        assert_eq!(m.count_ones(), 2);
    }
}
