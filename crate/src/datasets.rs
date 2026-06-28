//! Synthetic dataset generators for benchmarks and tests (spec: *Performance
//! Targets*, Task 12).
//!
//! The same generators back the native `criterion` benchmarks and the in-browser
//! timing harness, so both measure the *same* inputs. Generation is fully
//! deterministic for a given `(n, dist, seed)` — no `rng` crate, just a small
//! linear-congruential generator — so benchmark numbers are reproducible.
//!
//! Two axes of variation matter for layout cost:
//!
//! * **Word count** `n` — the spec's size buckets are 50, 500, 5000, 50000.
//! * **Weight distribution** — controls how font sizes (and therefore mask
//!   areas) spread out. Real word clouds are heavily skewed (a few huge words,
//!   a long tail of small ones), which [`WeightDist::Zipf`] models; the other
//!   variants bracket that with even and linear spreads.

use crate::models::{Item, LayoutConfig};

/// How weights are spread across the generated items.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WeightDist {
    /// Every weight drawn uniformly at random from `[1, 100]`. Font sizes end up
    /// scattered with no particular skew.
    Uniform,
    /// Weights decrease linearly with rank (`n, n-1, …, 1`). A smooth ramp of
    /// font sizes from largest to smallest.
    Linear,
    /// Power-law weights (`weight ∝ 1 / rank^s`, `s ≈ 1.07`). Models real word
    /// frequency: a few dominant words and a long tail of tiny ones — the
    /// hardest, most realistic case for collision-heavy placement.
    Zipf,
}

impl WeightDist {
    /// Short stable identifier, handy for benchmark labels.
    pub fn label(self) -> &'static str {
        match self {
            WeightDist::Uniform => "uniform",
            WeightDist::Linear => "linear",
            WeightDist::Zipf => "zipf",
        }
    }
}

/// Minimal deterministic PRNG (LCG, the classic Numerical-Recipes constants).
/// Avoids a dev-dependency and guarantees identical sequences across machines.
struct Lcg(u64);

impl Lcg {
    fn new(seed: u64) -> Self {
        // Avoid a zero state.
        Lcg(seed ^ 0x9E37_79B9_7F4A_7C15)
    }

    #[inline]
    fn next_u64(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        // Use the high bits, which have the best statistical quality.
        self.0 >> 11
    }

    /// Uniform float in `[0, 1)`.
    #[inline]
    fn next_f32(&mut self) -> f32 {
        (self.next_u64() & 0xFF_FFFF) as f32 / (0x100_0000 as f32)
    }
}

/// Syllables combined into pronounceable, varied-length pseudo-words so masks
/// have realistic widths rather than all being the same size.
const SYLLABLES: &[&str] = &[
    "ba", " to", "ka", "mi", "re", "lo", "nu", "vi", "sa", "de", "pro", "tion",
    "ity", "graph", "log", "net", "cod", "wave", "form", "stack", "ray", "byte",
    "loop", "node", "edge", "core", "flux", "grid", "mesh", "pixel",
];

/// Build a single pseudo-word of 1–4 syllables.
fn make_word(rng: &mut Lcg) -> String {
    let syllables = 1 + (rng.next_u64() % 4) as usize; // 1..=4
    let mut s = String::with_capacity(syllables * 4);
    for _ in 0..syllables {
        let idx = (rng.next_u64() as usize) % SYLLABLES.len();
        s.push_str(SYLLABLES[idx]);
    }
    s
}

/// Generate `n` weighted items with the given weight distribution.
///
/// Deterministic: the same `(n, dist, seed)` always yields the same `Vec<Item>`.
/// Text is generated independently of `dist` (so weight distribution is the only
/// variable between runs that share `n`/`seed`).
pub fn generate(n: usize, dist: WeightDist, seed: u64) -> Vec<Item> {
    // Separate streams for text and weights so the words are identical across
    // distributions for a fixed seed (only the weights differ).
    let mut text_rng = Lcg::new(seed);
    let mut weight_rng = Lcg::new(seed ^ 0xABCD_EF01_2345_6789);

    (0..n)
        .map(|i| {
            let text = make_word(&mut text_rng);
            let weight = match dist {
                WeightDist::Uniform => 1.0 + weight_rng.next_f32() * 99.0,
                WeightDist::Linear => (n - i) as f32,
                // rank starts at 1; s controls skew.
                WeightDist::Zipf => {
                    let rank = (i + 1) as f32;
                    1000.0 / rank.powf(1.07)
                }
            };
            Item { text, weight }
        })
        .collect()
}

/// Canvas + font configuration sized for `n` words, the single source of truth
/// shared by the native benchmarks and (mirrored in `js/datasets.js`) the
/// in-browser harness.
///
/// Two goals: shrink fonts as the cloud grows so it can physically fit, and
/// target a modest fill ratio so (nearly) every word places — otherwise the
/// spiral wastes time walking to the canvas edge for unplaceable words, which
/// would measure a failure mode rather than real placement work. 4:3 aspect
/// mirrors a typical viewport.
pub fn config_for(n: usize) -> LayoutConfig {
    let (min_font, max_font) = if n <= 500 {
        (12.0, 72.0)
    } else if n <= 5000 {
        (8.0, 30.0)
    } else {
        (6.0, 18.0)
    };
    let avg_font = 0.5 * (min_font + max_font);
    // Crude per-word footprint: ~6 chars at ~0.6 width ratio, height ≈ font.
    let per_word = 3.6 * avg_font * avg_font;
    let target_fill = 0.28;
    let area = (n.max(1) as f32) * per_word / target_fill;
    let aspect = 4.0 / 3.0;
    let height = (area / aspect).sqrt();
    let width = height * aspect;
    LayoutConfig {
        width: width.round(),
        height: height.round(),
        min_font_size: min_font,
        max_font_size: max_font,
        padding: 2.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generates_requested_count() {
        assert_eq!(generate(0, WeightDist::Uniform, 1).len(), 0);
        assert_eq!(generate(50, WeightDist::Zipf, 1).len(), 50);
        assert_eq!(generate(500, WeightDist::Linear, 1).len(), 500);
    }

    #[test]
    fn deterministic_for_same_seed() {
        let a = generate(200, WeightDist::Zipf, 42);
        let b = generate(200, WeightDist::Zipf, 42);
        assert_eq!(a, b);
    }

    #[test]
    fn seed_changes_output() {
        let a = generate(100, WeightDist::Uniform, 1);
        let b = generate(100, WeightDist::Uniform, 2);
        assert_ne!(a, b);
    }

    #[test]
    fn text_is_identical_across_distributions_for_same_seed() {
        // Only weights should differ between distributions at a fixed seed.
        let uni = generate(50, WeightDist::Uniform, 7);
        let zipf = generate(50, WeightDist::Zipf, 7);
        let lin = generate(50, WeightDist::Linear, 7);
        for i in 0..50 {
            assert_eq!(uni[i].text, zipf[i].text);
            assert_eq!(uni[i].text, lin[i].text);
        }
    }

    #[test]
    fn weights_are_positive_and_words_non_empty() {
        for dist in [WeightDist::Uniform, WeightDist::Linear, WeightDist::Zipf] {
            for it in generate(300, dist, 3) {
                assert!(it.weight > 0.0, "{dist:?} produced a non-positive weight");
                assert!(!it.text.is_empty());
            }
        }
    }

    #[test]
    fn linear_is_ranked_descending() {
        let it = generate(100, WeightDist::Linear, 9);
        for w in it.windows(2) {
            assert!(w[0].weight >= w[1].weight);
        }
    }

    #[test]
    fn zipf_is_heavily_skewed() {
        // The top word should dominate the median by a wide margin.
        let it = generate(500, WeightDist::Zipf, 11);
        let top = it[0].weight;
        let median = it[250].weight;
        assert!(top > median * 50.0, "zipf not skewed enough: top={top}, median={median}");
    }
}
