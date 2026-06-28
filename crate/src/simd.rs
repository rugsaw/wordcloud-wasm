//! Bitwise kernels for the collision hot path, with a WebAssembly SIMD
//! (`v128`) implementation and a portable scalar fallback (spec: *SIMD
//! Acceleration*).
//!
//! Two operations dominate [`OccupancyBitmap`](crate::bitmap::OccupancyBitmap):
//!
//! * **`and_any`** — the collision AND-scan: is any `a[i] & b[i]` non-zero?
//! * **`or_assign`** — the occupancy OR-write: `dst[i] |= src[i]`.
//!
//! Both run over contiguous `u64` slices. The SIMD path processes two `u64`
//! lanes per `v128` instruction; the scalar path one at a time. They are
//! behaviourally identical — same inputs, same result — so the only observable
//! difference is speed.
//!
//! ## Dispatch
//!
//! [`and_any`] / [`or_assign`] compile to the SIMD kernel when the target has
//! `simd128` enabled (`-C target-feature=+simd128` on `wasm32`), and to the
//! scalar kernel otherwise — so a non-SIMD target (or a native build) still
//! works, just without the vector speedup. The scalar variants
//! ([`and_any_scalar`] / [`or_assign_scalar`]) are always available so tests can
//! assert the two paths agree.

/// Returns `true` if `a[i] & b[i] != 0` for any `i` in `0..min(a.len, b.len)`.
///
/// Scalar reference implementation; always available.
#[inline]
pub fn and_any_scalar(a: &[u64], b: &[u64]) -> bool {
    a.iter().zip(b).any(|(&x, &y)| x & y != 0)
}

/// `dst[i] |= src[i]` for `i` in `0..min(dst.len, src.len)`.
///
/// Scalar reference implementation; always available.
#[inline]
pub fn or_assign_scalar(dst: &mut [u64], src: &[u64]) {
    for (d, &s) in dst.iter_mut().zip(src) {
        *d |= s;
    }
}

#[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
mod imp {
    use core::arch::wasm32::*;

    /// SIMD collision AND-scan: two `u64` lanes per `v128`, early-out per chunk.
    #[inline]
    pub fn and_any(a: &[u64], b: &[u64]) -> bool {
        let n = a.len().min(b.len());
        let mut i = 0;
        while i + 2 <= n {
            // SAFETY: `i + 2 <= n` guarantees 16 readable bytes at each pointer.
            // `v128.load` permits unaligned access on wasm.
            let va = unsafe { v128_load(a.as_ptr().add(i) as *const v128) };
            let vb = unsafe { v128_load(b.as_ptr().add(i) as *const v128) };
            if v128_any_true(v128_and(va, vb)) {
                return true;
            }
            i += 2;
        }
        // Tail (odd final word).
        while i < n {
            if a[i] & b[i] != 0 {
                return true;
            }
            i += 1;
        }
        false
    }

    /// SIMD occupancy OR-write: two `u64` lanes per `v128`.
    #[inline]
    pub fn or_assign(dst: &mut [u64], src: &[u64]) {
        let n = dst.len().min(src.len());
        let mut i = 0;
        while i + 2 <= n {
            // SAFETY: `i + 2 <= n` guarantees 16 readable/writable bytes.
            unsafe {
                let vd = v128_load(dst.as_ptr().add(i) as *const v128);
                let vs = v128_load(src.as_ptr().add(i) as *const v128);
                v128_store(dst.as_mut_ptr().add(i) as *mut v128, v128_or(vd, vs));
            }
            i += 2;
        }
        while i < n {
            dst[i] |= src[i];
            i += 1;
        }
    }
}

#[cfg(not(all(target_arch = "wasm32", target_feature = "simd128")))]
mod imp {
    pub use super::{and_any_scalar as and_any, or_assign_scalar as or_assign};
}

/// Collision AND-scan. SIMD when `simd128` is enabled, else scalar.
#[inline]
pub fn and_any(a: &[u64], b: &[u64]) -> bool {
    imp::and_any(a, b)
}

/// Occupancy OR-write. SIMD when `simd128` is enabled, else scalar.
#[inline]
pub fn or_assign(dst: &mut [u64], src: &[u64]) {
    imp::or_assign(dst, src)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn and_any_scalar_matches_definition() {
        assert!(!and_any_scalar(&[0b0011, 0b1100], &[0b1100, 0b0011]));
        assert!(and_any_scalar(&[0b0011, 0b1100], &[0b0001, 0])); // first word overlaps
        assert!(and_any_scalar(&[0, 0b0100], &[0, 0b0100])); // second word overlaps
        assert!(!and_any_scalar(&[], &[]));
    }

    #[test]
    fn or_assign_scalar_sets_union() {
        let mut dst = [0b0001u64, 0b0010];
        or_assign_scalar(&mut dst, &[0b0100, 0b1000]);
        assert_eq!(dst, [0b0101, 0b1010]);
    }

    #[test]
    fn and_any_uses_shorter_length() {
        // Extra words in the longer slice are ignored.
        assert!(!and_any(&[0, 0, 0xFFFF], &[0, 0]));
        assert!(and_any(&[0xFF], &[0x01, 0xFF]));
    }

    #[test]
    fn or_assign_uses_shorter_length() {
        let mut dst = [1u64, 2, 4];
        or_assign(&mut dst, &[8, 8]); // only first two updated
        assert_eq!(dst, [9, 10, 4]);
    }

    /// The active dispatch must agree with the scalar reference across sizes
    /// that exercise both the vector body and the scalar tail. On a SIMD target
    /// this compares the SIMD kernel to scalar; otherwise scalar to itself.
    #[test]
    fn dispatch_agrees_with_scalar_reference() {
        // Deterministic pseudo-random data, no rng dependency.
        let mut state: u64 = 0xDEAD_BEEF_1234_5678;
        let mut next = || {
            state = state
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            state >> 11
        };
        for len in 0..40usize {
            let a: Vec<u64> = (0..len).map(|_| next() & 0x0F0F_0F0F).collect();
            let b: Vec<u64> = (0..len).map(|_| next() & 0xF0F0_F0F0).collect();
            assert_eq!(and_any(&a, &b), and_any_scalar(&a, &b), "and_any len={len}");

            let mut d1 = a.clone();
            let mut d2 = a.clone();
            or_assign(&mut d1, &b);
            or_assign_scalar(&mut d2, &b);
            assert_eq!(d1, d2, "or_assign len={len}");
        }
    }
}
