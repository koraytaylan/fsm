//! Unsigned 256-bit integer used to widen decimal comparison and division.
//!
//! Restoring division invariant (why `wrapping_sub` is exact): before each
//! step `rem < d`, so the true shifted value `2·rem + bit < 2d`. When
//! `hi_bit == 1` the true value is ≥ 2¹²⁸ > `u128::MAX` ≥ `d`, so the
//! subtract branch is always correct there, and `true_value − d < d ≤
//! u128::MAX` always fits — the wrap-around arithmetic lands on exactly
//! that difference.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct U256 {
    hi: u128,
    lo: u128,
}

impl U256 {
    pub const ZERO: U256 = U256 { hi: 0, lo: 0 };

    pub fn from_u128(x: u128) -> Self {
        U256 { hi: 0, lo: x }
    }

    pub fn from_parts(hi: u128, lo: u128) -> Self {
        U256 { hi, lo }
    }

    pub fn hi(self) -> u128 {
        self.hi
    }

    pub fn lo(self) -> u128 {
        self.lo
    }

    pub fn cmp_parts(self, other: Self) -> core::cmp::Ordering {
        self.hi.cmp(&other.hi).then(self.lo.cmp(&other.lo))
    }

    #[allow(clippy::should_implement_trait)]
    pub fn cmp(self, other: Self) -> core::cmp::Ordering {
        self.cmp_parts(other)
    }

    fn bit(self, i: u32) -> u128 {
        if i < 128 {
            (self.lo >> i) & 1
        } else {
            (self.hi >> (i - 128)) & 1
        }
    }

    fn set_bit(&mut self, i: u32) {
        if i < 128 {
            self.lo |= 1u128 << i;
        } else {
            self.hi |= 1u128 << (i - 128);
        }
    }

    /// Multiply by 10^k using k single ×10 limb steps. `None` on overflow.
    pub fn checked_mul_pow10(self, k: u32) -> Option<U256> {
        let mut acc = self;
        for _ in 0..k {
            acc = acc.mul10()?;
        }
        Some(acc)
    }

    fn mul10(self) -> Option<U256> {
        let lo = self.lo;
        let hi = self.hi;
        let lo_lo = lo & 0xFFFF_FFFF_FFFF_FFFF;
        let lo_hi = lo >> 64;
        let p0 = lo_lo * 10;
        let p1 = lo_hi * 10 + (p0 >> 64);
        let new_lo = (p1 << 64) | (p0 & 0xFFFF_FFFF_FFFF_FFFF);
        let carry = p1 >> 64;
        let new_hi = hi.checked_mul(10)?.checked_add(carry)?;
        Some(U256 {
            hi: new_hi,
            lo: new_lo,
        })
    }

    /// Restoring division, one bit per iteration, high bit first.
    ///
    /// `d != 0` is caller-guaranteed.
    pub fn div_rem_u128(self, d: u128) -> (U256, u128) {
        debug_assert!(d != 0);
        let mut q = U256::ZERO;
        let mut rem: u128 = 0;
        for i in (0..256).rev() {
            let bit = self.bit(i);
            let hi_bit = rem >> 127;
            let mut r2 = (rem << 1) | bit;
            if hi_bit == 1 || r2 >= d {
                r2 = r2.wrapping_sub(d);
                q.set_bit(i);
            }
            rem = r2;
        }
        (q, rem)
    }
}

impl PartialOrd for U256 {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(Ord::cmp(self, other))
    }
}

impl Ord for U256 {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        self.cmp_parts(*other)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn xorshift(state: &mut u64) -> u64 {
        let mut x = *state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        *state = x;
        x
    }

    fn next_u128(state: &mut u64) -> u128 {
        let a = xorshift(state) as u128;
        let b = xorshift(state) as u128;
        (a << 64) | b
    }

    #[test]
    fn native_cross_check_sweep() {
        let mut state = 0xC0FFEE_u64;
        let mut cases = 0u32;
        while cases < 10_000 {
            let a = next_u128(&mut state);
            let b = next_u128(&mut state);
            let ua = U256::from_u128(a);
            let ub = U256::from_u128(b);
            assert_eq!(ua.cmp_parts(ub), a.cmp(&b));

            let k = (xorshift(&mut state) % 20) as u32;
            let rust = U256::from_u128(a).checked_mul_pow10(k);
            let mut native: Option<u128> = Some(a);
            for _ in 0..k {
                native = native.and_then(|n| n.checked_mul(10));
            }
            match (rust, native) {
                (Some(r), Some(n)) => {
                    assert_eq!(r.hi(), 0);
                    assert_eq!(r.lo(), n);
                }
                (None, None) => {}
                (Some(r), None) => {
                    assert!(r.hi() > 0 || r.lo() > 0);
                }
                (None, Some(_)) => panic!("u256 overflowed but u128 product exists"),
            }

            let d = (next_u128(&mut state) % (u128::MAX / 2)).saturating_add(1);
            let (q, r) = ua.div_rem_u128(d);
            assert_eq!(q.hi(), 0);
            assert_eq!(q.lo(), a / d);
            assert_eq!(r, a % d);
            assert!(r < d);
            cases += 1;
        }
    }

    #[test]
    fn crossing_mul_pow10() {
        let v = U256::from_u128(u128::MAX).checked_mul_pow10(1).unwrap();
        assert_eq!(v.hi(), 9);
        assert_eq!(v.lo(), u128::MAX - 9);
    }

    #[test]
    fn crossing_div_quotient_still_wide() {
        // (5 << 128) / 2 = (2 << 128) + 2^127
        let n = U256::from_parts(5, 0);
        let (q, r) = n.div_rem_u128(2);
        assert_eq!(r, 0);
        assert_eq!(q.hi(), 2);
        assert_eq!(q.lo(), 1u128 << 127);
    }

    #[test]
    fn crossing_div_quotient_fits_u128() {
        // 2^128 / 2 = 2^127
        let n = U256::from_parts(1, 0);
        let (q, r) = n.div_rem_u128(2);
        assert_eq!(r, 0);
        assert_eq!(q.hi(), 0);
        assert_eq!(q.lo(), 1u128 << 127);
    }

    #[test]
    fn division_edges() {
        let n = U256::from_parts(1, 7);
        let (q, r) = n.div_rem_u128(1);
        assert_eq!(q, n);
        assert_eq!(r, 0);

        let (q, r) = U256::from_u128(u128::MAX).div_rem_u128(u128::MAX);
        assert_eq!(q, U256::from_u128(1));
        assert_eq!(r, 0);

        let (q, r) = U256::ZERO.div_rem_u128(99);
        assert_eq!(q, U256::ZERO);
        assert_eq!(r, 0);
    }

    #[test]
    fn mul_pow10_overflow_boundary() {
        // (2^128 − 1) · 10^38 fits; 10^39 overflows.
        assert!(U256::from_u128(u128::MAX).checked_mul_pow10(38).is_some());
        assert!(U256::from_u128(u128::MAX).checked_mul_pow10(39).is_none());
    }

    #[test]
    fn worked_division_example() {
        // Worked: 2^128 ÷ 10 = 34028236692093846346337460743176821145 rem 6.
        let n = U256::from_parts(1, 0);
        let (q, r) = n.div_rem_u128(10);
        assert_eq!(r, 6);
        assert_eq!(q.hi(), 0);
        assert_eq!(q.lo(), 34028236692093846346337460743176821145);
        // Digit identity: 10·q + r = 2^128.
        let (back_q, back_r) = n.div_rem_u128(10);
        assert_eq!(back_q, q);
        assert_eq!(back_r, 6);
    }
}
