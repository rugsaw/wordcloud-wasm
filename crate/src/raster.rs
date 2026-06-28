//! Word rasterization — turning `(text, font_size, rotation)` into a
//! [`WordMask`](crate::mask::WordMask) collision footprint.
//!
//! ## Approach (and why)
//!
//! The MVP uses a **built-in proportional metrics table** rather than a bundled
//! font rasterizer (`ab_glyph` / `fontdue`). Each character contributes a filled
//! cell whose width comes from a per-character advance-ratio table (so `m`/`w`
//! are wide, `i`/`l` are narrow — not monospace) and whose height is the full em
//! box. The resulting mask is the union of those cells: a close approximation of
//! the word's *bounding footprint*, which is all the collision test needs.
//!
//! Tradeoffs of this choice:
//!
//! * **Pros** — zero dependencies, no font asset to vendor, builds cleanly for
//!   `wasm32`, fully deterministic and unit-testable offline, tiny binary.
//! * **Cons** — cells are filled boxes, so the mask does not capture per-glyph
//!   contours; words cannot nest into the gaps of tall/short glyphs.
//!
//! ### Upgrade path
//!
//! The browser ultimately renders text with *its own* font, so glyph-accurate
//! footprints really require *that* font's bytes. The [`Rasterizer`] trait below
//! is the seam for that: a future `GlyphRasterizer::from_font_bytes(&[u8])` (e.g.
//! backed by `ab_glyph`) can be supplied the font `ArrayBuffer` from JS in the
//! WASM API (Task 06) and produce contour-accurate masks without changing any
//! caller. [`MetricsRasterizer`] is the default that ships today.

use crate::mask::WordMask;

/// Rotations the layout engine emits. Stored compactly elsewhere as `u8`
/// degrees (0 or 90); this enum is the rasterizer-facing form.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rotation {
    /// Horizontal text.
    None,
    /// Quarter turn; swaps the mask's width and height.
    Ninety,
}

impl Rotation {
    /// Map a degrees value (as carried on `WordPlacement::rotation`) to a
    /// [`Rotation`]. Anything other than `90` is treated as upright.
    pub fn from_degrees(deg: u8) -> Self {
        match deg {
            90 => Rotation::Ninety,
            _ => Rotation::None,
        }
    }
}

/// Produces collision masks for words. The seam that lets a glyph-accurate
/// rasterizer replace the built-in metrics one without touching callers.
pub trait Rasterizer {
    /// Unrotated pixel bounding box `(width, height)` of `text` at `font_size`,
    /// excluding padding.
    fn measure(&self, text: &str, font_size: f32) -> (u32, u32);

    /// Rasterize `text` into a [`WordMask`], applying `rotation` and expanding
    /// every side by `padding` pixels.
    fn rasterize(&self, text: &str, font_size: f32, rotation: Rotation, padding: u32) -> WordMask;
}

/// Vertical em metrics, as fractions of the font size.
const ASCENT: f32 = 0.80;
const DESCENT: f32 = 0.20;

/// Advance width of a character, as a fraction of the em (font size).
///
/// A coarse proportional model: narrow glyphs, wide glyphs, and sensible
/// defaults for lowercase / uppercase / digits. Good enough to size a word's
/// footprint without shipping real font metrics.
fn advance_ratio(c: char) -> f32 {
    match c {
        ' ' => 0.30,
        'i' | 'j' | 'l' | 'I' | '.' | ',' | '\'' | '!' | '|' | ':' | ';' | '`' => 0.30,
        'f' | 'r' | 't' | '(' | ')' | '[' | ']' | '{' | '}' | '/' | '\\' => 0.40,
        'm' | 'w' | 'M' | 'W' | '@' => 0.95,
        'A'..='Z' | '0'..='9' => 0.68,
        _ => 0.52,
    }
}

/// The default, zero-dependency rasterizer described in the module docs.
#[derive(Debug, Clone, Copy, Default)]
pub struct MetricsRasterizer;

impl MetricsRasterizer {
    pub fn new() -> Self {
        MetricsRasterizer
    }

    /// Per-character cell widths (in pixels) for `text` at `font_size`.
    fn cell_widths(&self, text: &str, font_size: f32) -> Vec<u32> {
        text.chars()
            .map(|c| (advance_ratio(c) * font_size).round().max(1.0) as u32)
            .collect()
    }
}

impl Rasterizer for MetricsRasterizer {
    fn measure(&self, text: &str, font_size: f32) -> (u32, u32) {
        let width: u32 = self.cell_widths(text, font_size).iter().sum();
        let height = ((ASCENT + DESCENT) * font_size).round().max(1.0) as u32;
        (width.max(1), height)
    }

    fn rasterize(&self, text: &str, font_size: f32, rotation: Rotation, padding: u32) -> WordMask {
        let (w, h) = self.measure(text, font_size);
        let widths = self.cell_widths(text, font_size);

        // Collect filled pixels in unrotated space, skipping whitespace cells so
        // inter-word gaps stay empty.
        let mut filled: Vec<(u32, u32)> = Vec::new();
        let mut x_cursor = 0u32;
        for (c, cw) in text.chars().zip(widths.iter().copied()) {
            if !c.is_whitespace() {
                for x in x_cursor..x_cursor + cw {
                    for y in 0..h {
                        filled.push((x, y));
                    }
                }
            }
            x_cursor += cw;
        }

        // Apply rotation: 90° maps (x, y) -> (y, w - 1 - x) into an h×w grid.
        let (out_w, out_h) = match rotation {
            Rotation::None => (w, h),
            Rotation::Ninety => (h, w),
        };
        let place: Box<dyn Fn(u32, u32) -> (u32, u32)> = match rotation {
            Rotation::None => Box::new(|x, y| (x, y)),
            Rotation::Ninety => Box::new(move |x, y| (y, w - 1 - x)),
        };

        let pad = padding;
        let mask_w = out_w + 2 * pad;
        let mask_h = out_h + 2 * pad;
        let pixels = filled.into_iter().map(|(x, y)| {
            let (rx, ry) = place(x, y);
            (rx + pad, ry + pad)
        });
        WordMask::from_pixels(mask_w, mask_h, pixels)
    }
}

/// Convenience wrapper using the default [`MetricsRasterizer`].
pub fn rasterize_word(text: &str, font_size: f32, rotation: u8, padding: u32) -> WordMask {
    MetricsRasterizer::new().rasterize(text, font_size, Rotation::from_degrees(rotation), padding)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn produces_non_empty_mask_for_text() {
        let m = rasterize_word("Rust", 32.0, 0, 0);
        assert!(m.count_ones() > 0);
        assert!(m.width() > 0 && m.height() > 0);
    }

    #[test]
    fn bbox_scales_with_font_size() {
        let small = MetricsRasterizer::new().measure("WebAssembly", 16.0);
        let large = MetricsRasterizer::new().measure("WebAssembly", 48.0);
        assert!(large.0 > small.0, "width should grow with font size");
        assert!(large.1 > small.1, "height should grow with font size");
        // Roughly proportional (3x font ⇒ ~3x dimensions).
        assert!(large.1 >= small.1 * 2);
    }

    #[test]
    fn proportional_widths_are_not_monospace() {
        let r = MetricsRasterizer::new();
        let narrow = r.measure("iiii", 40.0).0;
        let wide = r.measure("mmmm", 40.0).0;
        assert!(wide > narrow, "wide glyphs must measure wider than narrow ones");
    }

    #[test]
    fn rotation_90_swaps_width_and_height() {
        let r = MetricsRasterizer::new();
        let up = r.rasterize("hello", 32.0, Rotation::None, 0);
        let side = r.rasterize("hello", 32.0, Rotation::Ninety, 0);
        assert_eq!(up.width(), side.height());
        assert_eq!(up.height(), side.width());
        // Rotation preserves the number of set pixels.
        assert_eq!(up.count_ones(), side.count_ones());
    }

    #[test]
    fn padding_expands_every_side() {
        let r = MetricsRasterizer::new();
        let (w, h) = r.measure("ab", 24.0);
        let pad = 3;
        let m = r.rasterize("ab", 24.0, Rotation::None, pad);
        assert_eq!(m.width(), w + 2 * pad);
        assert_eq!(m.height(), h + 2 * pad);
        // The padding border itself is empty.
        assert!(!m.get(0, 0));
    }

    #[test]
    fn whitespace_leaves_a_gap() {
        // "a a" must have an empty column band between the two letters.
        let r = MetricsRasterizer::new();
        let m = r.rasterize("a a", 40.0, Rotation::None, 0);
        let any_empty_column = (0..m.width()).any(|x| (0..m.height()).all(|y| !m.get(x, y)));
        assert!(any_empty_column, "the space should yield at least one empty column");
    }

    #[test]
    fn mask_is_and_compatible_with_occupancy_bitmap() {
        use crate::bitmap::OccupancyBitmap;
        let mask = rasterize_word("hi", 24.0, 0, 1);
        let mut occ = OccupancyBitmap::new(256, 128);
        assert!(!occ.collides(&mask, 10, 10));
        occ.occupy(&mask, 10, 10);
        assert!(occ.collides(&mask, 10, 10));
    }
}
