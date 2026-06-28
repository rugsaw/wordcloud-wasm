// Synthetic dataset generators for the in-browser benchmark harness.
//
// This is a faithful JS port of `crate/src/datasets.rs`: the same LCG, the same
// syllable table, the same weight formulas. For a given (n, dist, seed) it
// produces byte-for-byte identical items to the Rust generator, so the browser
// harness and the native `cargo bench` measure the *same* inputs.
//
// u64 wrapping arithmetic is reproduced with BigInt masked to 64 bits.

const U64 = (1n << 64n) - 1n;
const MUL = 6364136223846793005n;
const INC = 1442695040888963407n;
const SEED_MIX = 0x9e3779b97f4a7c15n;
const WEIGHT_MIX = 0xabcdef0123456789n;

/** Weight distributions, matching Rust's `WeightDist`. */
export const WeightDist = Object.freeze({
  Uniform: "uniform",
  Linear: "linear",
  Zipf: "zipf",
});

// Must match SYLLABLES in datasets.rs exactly (including the leading space in " to").
const SYLLABLES = [
  "ba", " to", "ka", "mi", "re", "lo", "nu", "vi", "sa", "de", "pro", "tion",
  "ity", "graph", "log", "net", "cod", "wave", "form", "stack", "ray", "byte",
  "loop", "node", "edge", "core", "flux", "grid", "mesh", "pixel",
];

class Lcg {
  constructor(seed) {
    this.state = (BigInt(seed) ^ SEED_MIX) & U64;
  }
  // Returns the high-bits u64 (>> 11) as a BigInt.
  nextU64() {
    this.state = (this.state * MUL + INC) & U64;
    return this.state >> 11n;
  }
  // Uniform float in [0, 1).
  nextF32() {
    const bits = Number(this.nextU64() & 0xffffffn);
    return bits / 0x1000000;
  }
}

function makeWord(rng) {
  const syllables = 1 + Number(rng.nextU64() % 4n); // 1..=4
  let s = "";
  for (let k = 0; k < syllables; k++) {
    const idx = Number(rng.nextU64() % BigInt(SYLLABLES.length));
    s += SYLLABLES[idx];
  }
  return s;
}

/**
 * Generate `n` weighted items. Deterministic for a given (n, dist, seed) and
 * identical to the Rust `datasets::generate`.
 *
 * @param {number} n
 * @param {"uniform"|"linear"|"zipf"} dist
 * @param {number|bigint} [seed=1]
 * @returns {Array<{text: string, weight: number}>}
 */
export function generate(n, dist, seed = 1) {
  const textRng = new Lcg(seed);
  const weightRng = new Lcg((BigInt(seed) ^ WEIGHT_MIX) & U64);
  const items = new Array(n);
  for (let i = 0; i < n; i++) {
    const text = makeWord(textRng);
    let weight;
    const f32 = Math.fround; // mirror Rust's f32 arithmetic
    switch (dist) {
      case WeightDist.Uniform:
        weight = f32(1.0 + f32(weightRng.nextF32() * 99.0));
        break;
      case WeightDist.Linear:
        weight = n - i;
        break;
      case WeightDist.Zipf:
        weight = f32(1000.0 / f32(Math.pow(i + 1, 1.07)));
        break;
      default:
        throw new RangeError(`unknown distribution: ${dist}`);
    }
    items[i] = { text, weight };
  }
  return items;
}

/**
 * Canvas + font config for `n` words, mirroring `config_for` in
 * `crate/benches/layout.rs` so browser timings line up with the native ones.
 *
 * @param {number} n
 * @returns {{width:number,height:number,minFontSize:number,maxFontSize:number,padding:number}}
 */
export function configFor(n) {
  let minFont;
  let maxFont;
  if (n <= 500) {
    [minFont, maxFont] = [12, 72];
  } else if (n <= 5000) {
    [minFont, maxFont] = [8, 30];
  } else {
    [minFont, maxFont] = [6, 18];
  }
  const avgFont = 0.5 * (minFont + maxFont);
  const perWord = 3.6 * avgFont * avgFont;
  const targetFill = 0.28;
  const area = (Math.max(1, n) * perWord) / targetFill;
  const aspect = 4 / 3;
  const height = Math.sqrt(area / aspect);
  const width = height * aspect;
  return {
    width: Math.round(width),
    height: Math.round(height),
    minFontSize: minFont,
    maxFontSize: maxFont,
    padding: 2,
  };
}
