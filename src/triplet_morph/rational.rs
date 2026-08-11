use std::cmp::Ordering;

/// Exact rational value used for beat positions, warp interpolation, and
/// displacement comparison. The denominator is always positive and the
/// fraction is stored fully reduced, so derived equality is value
/// equality. All planner-domain values are tiny (denominators bounded by
/// a few thousand), so i128 intermediates cannot overflow.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Rat {
    num: i128,
    den: i128,
}

impl Rat {
    pub const ZERO: Rat = Rat { num: 0, den: 1 };
    pub const ONE: Rat = Rat { num: 1, den: 1 };

    /// Build a reduced rational. Returns None for a zero denominator.
    pub fn new(num: i128, den: i128) -> Option<Self> {
        if den == 0 {
            return None;
        }
        Some(Self::reduced(num, den))
    }

    pub const fn int(value: i128) -> Self {
        Self { num: value, den: 1 }
    }

    /// Internal constructor for a denominator already known to be nonzero.
    fn reduced(num: i128, den: i128) -> Self {
        let (mut num, mut den) = if den < 0 { (-num, -den) } else { (num, den) };
        let g = gcd(num.unsigned_abs(), den.unsigned_abs());
        if g > 1 {
            num /= g as i128;
            den /= g as i128;
        }
        Self { num, den }
    }

    pub fn num(self) -> i128 {
        self.num
    }

    pub fn den(self) -> i128 {
        self.den
    }

    pub fn add(self, other: Self) -> Self {
        Self::reduced(
            self.num * other.den + other.num * self.den,
            self.den * other.den,
        )
    }

    pub fn sub(self, other: Self) -> Self {
        Self::reduced(
            self.num * other.den - other.num * self.den,
            self.den * other.den,
        )
    }

    pub fn mul(self, other: Self) -> Self {
        Self::reduced(self.num * other.num, self.den * other.den)
    }

    pub fn abs(self) -> Self {
        Self {
            num: self.num.abs(),
            den: self.den,
        }
    }

    pub fn is_negative(self) -> bool {
        self.num < 0
    }
}

impl Ord for Rat {
    fn cmp(&self, other: &Self) -> Ordering {
        (self.num * other.den).cmp(&(other.num * self.den))
    }
}

impl PartialOrd for Rat {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

fn gcd(mut a: u128, mut b: u128) -> u128 {
    while b != 0 {
        let r = a % b;
        a = b;
        b = r;
    }
    a.max(1)
}
