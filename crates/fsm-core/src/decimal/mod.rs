//! Fixed-point decimal on an i128 mantissa with explicit (never-normalized) scale.

pub mod u256;

use core::cmp::Ordering;

use self::u256::U256;

pub const MAX_SCALE: u8 = 12;

const fn pow10_i128(k: u32) -> i128 {
    let mut r = 1i128;
    let mut i = 0;
    while i < k {
        r *= 10;
        i += 1;
    }
    r
}

pub const MAX_MANT: i128 = pow10_i128(38) - 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Dec {
    pub mant: i128,
    pub scale: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecError {
    Parse,
    Overflow,
    ScaleCap,
    DivZero,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RoundMode {
    Down,
    Up,
    Floor,
    Ceiling,
    HalfUp,
    HalfDown,
    HalfEven,
}

impl RoundMode {
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "down" => Some(Self::Down),
            "up" => Some(Self::Up),
            "floor" => Some(Self::Floor),
            "ceiling" => Some(Self::Ceiling),
            "half_up" => Some(Self::HalfUp),
            "half_down" => Some(Self::HalfDown),
            "half_even" => Some(Self::HalfEven),
            _ => None,
        }
    }
}

/// Shared rounding decision. `r == 0` never reaches `bump`.
pub(crate) fn bump(
    mode: RoundMode,
    negative: bool,
    twice_rem_vs_divisor: Ordering,
    q_is_even: bool,
) -> bool {
    match mode {
        RoundMode::Down => false,
        RoundMode::Up => true,
        RoundMode::Floor => negative,
        RoundMode::Ceiling => !negative,
        RoundMode::HalfUp => matches!(twice_rem_vs_divisor, Ordering::Greater | Ordering::Equal),
        RoundMode::HalfDown => twice_rem_vs_divisor == Ordering::Greater,
        RoundMode::HalfEven => {
            twice_rem_vs_divisor == Ordering::Greater
                || (twice_rem_vs_divisor == Ordering::Equal && !q_is_even)
        }
    }
}

impl Dec {
    pub fn new(mant: i128, scale: u8) -> Result<Self, DecError> {
        if scale > MAX_SCALE {
            return Err(DecError::ScaleCap);
        }
        if mant.unsigned_abs() > MAX_MANT.unsigned_abs() {
            return Err(DecError::Overflow);
        }
        if mant == 0 {
            return Ok(Dec { mant: 0, scale });
        }
        Ok(Dec { mant, scale })
    }

    /// Grammar: `-?(0|[1-9][0-9]*)(\.[0-9]+)?`. Extra fraction digits are an error.
    pub fn parse(s: &str, scale: u8) -> Result<Self, DecError> {
        if scale > MAX_SCALE {
            return Err(DecError::ScaleCap);
        }
        let b = s.as_bytes();
        if b.is_empty() {
            return Err(DecError::Parse);
        }
        let mut i = 0;
        let neg = if b[0] == b'-' {
            i = 1;
            if i >= b.len() {
                return Err(DecError::Parse);
            }
            true
        } else if b[0] == b'+' {
            return Err(DecError::Parse);
        } else {
            false
        };
        if i >= b.len() {
            return Err(DecError::Parse);
        }
        let int_start = i;
        if b[i] == b'0' {
            i += 1;
        } else if (b'1'..=b'9').contains(&b[i]) {
            i += 1;
            while i < b.len() && b[i].is_ascii_digit() {
                i += 1;
            }
        } else {
            return Err(DecError::Parse);
        }
        let int_part = &s[int_start..i];
        let mut frac = "";
        if i < b.len() {
            if b[i] != b'.' {
                return Err(DecError::Parse);
            }
            i += 1;
            let frac_start = i;
            if i >= b.len() || !b[i].is_ascii_digit() {
                return Err(DecError::Parse);
            }
            while i < b.len() && b[i].is_ascii_digit() {
                i += 1;
            }
            frac = &s[frac_start..i];
            if i != b.len() {
                return Err(DecError::Parse);
            }
        }
        if frac.len() > scale as usize {
            return Err(DecError::Parse);
        }
        let mut digits = String::from(int_part);
        digits.push_str(frac);
        for _ in 0..(scale as usize - frac.len()) {
            digits.push('0');
        }
        let mut mant: i128 = 0;
        for ch in digits.bytes() {
            let d = (ch - b'0') as i128;
            mant = mant
                .checked_mul(10)
                .ok_or(DecError::Overflow)?
                .checked_add(d)
                .ok_or(DecError::Overflow)?;
        }
        if mant > MAX_MANT {
            return Err(DecError::Overflow);
        }
        if mant == 0 {
            return Ok(Dec { mant: 0, scale });
        }
        if neg {
            mant = -mant;
        }
        Ok(Dec { mant, scale })
    }

    /// Exactly `scale` fraction digits, no exponent, no `+`. Zero is unsigned.
    pub fn format(self) -> String {
        let neg = self.mant < 0;
        let mag = self.mant.unsigned_abs();
        let mut digits = mag.to_string();
        let scale = self.scale as usize;
        if scale == 0 {
            if neg && mag != 0 {
                return format!("-{digits}");
            }
            return digits;
        }
        if digits.len() <= scale {
            let pad = scale + 1 - digits.len();
            let mut padded = String::from("0").repeat(pad);
            padded.push_str(&digits);
            digits = padded;
        }
        let split = digits.len() - scale;
        let (int, frac) = digits.split_at(split);
        if neg && mag != 0 {
            format!("-{int}.{frac}")
        } else {
            format!("{int}.{frac}")
        }
    }

    pub fn rescale_up(self, target: u8) -> Result<Self, DecError> {
        if target > MAX_SCALE {
            return Err(DecError::ScaleCap);
        }
        if target < self.scale {
            return Err(DecError::Parse);
        }
        let delta = (target - self.scale) as u32;
        let factor = pow10_checked(delta)?;
        let mant = self.mant.checked_mul(factor).ok_or(DecError::Overflow)?;
        if mant.unsigned_abs() > MAX_MANT.unsigned_abs() {
            return Err(DecError::Overflow);
        }
        Ok(Dec {
            mant,
            scale: target,
        })
    }

    pub fn checked_add(self, other: Self) -> Result<Self, DecError> {
        let scale = self.scale.max(other.scale);
        let a = self.rescale_up(scale)?;
        let b = other.rescale_up(scale)?;
        let mant = a.mant.checked_add(b.mant).ok_or(DecError::Overflow)?;
        Dec::new(mant, scale)
    }

    pub fn checked_sub(self, other: Self) -> Result<Self, DecError> {
        let scale = self.scale.max(other.scale);
        let a = self.rescale_up(scale)?;
        let b = other.rescale_up(scale)?;
        let mant = a.mant.checked_sub(b.mant).ok_or(DecError::Overflow)?;
        Dec::new(mant, scale)
    }

    pub fn checked_mul(self, other: Self) -> Result<Self, DecError> {
        let scale = u16::from(self.scale) + u16::from(other.scale);
        if scale > u16::from(MAX_SCALE) {
            return Err(DecError::ScaleCap);
        }
        let mant = self
            .mant
            .checked_mul(other.mant)
            .ok_or(DecError::Overflow)?;
        Dec::new(mant, scale as u8)
    }

    #[allow(clippy::should_implement_trait)]
    pub fn cmp(self, other: Self) -> Ordering {
        match (self.mant.signum(), other.mant.signum()) {
            (a, b) if a < b => return Ordering::Less,
            (a, b) if a > b => return Ordering::Greater,
            (0, 0) => return Ordering::Equal,
            _ => {}
        }
        let neg = self.mant < 0;
        let sa = self.scale;
        let sb = other.scale;
        let ma = U256::from_u128(self.mant.unsigned_abs());
        let mb = U256::from_u128(other.mant.unsigned_abs());
        // Align the smaller-scale magnitude by ×10^Δ so both sit at max(sa, sb).
        let aligned = if sa >= sb {
            let bb = mb
                .checked_mul_pow10(u32::from(sa - sb))
                .expect("align pow10 fits u256");
            ma.cmp(bb)
        } else {
            let aa = ma
                .checked_mul_pow10(u32::from(sb - sa))
                .expect("align pow10 fits u256");
            aa.cmp(mb)
        };
        if neg { aligned.reverse() } else { aligned }
    }

    pub fn round(self, scale: u8, mode: RoundMode) -> Result<Self, DecError> {
        if scale > MAX_SCALE {
            return Err(DecError::ScaleCap);
        }
        if scale >= self.scale {
            return self.rescale_up(scale);
        }
        let delta = (self.scale - scale) as u32;
        let div = pow10_u128(delta)?;
        let negative = self.mant < 0;
        let mag = self.mant.unsigned_abs();
        let q = mag / div;
        let r = mag % div;
        let mut q = q;
        if r != 0 {
            let twice = r.saturating_mul(2);
            let ord = twice.cmp(&div);
            if bump(mode, negative, ord, q.is_multiple_of(2)) {
                q = q.checked_add(1).ok_or(DecError::Overflow)?;
            }
        }
        if q > MAX_MANT.unsigned_abs() {
            return Err(DecError::Overflow);
        }
        let mant = if negative { -(q as i128) } else { q as i128 };
        Dec::new(if q == 0 { 0 } else { mant }, scale)
    }

    /// Correctly-rounded value of the exact rational a/b at scale S.
    pub fn div(self, other: Self, scale: u8, mode: RoundMode) -> Result<Self, DecError> {
        if scale > MAX_SCALE {
            return Err(DecError::ScaleCap);
        }
        if other.mant == 0 {
            return Err(DecError::DivZero);
        }
        let k = i32::from(scale) - i32::from(self.scale) + i32::from(other.scale);
        let negative = (self.mant < 0) != (other.mant < 0);
        let a_mag = self.mant.unsigned_abs();
        let b_mag = other.mant.unsigned_abs();
        let (q, r, d): (u128, u128, u128) = if k >= 0 {
            let n = U256::from_u128(a_mag)
                .checked_mul_pow10(k as u32)
                .ok_or(DecError::Overflow)?;
            let (qq, rr) = n.div_rem_u128(b_mag);
            if qq.hi() != 0 {
                return Err(DecError::Overflow);
            }
            (qq.lo(), rr, b_mag)
        } else {
            let fold = pow10_u128((-k) as u32)?;
            match b_mag.checked_mul(fold) {
                None => {
                    // Fold overflows u128: q = 0, r = |a.mant|, 2r < d guaranteed.
                    (0, a_mag, u128::MAX)
                }
                Some(d) => {
                    let (qq, rr) = U256::from_u128(a_mag).div_rem_u128(d);
                    if qq.hi() != 0 {
                        return Err(DecError::Overflow);
                    }
                    (qq.lo(), rr, d)
                }
            }
        };
        let mut q = q;
        if r != 0 {
            let twice = match r.checked_mul(2) {
                Some(t) => t.cmp(&d),
                None => Ordering::Greater,
            };
            // For the fold-overflow path we pass d = u128::MAX as a stand-in
            // that is still strictly greater than 2r (r ≤ 10^38−1).
            if k < 0
                && b_mag
                    .checked_mul(pow10_u128((-k) as u32).unwrap_or(0))
                    .is_none()
            {
                // 2r < d is guaranteed; twice is Less.
                let _ = d;
            }
            let ord = if k < 0
                && b_mag
                    .checked_mul(pow10_u128((-k) as u32).unwrap_or(0))
                    .is_none()
            {
                Ordering::Less
            } else {
                twice
            };
            if bump(mode, negative, ord, q.is_multiple_of(2)) {
                q = q.checked_add(1).ok_or(DecError::Overflow)?;
            }
        }
        if q > MAX_MANT.unsigned_abs() {
            return Err(DecError::Overflow);
        }
        let mant = if q == 0 {
            0
        } else if negative {
            -(q as i128)
        } else {
            q as i128
        };
        Dec::new(mant, scale)
    }
}

fn pow10_checked(k: u32) -> Result<i128, DecError> {
    let mut r = 1i128;
    for _ in 0..k {
        r = r.checked_mul(10).ok_or(DecError::Overflow)?;
    }
    Ok(r)
}

fn pow10_u128(k: u32) -> Result<u128, DecError> {
    let mut r = 1u128;
    for _ in 0..k {
        r = r.checked_mul(10).ok_or(DecError::Overflow)?;
    }
    Ok(r)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dec_error_variants_reachable() {
        assert_eq!(Dec::parse("1.", 2).unwrap_err(), DecError::Parse);
        assert_eq!(
            Dec::parse("99999999999999999999999999999999999999", 0)
                .unwrap()
                .checked_add(Dec::parse("1", 0).unwrap())
                .unwrap_err(),
            DecError::Overflow
        );
        assert_eq!(
            Dec::parse("1", 7)
                .unwrap()
                .checked_mul(Dec::parse("1", 6).unwrap())
                .unwrap_err(),
            DecError::ScaleCap
        );
    }

    #[test]
    fn bump_truth_table() {
        let modes = [
            RoundMode::Down,
            RoundMode::Up,
            RoundMode::Floor,
            RoundMode::Ceiling,
            RoundMode::HalfUp,
            RoundMode::HalfDown,
            RoundMode::HalfEven,
        ];
        let ords = [Ordering::Less, Ordering::Equal, Ordering::Greater];
        for mode in modes {
            for &ord in &ords {
                for q_even in [true, false] {
                    for negative in [false, true] {
                        let got = bump(mode, negative, ord, q_even);
                        let want = match mode {
                            RoundMode::Down => false,
                            RoundMode::Up => true,
                            RoundMode::Floor => negative,
                            RoundMode::Ceiling => !negative,
                            RoundMode::HalfUp => matches!(ord, Ordering::Greater | Ordering::Equal),
                            RoundMode::HalfDown => ord == Ordering::Greater,
                            RoundMode::HalfEven => {
                                ord == Ordering::Greater || (ord == Ordering::Equal && !q_even)
                            }
                        };
                        assert_eq!(
                            got, want,
                            "mode={mode:?} ord={ord:?} even={q_even} neg={negative}"
                        );
                    }
                }
            }
        }
    }
}
