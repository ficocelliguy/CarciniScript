// (wrapping prelude)
// `std::num::Wrapping`, compiled in when a program names it: arithmetic that wraps around, for
// each integer type up to 64 bits.

/// `std::num::Wrapping<T>`: `+`, `-`, `*`, `/`, `%`, `-x`, `!x`, the bitwise operators and their
/// assignments wrap.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Wrapping<T>(pub T);

// (std's `{:?}` is the number's own.)
impl<T: std::fmt::Debug> std::fmt::Debug for Wrapping<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{:?}", self.0)
    }
}

impl<T: std::fmt::Display> std::fmt::Display for Wrapping<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::ops::Add for Wrapping<u8> {
    type Output = Wrapping<u8>;
    fn add(self, o: Wrapping<u8>) -> Wrapping<u8> {
        Wrapping(self.0.wrapping_add(o.0))
    }
}

impl std::ops::Sub for Wrapping<u8> {
    type Output = Wrapping<u8>;
    fn sub(self, o: Wrapping<u8>) -> Wrapping<u8> {
        Wrapping(self.0.wrapping_sub(o.0))
    }
}

impl std::ops::Mul for Wrapping<u8> {
    type Output = Wrapping<u8>;
    fn mul(self, o: Wrapping<u8>) -> Wrapping<u8> {
        Wrapping(self.0.wrapping_mul(o.0))
    }
}

impl std::ops::Div for Wrapping<u8> {
    type Output = Wrapping<u8>;
    fn div(self, o: Wrapping<u8>) -> Wrapping<u8> {
        Wrapping(self.0.wrapping_div(o.0))
    }
}

impl std::ops::Rem for Wrapping<u8> {
    type Output = Wrapping<u8>;
    fn rem(self, o: Wrapping<u8>) -> Wrapping<u8> {
        Wrapping(self.0.wrapping_rem(o.0))
    }
}

impl std::ops::BitAnd for Wrapping<u8> {
    type Output = Wrapping<u8>;
    fn bitand(self, o: Wrapping<u8>) -> Wrapping<u8> {
        Wrapping(self.0 & o.0)
    }
}

impl std::ops::BitOr for Wrapping<u8> {
    type Output = Wrapping<u8>;
    fn bitor(self, o: Wrapping<u8>) -> Wrapping<u8> {
        Wrapping(self.0 | o.0)
    }
}

impl std::ops::BitXor for Wrapping<u8> {
    type Output = Wrapping<u8>;
    fn bitxor(self, o: Wrapping<u8>) -> Wrapping<u8> {
        Wrapping(self.0 ^ o.0)
    }
}

impl std::ops::AddAssign for Wrapping<u8> {
    fn add_assign(&mut self, o: Wrapping<u8>) {
        self.0 = self.0.wrapping_add(o.0);
    }
}

impl std::ops::SubAssign for Wrapping<u8> {
    fn sub_assign(&mut self, o: Wrapping<u8>) {
        self.0 = self.0.wrapping_sub(o.0);
    }
}

impl std::ops::MulAssign for Wrapping<u8> {
    fn mul_assign(&mut self, o: Wrapping<u8>) {
        self.0 = self.0.wrapping_mul(o.0);
    }
}

impl std::ops::DivAssign for Wrapping<u8> {
    fn div_assign(&mut self, o: Wrapping<u8>) {
        self.0 = self.0.wrapping_div(o.0);
    }
}

impl std::ops::RemAssign for Wrapping<u8> {
    fn rem_assign(&mut self, o: Wrapping<u8>) {
        self.0 = self.0.wrapping_rem(o.0);
    }
}

impl std::ops::BitAndAssign for Wrapping<u8> {
    fn bitand_assign(&mut self, o: Wrapping<u8>) {
        self.0 = self.0 & o.0;
    }
}

impl std::ops::BitOrAssign for Wrapping<u8> {
    fn bitor_assign(&mut self, o: Wrapping<u8>) {
        self.0 = self.0 | o.0;
    }
}

impl std::ops::BitXorAssign for Wrapping<u8> {
    fn bitxor_assign(&mut self, o: Wrapping<u8>) {
        self.0 = self.0 ^ o.0;
    }
}

impl std::ops::Not for Wrapping<u8> {
    type Output = Wrapping<u8>;
    fn not(self) -> Wrapping<u8> {
        Wrapping(!self.0)
    }
}

impl std::ops::Add for Wrapping<u16> {
    type Output = Wrapping<u16>;
    fn add(self, o: Wrapping<u16>) -> Wrapping<u16> {
        Wrapping(self.0.wrapping_add(o.0))
    }
}

impl std::ops::Sub for Wrapping<u16> {
    type Output = Wrapping<u16>;
    fn sub(self, o: Wrapping<u16>) -> Wrapping<u16> {
        Wrapping(self.0.wrapping_sub(o.0))
    }
}

impl std::ops::Mul for Wrapping<u16> {
    type Output = Wrapping<u16>;
    fn mul(self, o: Wrapping<u16>) -> Wrapping<u16> {
        Wrapping(self.0.wrapping_mul(o.0))
    }
}

impl std::ops::Div for Wrapping<u16> {
    type Output = Wrapping<u16>;
    fn div(self, o: Wrapping<u16>) -> Wrapping<u16> {
        Wrapping(self.0.wrapping_div(o.0))
    }
}

impl std::ops::Rem for Wrapping<u16> {
    type Output = Wrapping<u16>;
    fn rem(self, o: Wrapping<u16>) -> Wrapping<u16> {
        Wrapping(self.0.wrapping_rem(o.0))
    }
}

impl std::ops::BitAnd for Wrapping<u16> {
    type Output = Wrapping<u16>;
    fn bitand(self, o: Wrapping<u16>) -> Wrapping<u16> {
        Wrapping(self.0 & o.0)
    }
}

impl std::ops::BitOr for Wrapping<u16> {
    type Output = Wrapping<u16>;
    fn bitor(self, o: Wrapping<u16>) -> Wrapping<u16> {
        Wrapping(self.0 | o.0)
    }
}

impl std::ops::BitXor for Wrapping<u16> {
    type Output = Wrapping<u16>;
    fn bitxor(self, o: Wrapping<u16>) -> Wrapping<u16> {
        Wrapping(self.0 ^ o.0)
    }
}

impl std::ops::AddAssign for Wrapping<u16> {
    fn add_assign(&mut self, o: Wrapping<u16>) {
        self.0 = self.0.wrapping_add(o.0);
    }
}

impl std::ops::SubAssign for Wrapping<u16> {
    fn sub_assign(&mut self, o: Wrapping<u16>) {
        self.0 = self.0.wrapping_sub(o.0);
    }
}

impl std::ops::MulAssign for Wrapping<u16> {
    fn mul_assign(&mut self, o: Wrapping<u16>) {
        self.0 = self.0.wrapping_mul(o.0);
    }
}

impl std::ops::DivAssign for Wrapping<u16> {
    fn div_assign(&mut self, o: Wrapping<u16>) {
        self.0 = self.0.wrapping_div(o.0);
    }
}

impl std::ops::RemAssign for Wrapping<u16> {
    fn rem_assign(&mut self, o: Wrapping<u16>) {
        self.0 = self.0.wrapping_rem(o.0);
    }
}

impl std::ops::BitAndAssign for Wrapping<u16> {
    fn bitand_assign(&mut self, o: Wrapping<u16>) {
        self.0 = self.0 & o.0;
    }
}

impl std::ops::BitOrAssign for Wrapping<u16> {
    fn bitor_assign(&mut self, o: Wrapping<u16>) {
        self.0 = self.0 | o.0;
    }
}

impl std::ops::BitXorAssign for Wrapping<u16> {
    fn bitxor_assign(&mut self, o: Wrapping<u16>) {
        self.0 = self.0 ^ o.0;
    }
}

impl std::ops::Not for Wrapping<u16> {
    type Output = Wrapping<u16>;
    fn not(self) -> Wrapping<u16> {
        Wrapping(!self.0)
    }
}

impl std::ops::Add for Wrapping<u32> {
    type Output = Wrapping<u32>;
    fn add(self, o: Wrapping<u32>) -> Wrapping<u32> {
        Wrapping(self.0.wrapping_add(o.0))
    }
}

impl std::ops::Sub for Wrapping<u32> {
    type Output = Wrapping<u32>;
    fn sub(self, o: Wrapping<u32>) -> Wrapping<u32> {
        Wrapping(self.0.wrapping_sub(o.0))
    }
}

impl std::ops::Mul for Wrapping<u32> {
    type Output = Wrapping<u32>;
    fn mul(self, o: Wrapping<u32>) -> Wrapping<u32> {
        Wrapping(self.0.wrapping_mul(o.0))
    }
}

impl std::ops::Div for Wrapping<u32> {
    type Output = Wrapping<u32>;
    fn div(self, o: Wrapping<u32>) -> Wrapping<u32> {
        Wrapping(self.0.wrapping_div(o.0))
    }
}

impl std::ops::Rem for Wrapping<u32> {
    type Output = Wrapping<u32>;
    fn rem(self, o: Wrapping<u32>) -> Wrapping<u32> {
        Wrapping(self.0.wrapping_rem(o.0))
    }
}

impl std::ops::BitAnd for Wrapping<u32> {
    type Output = Wrapping<u32>;
    fn bitand(self, o: Wrapping<u32>) -> Wrapping<u32> {
        Wrapping(self.0 & o.0)
    }
}

impl std::ops::BitOr for Wrapping<u32> {
    type Output = Wrapping<u32>;
    fn bitor(self, o: Wrapping<u32>) -> Wrapping<u32> {
        Wrapping(self.0 | o.0)
    }
}

impl std::ops::BitXor for Wrapping<u32> {
    type Output = Wrapping<u32>;
    fn bitxor(self, o: Wrapping<u32>) -> Wrapping<u32> {
        Wrapping(self.0 ^ o.0)
    }
}

impl std::ops::AddAssign for Wrapping<u32> {
    fn add_assign(&mut self, o: Wrapping<u32>) {
        self.0 = self.0.wrapping_add(o.0);
    }
}

impl std::ops::SubAssign for Wrapping<u32> {
    fn sub_assign(&mut self, o: Wrapping<u32>) {
        self.0 = self.0.wrapping_sub(o.0);
    }
}

impl std::ops::MulAssign for Wrapping<u32> {
    fn mul_assign(&mut self, o: Wrapping<u32>) {
        self.0 = self.0.wrapping_mul(o.0);
    }
}

impl std::ops::DivAssign for Wrapping<u32> {
    fn div_assign(&mut self, o: Wrapping<u32>) {
        self.0 = self.0.wrapping_div(o.0);
    }
}

impl std::ops::RemAssign for Wrapping<u32> {
    fn rem_assign(&mut self, o: Wrapping<u32>) {
        self.0 = self.0.wrapping_rem(o.0);
    }
}

impl std::ops::BitAndAssign for Wrapping<u32> {
    fn bitand_assign(&mut self, o: Wrapping<u32>) {
        self.0 = self.0 & o.0;
    }
}

impl std::ops::BitOrAssign for Wrapping<u32> {
    fn bitor_assign(&mut self, o: Wrapping<u32>) {
        self.0 = self.0 | o.0;
    }
}

impl std::ops::BitXorAssign for Wrapping<u32> {
    fn bitxor_assign(&mut self, o: Wrapping<u32>) {
        self.0 = self.0 ^ o.0;
    }
}

impl std::ops::Not for Wrapping<u32> {
    type Output = Wrapping<u32>;
    fn not(self) -> Wrapping<u32> {
        Wrapping(!self.0)
    }
}

impl std::ops::Add for Wrapping<u64> {
    type Output = Wrapping<u64>;
    fn add(self, o: Wrapping<u64>) -> Wrapping<u64> {
        Wrapping(self.0.wrapping_add(o.0))
    }
}

impl std::ops::Sub for Wrapping<u64> {
    type Output = Wrapping<u64>;
    fn sub(self, o: Wrapping<u64>) -> Wrapping<u64> {
        Wrapping(self.0.wrapping_sub(o.0))
    }
}

impl std::ops::Mul for Wrapping<u64> {
    type Output = Wrapping<u64>;
    fn mul(self, o: Wrapping<u64>) -> Wrapping<u64> {
        Wrapping(self.0.wrapping_mul(o.0))
    }
}

impl std::ops::Div for Wrapping<u64> {
    type Output = Wrapping<u64>;
    fn div(self, o: Wrapping<u64>) -> Wrapping<u64> {
        Wrapping(self.0.wrapping_div(o.0))
    }
}

impl std::ops::Rem for Wrapping<u64> {
    type Output = Wrapping<u64>;
    fn rem(self, o: Wrapping<u64>) -> Wrapping<u64> {
        Wrapping(self.0.wrapping_rem(o.0))
    }
}

impl std::ops::BitAnd for Wrapping<u64> {
    type Output = Wrapping<u64>;
    fn bitand(self, o: Wrapping<u64>) -> Wrapping<u64> {
        Wrapping(self.0 & o.0)
    }
}

impl std::ops::BitOr for Wrapping<u64> {
    type Output = Wrapping<u64>;
    fn bitor(self, o: Wrapping<u64>) -> Wrapping<u64> {
        Wrapping(self.0 | o.0)
    }
}

impl std::ops::BitXor for Wrapping<u64> {
    type Output = Wrapping<u64>;
    fn bitxor(self, o: Wrapping<u64>) -> Wrapping<u64> {
        Wrapping(self.0 ^ o.0)
    }
}

impl std::ops::AddAssign for Wrapping<u64> {
    fn add_assign(&mut self, o: Wrapping<u64>) {
        self.0 = self.0.wrapping_add(o.0);
    }
}

impl std::ops::SubAssign for Wrapping<u64> {
    fn sub_assign(&mut self, o: Wrapping<u64>) {
        self.0 = self.0.wrapping_sub(o.0);
    }
}

impl std::ops::MulAssign for Wrapping<u64> {
    fn mul_assign(&mut self, o: Wrapping<u64>) {
        self.0 = self.0.wrapping_mul(o.0);
    }
}

impl std::ops::DivAssign for Wrapping<u64> {
    fn div_assign(&mut self, o: Wrapping<u64>) {
        self.0 = self.0.wrapping_div(o.0);
    }
}

impl std::ops::RemAssign for Wrapping<u64> {
    fn rem_assign(&mut self, o: Wrapping<u64>) {
        self.0 = self.0.wrapping_rem(o.0);
    }
}

impl std::ops::BitAndAssign for Wrapping<u64> {
    fn bitand_assign(&mut self, o: Wrapping<u64>) {
        self.0 = self.0 & o.0;
    }
}

impl std::ops::BitOrAssign for Wrapping<u64> {
    fn bitor_assign(&mut self, o: Wrapping<u64>) {
        self.0 = self.0 | o.0;
    }
}

impl std::ops::BitXorAssign for Wrapping<u64> {
    fn bitxor_assign(&mut self, o: Wrapping<u64>) {
        self.0 = self.0 ^ o.0;
    }
}

impl std::ops::Not for Wrapping<u64> {
    type Output = Wrapping<u64>;
    fn not(self) -> Wrapping<u64> {
        Wrapping(!self.0)
    }
}

impl std::ops::Add for Wrapping<usize> {
    type Output = Wrapping<usize>;
    fn add(self, o: Wrapping<usize>) -> Wrapping<usize> {
        Wrapping(self.0.wrapping_add(o.0))
    }
}

impl std::ops::Sub for Wrapping<usize> {
    type Output = Wrapping<usize>;
    fn sub(self, o: Wrapping<usize>) -> Wrapping<usize> {
        Wrapping(self.0.wrapping_sub(o.0))
    }
}

impl std::ops::Mul for Wrapping<usize> {
    type Output = Wrapping<usize>;
    fn mul(self, o: Wrapping<usize>) -> Wrapping<usize> {
        Wrapping(self.0.wrapping_mul(o.0))
    }
}

impl std::ops::Div for Wrapping<usize> {
    type Output = Wrapping<usize>;
    fn div(self, o: Wrapping<usize>) -> Wrapping<usize> {
        Wrapping(self.0.wrapping_div(o.0))
    }
}

impl std::ops::Rem for Wrapping<usize> {
    type Output = Wrapping<usize>;
    fn rem(self, o: Wrapping<usize>) -> Wrapping<usize> {
        Wrapping(self.0.wrapping_rem(o.0))
    }
}

impl std::ops::BitAnd for Wrapping<usize> {
    type Output = Wrapping<usize>;
    fn bitand(self, o: Wrapping<usize>) -> Wrapping<usize> {
        Wrapping(self.0 & o.0)
    }
}

impl std::ops::BitOr for Wrapping<usize> {
    type Output = Wrapping<usize>;
    fn bitor(self, o: Wrapping<usize>) -> Wrapping<usize> {
        Wrapping(self.0 | o.0)
    }
}

impl std::ops::BitXor for Wrapping<usize> {
    type Output = Wrapping<usize>;
    fn bitxor(self, o: Wrapping<usize>) -> Wrapping<usize> {
        Wrapping(self.0 ^ o.0)
    }
}

impl std::ops::AddAssign for Wrapping<usize> {
    fn add_assign(&mut self, o: Wrapping<usize>) {
        self.0 = self.0.wrapping_add(o.0);
    }
}

impl std::ops::SubAssign for Wrapping<usize> {
    fn sub_assign(&mut self, o: Wrapping<usize>) {
        self.0 = self.0.wrapping_sub(o.0);
    }
}

impl std::ops::MulAssign for Wrapping<usize> {
    fn mul_assign(&mut self, o: Wrapping<usize>) {
        self.0 = self.0.wrapping_mul(o.0);
    }
}

impl std::ops::DivAssign for Wrapping<usize> {
    fn div_assign(&mut self, o: Wrapping<usize>) {
        self.0 = self.0.wrapping_div(o.0);
    }
}

impl std::ops::RemAssign for Wrapping<usize> {
    fn rem_assign(&mut self, o: Wrapping<usize>) {
        self.0 = self.0.wrapping_rem(o.0);
    }
}

impl std::ops::BitAndAssign for Wrapping<usize> {
    fn bitand_assign(&mut self, o: Wrapping<usize>) {
        self.0 = self.0 & o.0;
    }
}

impl std::ops::BitOrAssign for Wrapping<usize> {
    fn bitor_assign(&mut self, o: Wrapping<usize>) {
        self.0 = self.0 | o.0;
    }
}

impl std::ops::BitXorAssign for Wrapping<usize> {
    fn bitxor_assign(&mut self, o: Wrapping<usize>) {
        self.0 = self.0 ^ o.0;
    }
}

impl std::ops::Not for Wrapping<usize> {
    type Output = Wrapping<usize>;
    fn not(self) -> Wrapping<usize> {
        Wrapping(!self.0)
    }
}

impl std::ops::Add for Wrapping<i8> {
    type Output = Wrapping<i8>;
    fn add(self, o: Wrapping<i8>) -> Wrapping<i8> {
        Wrapping(self.0.wrapping_add(o.0))
    }
}

impl std::ops::Sub for Wrapping<i8> {
    type Output = Wrapping<i8>;
    fn sub(self, o: Wrapping<i8>) -> Wrapping<i8> {
        Wrapping(self.0.wrapping_sub(o.0))
    }
}

impl std::ops::Mul for Wrapping<i8> {
    type Output = Wrapping<i8>;
    fn mul(self, o: Wrapping<i8>) -> Wrapping<i8> {
        Wrapping(self.0.wrapping_mul(o.0))
    }
}

impl std::ops::Div for Wrapping<i8> {
    type Output = Wrapping<i8>;
    fn div(self, o: Wrapping<i8>) -> Wrapping<i8> {
        Wrapping(self.0.wrapping_div(o.0))
    }
}

impl std::ops::Rem for Wrapping<i8> {
    type Output = Wrapping<i8>;
    fn rem(self, o: Wrapping<i8>) -> Wrapping<i8> {
        Wrapping(self.0.wrapping_rem(o.0))
    }
}

impl std::ops::BitAnd for Wrapping<i8> {
    type Output = Wrapping<i8>;
    fn bitand(self, o: Wrapping<i8>) -> Wrapping<i8> {
        Wrapping(self.0 & o.0)
    }
}

impl std::ops::BitOr for Wrapping<i8> {
    type Output = Wrapping<i8>;
    fn bitor(self, o: Wrapping<i8>) -> Wrapping<i8> {
        Wrapping(self.0 | o.0)
    }
}

impl std::ops::BitXor for Wrapping<i8> {
    type Output = Wrapping<i8>;
    fn bitxor(self, o: Wrapping<i8>) -> Wrapping<i8> {
        Wrapping(self.0 ^ o.0)
    }
}

impl std::ops::AddAssign for Wrapping<i8> {
    fn add_assign(&mut self, o: Wrapping<i8>) {
        self.0 = self.0.wrapping_add(o.0);
    }
}

impl std::ops::SubAssign for Wrapping<i8> {
    fn sub_assign(&mut self, o: Wrapping<i8>) {
        self.0 = self.0.wrapping_sub(o.0);
    }
}

impl std::ops::MulAssign for Wrapping<i8> {
    fn mul_assign(&mut self, o: Wrapping<i8>) {
        self.0 = self.0.wrapping_mul(o.0);
    }
}

impl std::ops::DivAssign for Wrapping<i8> {
    fn div_assign(&mut self, o: Wrapping<i8>) {
        self.0 = self.0.wrapping_div(o.0);
    }
}

impl std::ops::RemAssign for Wrapping<i8> {
    fn rem_assign(&mut self, o: Wrapping<i8>) {
        self.0 = self.0.wrapping_rem(o.0);
    }
}

impl std::ops::BitAndAssign for Wrapping<i8> {
    fn bitand_assign(&mut self, o: Wrapping<i8>) {
        self.0 = self.0 & o.0;
    }
}

impl std::ops::BitOrAssign for Wrapping<i8> {
    fn bitor_assign(&mut self, o: Wrapping<i8>) {
        self.0 = self.0 | o.0;
    }
}

impl std::ops::BitXorAssign for Wrapping<i8> {
    fn bitxor_assign(&mut self, o: Wrapping<i8>) {
        self.0 = self.0 ^ o.0;
    }
}

impl std::ops::Not for Wrapping<i8> {
    type Output = Wrapping<i8>;
    fn not(self) -> Wrapping<i8> {
        Wrapping(!self.0)
    }
}

impl std::ops::Neg for Wrapping<i8> {
    type Output = Wrapping<i8>;
    fn neg(self) -> Wrapping<i8> {
        Wrapping(self.0.wrapping_neg())
    }
}

impl std::ops::Add for Wrapping<i16> {
    type Output = Wrapping<i16>;
    fn add(self, o: Wrapping<i16>) -> Wrapping<i16> {
        Wrapping(self.0.wrapping_add(o.0))
    }
}

impl std::ops::Sub for Wrapping<i16> {
    type Output = Wrapping<i16>;
    fn sub(self, o: Wrapping<i16>) -> Wrapping<i16> {
        Wrapping(self.0.wrapping_sub(o.0))
    }
}

impl std::ops::Mul for Wrapping<i16> {
    type Output = Wrapping<i16>;
    fn mul(self, o: Wrapping<i16>) -> Wrapping<i16> {
        Wrapping(self.0.wrapping_mul(o.0))
    }
}

impl std::ops::Div for Wrapping<i16> {
    type Output = Wrapping<i16>;
    fn div(self, o: Wrapping<i16>) -> Wrapping<i16> {
        Wrapping(self.0.wrapping_div(o.0))
    }
}

impl std::ops::Rem for Wrapping<i16> {
    type Output = Wrapping<i16>;
    fn rem(self, o: Wrapping<i16>) -> Wrapping<i16> {
        Wrapping(self.0.wrapping_rem(o.0))
    }
}

impl std::ops::BitAnd for Wrapping<i16> {
    type Output = Wrapping<i16>;
    fn bitand(self, o: Wrapping<i16>) -> Wrapping<i16> {
        Wrapping(self.0 & o.0)
    }
}

impl std::ops::BitOr for Wrapping<i16> {
    type Output = Wrapping<i16>;
    fn bitor(self, o: Wrapping<i16>) -> Wrapping<i16> {
        Wrapping(self.0 | o.0)
    }
}

impl std::ops::BitXor for Wrapping<i16> {
    type Output = Wrapping<i16>;
    fn bitxor(self, o: Wrapping<i16>) -> Wrapping<i16> {
        Wrapping(self.0 ^ o.0)
    }
}

impl std::ops::AddAssign for Wrapping<i16> {
    fn add_assign(&mut self, o: Wrapping<i16>) {
        self.0 = self.0.wrapping_add(o.0);
    }
}

impl std::ops::SubAssign for Wrapping<i16> {
    fn sub_assign(&mut self, o: Wrapping<i16>) {
        self.0 = self.0.wrapping_sub(o.0);
    }
}

impl std::ops::MulAssign for Wrapping<i16> {
    fn mul_assign(&mut self, o: Wrapping<i16>) {
        self.0 = self.0.wrapping_mul(o.0);
    }
}

impl std::ops::DivAssign for Wrapping<i16> {
    fn div_assign(&mut self, o: Wrapping<i16>) {
        self.0 = self.0.wrapping_div(o.0);
    }
}

impl std::ops::RemAssign for Wrapping<i16> {
    fn rem_assign(&mut self, o: Wrapping<i16>) {
        self.0 = self.0.wrapping_rem(o.0);
    }
}

impl std::ops::BitAndAssign for Wrapping<i16> {
    fn bitand_assign(&mut self, o: Wrapping<i16>) {
        self.0 = self.0 & o.0;
    }
}

impl std::ops::BitOrAssign for Wrapping<i16> {
    fn bitor_assign(&mut self, o: Wrapping<i16>) {
        self.0 = self.0 | o.0;
    }
}

impl std::ops::BitXorAssign for Wrapping<i16> {
    fn bitxor_assign(&mut self, o: Wrapping<i16>) {
        self.0 = self.0 ^ o.0;
    }
}

impl std::ops::Not for Wrapping<i16> {
    type Output = Wrapping<i16>;
    fn not(self) -> Wrapping<i16> {
        Wrapping(!self.0)
    }
}

impl std::ops::Neg for Wrapping<i16> {
    type Output = Wrapping<i16>;
    fn neg(self) -> Wrapping<i16> {
        Wrapping(self.0.wrapping_neg())
    }
}

impl std::ops::Add for Wrapping<i32> {
    type Output = Wrapping<i32>;
    fn add(self, o: Wrapping<i32>) -> Wrapping<i32> {
        Wrapping(self.0.wrapping_add(o.0))
    }
}

impl std::ops::Sub for Wrapping<i32> {
    type Output = Wrapping<i32>;
    fn sub(self, o: Wrapping<i32>) -> Wrapping<i32> {
        Wrapping(self.0.wrapping_sub(o.0))
    }
}

impl std::ops::Mul for Wrapping<i32> {
    type Output = Wrapping<i32>;
    fn mul(self, o: Wrapping<i32>) -> Wrapping<i32> {
        Wrapping(self.0.wrapping_mul(o.0))
    }
}

impl std::ops::Div for Wrapping<i32> {
    type Output = Wrapping<i32>;
    fn div(self, o: Wrapping<i32>) -> Wrapping<i32> {
        Wrapping(self.0.wrapping_div(o.0))
    }
}

impl std::ops::Rem for Wrapping<i32> {
    type Output = Wrapping<i32>;
    fn rem(self, o: Wrapping<i32>) -> Wrapping<i32> {
        Wrapping(self.0.wrapping_rem(o.0))
    }
}

impl std::ops::BitAnd for Wrapping<i32> {
    type Output = Wrapping<i32>;
    fn bitand(self, o: Wrapping<i32>) -> Wrapping<i32> {
        Wrapping(self.0 & o.0)
    }
}

impl std::ops::BitOr for Wrapping<i32> {
    type Output = Wrapping<i32>;
    fn bitor(self, o: Wrapping<i32>) -> Wrapping<i32> {
        Wrapping(self.0 | o.0)
    }
}

impl std::ops::BitXor for Wrapping<i32> {
    type Output = Wrapping<i32>;
    fn bitxor(self, o: Wrapping<i32>) -> Wrapping<i32> {
        Wrapping(self.0 ^ o.0)
    }
}

impl std::ops::AddAssign for Wrapping<i32> {
    fn add_assign(&mut self, o: Wrapping<i32>) {
        self.0 = self.0.wrapping_add(o.0);
    }
}

impl std::ops::SubAssign for Wrapping<i32> {
    fn sub_assign(&mut self, o: Wrapping<i32>) {
        self.0 = self.0.wrapping_sub(o.0);
    }
}

impl std::ops::MulAssign for Wrapping<i32> {
    fn mul_assign(&mut self, o: Wrapping<i32>) {
        self.0 = self.0.wrapping_mul(o.0);
    }
}

impl std::ops::DivAssign for Wrapping<i32> {
    fn div_assign(&mut self, o: Wrapping<i32>) {
        self.0 = self.0.wrapping_div(o.0);
    }
}

impl std::ops::RemAssign for Wrapping<i32> {
    fn rem_assign(&mut self, o: Wrapping<i32>) {
        self.0 = self.0.wrapping_rem(o.0);
    }
}

impl std::ops::BitAndAssign for Wrapping<i32> {
    fn bitand_assign(&mut self, o: Wrapping<i32>) {
        self.0 = self.0 & o.0;
    }
}

impl std::ops::BitOrAssign for Wrapping<i32> {
    fn bitor_assign(&mut self, o: Wrapping<i32>) {
        self.0 = self.0 | o.0;
    }
}

impl std::ops::BitXorAssign for Wrapping<i32> {
    fn bitxor_assign(&mut self, o: Wrapping<i32>) {
        self.0 = self.0 ^ o.0;
    }
}

impl std::ops::Not for Wrapping<i32> {
    type Output = Wrapping<i32>;
    fn not(self) -> Wrapping<i32> {
        Wrapping(!self.0)
    }
}

impl std::ops::Neg for Wrapping<i32> {
    type Output = Wrapping<i32>;
    fn neg(self) -> Wrapping<i32> {
        Wrapping(self.0.wrapping_neg())
    }
}

impl std::ops::Add for Wrapping<i64> {
    type Output = Wrapping<i64>;
    fn add(self, o: Wrapping<i64>) -> Wrapping<i64> {
        Wrapping(self.0.wrapping_add(o.0))
    }
}

impl std::ops::Sub for Wrapping<i64> {
    type Output = Wrapping<i64>;
    fn sub(self, o: Wrapping<i64>) -> Wrapping<i64> {
        Wrapping(self.0.wrapping_sub(o.0))
    }
}

impl std::ops::Mul for Wrapping<i64> {
    type Output = Wrapping<i64>;
    fn mul(self, o: Wrapping<i64>) -> Wrapping<i64> {
        Wrapping(self.0.wrapping_mul(o.0))
    }
}

impl std::ops::Div for Wrapping<i64> {
    type Output = Wrapping<i64>;
    fn div(self, o: Wrapping<i64>) -> Wrapping<i64> {
        Wrapping(self.0.wrapping_div(o.0))
    }
}

impl std::ops::Rem for Wrapping<i64> {
    type Output = Wrapping<i64>;
    fn rem(self, o: Wrapping<i64>) -> Wrapping<i64> {
        Wrapping(self.0.wrapping_rem(o.0))
    }
}

impl std::ops::BitAnd for Wrapping<i64> {
    type Output = Wrapping<i64>;
    fn bitand(self, o: Wrapping<i64>) -> Wrapping<i64> {
        Wrapping(self.0 & o.0)
    }
}

impl std::ops::BitOr for Wrapping<i64> {
    type Output = Wrapping<i64>;
    fn bitor(self, o: Wrapping<i64>) -> Wrapping<i64> {
        Wrapping(self.0 | o.0)
    }
}

impl std::ops::BitXor for Wrapping<i64> {
    type Output = Wrapping<i64>;
    fn bitxor(self, o: Wrapping<i64>) -> Wrapping<i64> {
        Wrapping(self.0 ^ o.0)
    }
}

impl std::ops::AddAssign for Wrapping<i64> {
    fn add_assign(&mut self, o: Wrapping<i64>) {
        self.0 = self.0.wrapping_add(o.0);
    }
}

impl std::ops::SubAssign for Wrapping<i64> {
    fn sub_assign(&mut self, o: Wrapping<i64>) {
        self.0 = self.0.wrapping_sub(o.0);
    }
}

impl std::ops::MulAssign for Wrapping<i64> {
    fn mul_assign(&mut self, o: Wrapping<i64>) {
        self.0 = self.0.wrapping_mul(o.0);
    }
}

impl std::ops::DivAssign for Wrapping<i64> {
    fn div_assign(&mut self, o: Wrapping<i64>) {
        self.0 = self.0.wrapping_div(o.0);
    }
}

impl std::ops::RemAssign for Wrapping<i64> {
    fn rem_assign(&mut self, o: Wrapping<i64>) {
        self.0 = self.0.wrapping_rem(o.0);
    }
}

impl std::ops::BitAndAssign for Wrapping<i64> {
    fn bitand_assign(&mut self, o: Wrapping<i64>) {
        self.0 = self.0 & o.0;
    }
}

impl std::ops::BitOrAssign for Wrapping<i64> {
    fn bitor_assign(&mut self, o: Wrapping<i64>) {
        self.0 = self.0 | o.0;
    }
}

impl std::ops::BitXorAssign for Wrapping<i64> {
    fn bitxor_assign(&mut self, o: Wrapping<i64>) {
        self.0 = self.0 ^ o.0;
    }
}

impl std::ops::Not for Wrapping<i64> {
    type Output = Wrapping<i64>;
    fn not(self) -> Wrapping<i64> {
        Wrapping(!self.0)
    }
}

impl std::ops::Neg for Wrapping<i64> {
    type Output = Wrapping<i64>;
    fn neg(self) -> Wrapping<i64> {
        Wrapping(self.0.wrapping_neg())
    }
}

impl std::ops::Add for Wrapping<isize> {
    type Output = Wrapping<isize>;
    fn add(self, o: Wrapping<isize>) -> Wrapping<isize> {
        Wrapping(self.0.wrapping_add(o.0))
    }
}

impl std::ops::Sub for Wrapping<isize> {
    type Output = Wrapping<isize>;
    fn sub(self, o: Wrapping<isize>) -> Wrapping<isize> {
        Wrapping(self.0.wrapping_sub(o.0))
    }
}

impl std::ops::Mul for Wrapping<isize> {
    type Output = Wrapping<isize>;
    fn mul(self, o: Wrapping<isize>) -> Wrapping<isize> {
        Wrapping(self.0.wrapping_mul(o.0))
    }
}

impl std::ops::Div for Wrapping<isize> {
    type Output = Wrapping<isize>;
    fn div(self, o: Wrapping<isize>) -> Wrapping<isize> {
        Wrapping(self.0.wrapping_div(o.0))
    }
}

impl std::ops::Rem for Wrapping<isize> {
    type Output = Wrapping<isize>;
    fn rem(self, o: Wrapping<isize>) -> Wrapping<isize> {
        Wrapping(self.0.wrapping_rem(o.0))
    }
}

impl std::ops::BitAnd for Wrapping<isize> {
    type Output = Wrapping<isize>;
    fn bitand(self, o: Wrapping<isize>) -> Wrapping<isize> {
        Wrapping(self.0 & o.0)
    }
}

impl std::ops::BitOr for Wrapping<isize> {
    type Output = Wrapping<isize>;
    fn bitor(self, o: Wrapping<isize>) -> Wrapping<isize> {
        Wrapping(self.0 | o.0)
    }
}

impl std::ops::BitXor for Wrapping<isize> {
    type Output = Wrapping<isize>;
    fn bitxor(self, o: Wrapping<isize>) -> Wrapping<isize> {
        Wrapping(self.0 ^ o.0)
    }
}

impl std::ops::AddAssign for Wrapping<isize> {
    fn add_assign(&mut self, o: Wrapping<isize>) {
        self.0 = self.0.wrapping_add(o.0);
    }
}

impl std::ops::SubAssign for Wrapping<isize> {
    fn sub_assign(&mut self, o: Wrapping<isize>) {
        self.0 = self.0.wrapping_sub(o.0);
    }
}

impl std::ops::MulAssign for Wrapping<isize> {
    fn mul_assign(&mut self, o: Wrapping<isize>) {
        self.0 = self.0.wrapping_mul(o.0);
    }
}

impl std::ops::DivAssign for Wrapping<isize> {
    fn div_assign(&mut self, o: Wrapping<isize>) {
        self.0 = self.0.wrapping_div(o.0);
    }
}

impl std::ops::RemAssign for Wrapping<isize> {
    fn rem_assign(&mut self, o: Wrapping<isize>) {
        self.0 = self.0.wrapping_rem(o.0);
    }
}

impl std::ops::BitAndAssign for Wrapping<isize> {
    fn bitand_assign(&mut self, o: Wrapping<isize>) {
        self.0 = self.0 & o.0;
    }
}

impl std::ops::BitOrAssign for Wrapping<isize> {
    fn bitor_assign(&mut self, o: Wrapping<isize>) {
        self.0 = self.0 | o.0;
    }
}

impl std::ops::BitXorAssign for Wrapping<isize> {
    fn bitxor_assign(&mut self, o: Wrapping<isize>) {
        self.0 = self.0 ^ o.0;
    }
}

impl std::ops::Not for Wrapping<isize> {
    type Output = Wrapping<isize>;
    fn not(self) -> Wrapping<isize> {
        Wrapping(!self.0)
    }
}

impl std::ops::Neg for Wrapping<isize> {
    type Output = Wrapping<isize>;
    fn neg(self) -> Wrapping<isize> {
        Wrapping(self.0.wrapping_neg())
    }
}
