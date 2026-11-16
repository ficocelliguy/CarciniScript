// (num prelude)
// The bundled `num` / `num-traits` / `num-integer`: the `Integer` methods (`gcd`, `lcm`,
// `div_floor`, `mod_floor`, `is_even`, ..) and `gcd` / `lcm` functions, `Zero` / `One`,
// `Signed`, `Bounded`, `ToPrimitive` / `FromPrimitive`, and `pow` / `abs` / `clamp`, for the
// integer and float types up to 64 bits (and as bounds on your generic functions).

/// `num::Integer`.
pub trait NumInteger: Sized {
    fn gcd(&self, other: &Self) -> Self;
    fn lcm(&self, other: &Self) -> Self;
    fn div_floor(&self, other: &Self) -> Self;
    fn mod_floor(&self, other: &Self) -> Self;
    fn is_even(&self) -> bool;
    fn is_odd(&self) -> bool;
    fn is_multiple_of(&self, other: &Self) -> bool;
    fn div_rem(&self, other: &Self) -> (Self, Self);
}

/// `num::Zero`.
pub trait NumZero: Sized {
    fn zero() -> Self;
    fn is_zero(&self) -> bool;
}

/// `num::One`.
pub trait NumOne: Sized {
    fn one() -> Self;
    fn is_one(&self) -> bool;
}

/// `num::Signed`.
pub trait NumSigned: Sized {
    fn abs(&self) -> Self;
    fn signum(&self) -> Self;
    fn is_positive(&self) -> bool;
    fn is_negative(&self) -> bool;
    fn abs_sub(&self, other: &Self) -> Self;
}

/// `num::Bounded`.
pub trait NumBounded: Sized {
    fn min_value() -> Self;
    fn max_value() -> Self;
}

/// `num::ToPrimitive`.
pub trait NumToPrimitive {
    fn to_i64(&self) -> Option<i64>;
    fn to_u64(&self) -> Option<u64>;
    fn to_i32(&self) -> Option<i32>;
    fn to_u32(&self) -> Option<u32>;
    fn to_usize(&self) -> Option<usize>;
    fn to_isize(&self) -> Option<isize>;
    fn to_u8(&self) -> Option<u8>;
    fn to_i8(&self) -> Option<i8>;
    fn to_u16(&self) -> Option<u16>;
    fn to_i16(&self) -> Option<i16>;
    fn to_f64(&self) -> Option<f64>;
    fn to_f32(&self) -> Option<f32>;
}

/// `num::FromPrimitive`.
pub trait NumFromPrimitive: Sized {
    fn from_i64(n: i64) -> Option<Self>;
    fn from_u64(n: u64) -> Option<Self>;
    fn from_f64(n: f64) -> Option<Self>;
    fn from_usize(n: usize) -> Option<Self>;
    fn from_i32(n: i32) -> Option<Self>;
    fn from_u32(n: u32) -> Option<Self>;
}

impl NumInteger for i8 {
    fn gcd(&self, other: &i8) -> i8 {
        let mut a = *self;
        let mut b = *other;
        while b != 0 {
            let r = a % b;
            a = b;
            b = r;
        }
        let x = a;
        x.abs()
    }
    fn lcm(&self, other: &i8) -> i8 {
        if *self == 0 && *other == 0 {
            return 0;
        }
        let g = self.gcd(other);
        let x = *self * (*other / g);
        x.abs()
    }
    fn div_floor(&self, other: &i8) -> i8 {
        let d = *self / *other;
        let r = *self % *other;
        if r != 0 && ((r < 0) != (*other < 0)) {
            d - 1
        } else {
            d
        }
    }
    fn mod_floor(&self, other: &i8) -> i8 {
        let r = *self % *other;
        if r != 0 && ((r < 0) != (*other < 0)) {
            r + *other
        } else {
            r
        }
    }
    fn is_even(&self) -> bool {
        *self % 2 == 0
    }
    fn is_odd(&self) -> bool {
        *self % 2 != 0
    }
    fn is_multiple_of(&self, other: &i8) -> bool {
        if *other == 0 {
            return *self == 0;
        }
        *self % *other == 0
    }
    fn div_rem(&self, other: &i8) -> (i8, i8) {
        (*self / *other, *self % *other)
    }
}

impl NumInteger for i16 {
    fn gcd(&self, other: &i16) -> i16 {
        let mut a = *self;
        let mut b = *other;
        while b != 0 {
            let r = a % b;
            a = b;
            b = r;
        }
        let x = a;
        x.abs()
    }
    fn lcm(&self, other: &i16) -> i16 {
        if *self == 0 && *other == 0 {
            return 0;
        }
        let g = self.gcd(other);
        let x = *self * (*other / g);
        x.abs()
    }
    fn div_floor(&self, other: &i16) -> i16 {
        let d = *self / *other;
        let r = *self % *other;
        if r != 0 && ((r < 0) != (*other < 0)) {
            d - 1
        } else {
            d
        }
    }
    fn mod_floor(&self, other: &i16) -> i16 {
        let r = *self % *other;
        if r != 0 && ((r < 0) != (*other < 0)) {
            r + *other
        } else {
            r
        }
    }
    fn is_even(&self) -> bool {
        *self % 2 == 0
    }
    fn is_odd(&self) -> bool {
        *self % 2 != 0
    }
    fn is_multiple_of(&self, other: &i16) -> bool {
        if *other == 0 {
            return *self == 0;
        }
        *self % *other == 0
    }
    fn div_rem(&self, other: &i16) -> (i16, i16) {
        (*self / *other, *self % *other)
    }
}

impl NumInteger for i32 {
    fn gcd(&self, other: &i32) -> i32 {
        let mut a = *self;
        let mut b = *other;
        while b != 0 {
            let r = a % b;
            a = b;
            b = r;
        }
        let x = a;
        x.abs()
    }
    fn lcm(&self, other: &i32) -> i32 {
        if *self == 0 && *other == 0 {
            return 0;
        }
        let g = self.gcd(other);
        let x = *self * (*other / g);
        x.abs()
    }
    fn div_floor(&self, other: &i32) -> i32 {
        let d = *self / *other;
        let r = *self % *other;
        if r != 0 && ((r < 0) != (*other < 0)) {
            d - 1
        } else {
            d
        }
    }
    fn mod_floor(&self, other: &i32) -> i32 {
        let r = *self % *other;
        if r != 0 && ((r < 0) != (*other < 0)) {
            r + *other
        } else {
            r
        }
    }
    fn is_even(&self) -> bool {
        *self % 2 == 0
    }
    fn is_odd(&self) -> bool {
        *self % 2 != 0
    }
    fn is_multiple_of(&self, other: &i32) -> bool {
        if *other == 0 {
            return *self == 0;
        }
        *self % *other == 0
    }
    fn div_rem(&self, other: &i32) -> (i32, i32) {
        (*self / *other, *self % *other)
    }
}

impl NumInteger for i64 {
    fn gcd(&self, other: &i64) -> i64 {
        let mut a = *self;
        let mut b = *other;
        while b != 0 {
            let r = a % b;
            a = b;
            b = r;
        }
        let x = a;
        x.abs()
    }
    fn lcm(&self, other: &i64) -> i64 {
        if *self == 0 && *other == 0 {
            return 0;
        }
        let g = self.gcd(other);
        let x = *self * (*other / g);
        x.abs()
    }
    fn div_floor(&self, other: &i64) -> i64 {
        let d = *self / *other;
        let r = *self % *other;
        if r != 0 && ((r < 0) != (*other < 0)) {
            d - 1
        } else {
            d
        }
    }
    fn mod_floor(&self, other: &i64) -> i64 {
        let r = *self % *other;
        if r != 0 && ((r < 0) != (*other < 0)) {
            r + *other
        } else {
            r
        }
    }
    fn is_even(&self) -> bool {
        *self % 2 == 0
    }
    fn is_odd(&self) -> bool {
        *self % 2 != 0
    }
    fn is_multiple_of(&self, other: &i64) -> bool {
        if *other == 0 {
            return *self == 0;
        }
        *self % *other == 0
    }
    fn div_rem(&self, other: &i64) -> (i64, i64) {
        (*self / *other, *self % *other)
    }
}

impl NumInteger for isize {
    fn gcd(&self, other: &isize) -> isize {
        let mut a = *self;
        let mut b = *other;
        while b != 0 {
            let r = a % b;
            a = b;
            b = r;
        }
        let x = a;
        x.abs()
    }
    fn lcm(&self, other: &isize) -> isize {
        if *self == 0 && *other == 0 {
            return 0;
        }
        let g = self.gcd(other);
        let x = *self * (*other / g);
        x.abs()
    }
    fn div_floor(&self, other: &isize) -> isize {
        let d = *self / *other;
        let r = *self % *other;
        if r != 0 && ((r < 0) != (*other < 0)) {
            d - 1
        } else {
            d
        }
    }
    fn mod_floor(&self, other: &isize) -> isize {
        let r = *self % *other;
        if r != 0 && ((r < 0) != (*other < 0)) {
            r + *other
        } else {
            r
        }
    }
    fn is_even(&self) -> bool {
        *self % 2 == 0
    }
    fn is_odd(&self) -> bool {
        *self % 2 != 0
    }
    fn is_multiple_of(&self, other: &isize) -> bool {
        if *other == 0 {
            return *self == 0;
        }
        *self % *other == 0
    }
    fn div_rem(&self, other: &isize) -> (isize, isize) {
        (*self / *other, *self % *other)
    }
}

impl NumInteger for u8 {
    fn gcd(&self, other: &u8) -> u8 {
        let mut a = *self;
        let mut b = *other;
        while b != 0 {
            let r = a % b;
            a = b;
            b = r;
        }
        let x = a;
        x
    }
    fn lcm(&self, other: &u8) -> u8 {
        if *self == 0 && *other == 0 {
            return 0;
        }
        let g = self.gcd(other);
        let x = *self * (*other / g);
        x
    }
    fn div_floor(&self, other: &u8) -> u8 {
        let d = *self / *other;
        let r = *self % *other;
        if r != 0 && ((r < 0) != (*other < 0)) {
            d - 1
        } else {
            d
        }
    }
    fn mod_floor(&self, other: &u8) -> u8 {
        let r = *self % *other;
        if r != 0 && ((r < 0) != (*other < 0)) {
            r + *other
        } else {
            r
        }
    }
    fn is_even(&self) -> bool {
        *self % 2 == 0
    }
    fn is_odd(&self) -> bool {
        *self % 2 != 0
    }
    fn is_multiple_of(&self, other: &u8) -> bool {
        if *other == 0 {
            return *self == 0;
        }
        *self % *other == 0
    }
    fn div_rem(&self, other: &u8) -> (u8, u8) {
        (*self / *other, *self % *other)
    }
}

impl NumInteger for u16 {
    fn gcd(&self, other: &u16) -> u16 {
        let mut a = *self;
        let mut b = *other;
        while b != 0 {
            let r = a % b;
            a = b;
            b = r;
        }
        let x = a;
        x
    }
    fn lcm(&self, other: &u16) -> u16 {
        if *self == 0 && *other == 0 {
            return 0;
        }
        let g = self.gcd(other);
        let x = *self * (*other / g);
        x
    }
    fn div_floor(&self, other: &u16) -> u16 {
        let d = *self / *other;
        let r = *self % *other;
        if r != 0 && ((r < 0) != (*other < 0)) {
            d - 1
        } else {
            d
        }
    }
    fn mod_floor(&self, other: &u16) -> u16 {
        let r = *self % *other;
        if r != 0 && ((r < 0) != (*other < 0)) {
            r + *other
        } else {
            r
        }
    }
    fn is_even(&self) -> bool {
        *self % 2 == 0
    }
    fn is_odd(&self) -> bool {
        *self % 2 != 0
    }
    fn is_multiple_of(&self, other: &u16) -> bool {
        if *other == 0 {
            return *self == 0;
        }
        *self % *other == 0
    }
    fn div_rem(&self, other: &u16) -> (u16, u16) {
        (*self / *other, *self % *other)
    }
}

impl NumInteger for u32 {
    fn gcd(&self, other: &u32) -> u32 {
        let mut a = *self;
        let mut b = *other;
        while b != 0 {
            let r = a % b;
            a = b;
            b = r;
        }
        let x = a;
        x
    }
    fn lcm(&self, other: &u32) -> u32 {
        if *self == 0 && *other == 0 {
            return 0;
        }
        let g = self.gcd(other);
        let x = *self * (*other / g);
        x
    }
    fn div_floor(&self, other: &u32) -> u32 {
        let d = *self / *other;
        let r = *self % *other;
        if r != 0 && ((r < 0) != (*other < 0)) {
            d - 1
        } else {
            d
        }
    }
    fn mod_floor(&self, other: &u32) -> u32 {
        let r = *self % *other;
        if r != 0 && ((r < 0) != (*other < 0)) {
            r + *other
        } else {
            r
        }
    }
    fn is_even(&self) -> bool {
        *self % 2 == 0
    }
    fn is_odd(&self) -> bool {
        *self % 2 != 0
    }
    fn is_multiple_of(&self, other: &u32) -> bool {
        if *other == 0 {
            return *self == 0;
        }
        *self % *other == 0
    }
    fn div_rem(&self, other: &u32) -> (u32, u32) {
        (*self / *other, *self % *other)
    }
}

impl NumInteger for u64 {
    fn gcd(&self, other: &u64) -> u64 {
        let mut a = *self;
        let mut b = *other;
        while b != 0 {
            let r = a % b;
            a = b;
            b = r;
        }
        let x = a;
        x
    }
    fn lcm(&self, other: &u64) -> u64 {
        if *self == 0 && *other == 0 {
            return 0;
        }
        let g = self.gcd(other);
        let x = *self * (*other / g);
        x
    }
    fn div_floor(&self, other: &u64) -> u64 {
        let d = *self / *other;
        let r = *self % *other;
        if r != 0 && ((r < 0) != (*other < 0)) {
            d - 1
        } else {
            d
        }
    }
    fn mod_floor(&self, other: &u64) -> u64 {
        let r = *self % *other;
        if r != 0 && ((r < 0) != (*other < 0)) {
            r + *other
        } else {
            r
        }
    }
    fn is_even(&self) -> bool {
        *self % 2 == 0
    }
    fn is_odd(&self) -> bool {
        *self % 2 != 0
    }
    fn is_multiple_of(&self, other: &u64) -> bool {
        if *other == 0 {
            return *self == 0;
        }
        *self % *other == 0
    }
    fn div_rem(&self, other: &u64) -> (u64, u64) {
        (*self / *other, *self % *other)
    }
}

impl NumInteger for usize {
    fn gcd(&self, other: &usize) -> usize {
        let mut a = *self;
        let mut b = *other;
        while b != 0 {
            let r = a % b;
            a = b;
            b = r;
        }
        let x = a;
        x
    }
    fn lcm(&self, other: &usize) -> usize {
        if *self == 0 && *other == 0 {
            return 0;
        }
        let g = self.gcd(other);
        let x = *self * (*other / g);
        x
    }
    fn div_floor(&self, other: &usize) -> usize {
        let d = *self / *other;
        let r = *self % *other;
        if r != 0 && ((r < 0) != (*other < 0)) {
            d - 1
        } else {
            d
        }
    }
    fn mod_floor(&self, other: &usize) -> usize {
        let r = *self % *other;
        if r != 0 && ((r < 0) != (*other < 0)) {
            r + *other
        } else {
            r
        }
    }
    fn is_even(&self) -> bool {
        *self % 2 == 0
    }
    fn is_odd(&self) -> bool {
        *self % 2 != 0
    }
    fn is_multiple_of(&self, other: &usize) -> bool {
        if *other == 0 {
            return *self == 0;
        }
        *self % *other == 0
    }
    fn div_rem(&self, other: &usize) -> (usize, usize) {
        (*self / *other, *self % *other)
    }
}

impl NumZero for i8 {
    fn zero() -> i8 {
        0
    }
    fn is_zero(&self) -> bool {
        *self == 0
    }
}

impl NumOne for i8 {
    fn one() -> i8 {
        1
    }
    fn is_one(&self) -> bool {
        *self == 1
    }
}

impl NumBounded for i8 {
    fn min_value() -> i8 {
        i8::MIN
    }
    fn max_value() -> i8 {
        i8::MAX
    }
}

impl NumToPrimitive for i8 {
    fn to_i64(&self) -> Option<i64> {
        i64::try_from(*self as i128).ok()
    }
    fn to_u64(&self) -> Option<u64> {
        u64::try_from(*self as i128).ok()
    }
    fn to_i32(&self) -> Option<i32> {
        i32::try_from(*self as i128).ok()
    }
    fn to_u32(&self) -> Option<u32> {
        u32::try_from(*self as i128).ok()
    }
    fn to_usize(&self) -> Option<usize> {
        usize::try_from(*self as i128).ok()
    }
    fn to_isize(&self) -> Option<isize> {
        isize::try_from(*self as i128).ok()
    }
    fn to_u8(&self) -> Option<u8> {
        u8::try_from(*self as i128).ok()
    }
    fn to_i8(&self) -> Option<i8> {
        i8::try_from(*self as i128).ok()
    }
    fn to_u16(&self) -> Option<u16> {
        u16::try_from(*self as i128).ok()
    }
    fn to_i16(&self) -> Option<i16> {
        i16::try_from(*self as i128).ok()
    }
    fn to_f64(&self) -> Option<f64> {
        Some(*self as f64)
    }
    fn to_f32(&self) -> Option<f32> {
        Some(*self as f32)
    }
}

impl NumFromPrimitive for i8 {
    fn from_i64(n: i64) -> Option<i8> {
        i8::try_from(n).ok()
    }
    fn from_u64(n: u64) -> Option<i8> {
        i8::try_from(n).ok()
    }
    fn from_f64(n: f64) -> Option<i8> {
        if n.is_nan() || n < i8::MIN as f64 || n >= (i8::MAX as f64) + 1.0 {
            None
        } else {
            Some(n as i8)
        }
    }
    fn from_usize(n: usize) -> Option<i8> {
        i8::try_from(n).ok()
    }
    fn from_i32(n: i32) -> Option<i8> {
        i8::try_from(n).ok()
    }
    fn from_u32(n: u32) -> Option<i8> {
        i8::try_from(n).ok()
    }
}

impl NumZero for i16 {
    fn zero() -> i16 {
        0
    }
    fn is_zero(&self) -> bool {
        *self == 0
    }
}

impl NumOne for i16 {
    fn one() -> i16 {
        1
    }
    fn is_one(&self) -> bool {
        *self == 1
    }
}

impl NumBounded for i16 {
    fn min_value() -> i16 {
        i16::MIN
    }
    fn max_value() -> i16 {
        i16::MAX
    }
}

impl NumToPrimitive for i16 {
    fn to_i64(&self) -> Option<i64> {
        i64::try_from(*self as i128).ok()
    }
    fn to_u64(&self) -> Option<u64> {
        u64::try_from(*self as i128).ok()
    }
    fn to_i32(&self) -> Option<i32> {
        i32::try_from(*self as i128).ok()
    }
    fn to_u32(&self) -> Option<u32> {
        u32::try_from(*self as i128).ok()
    }
    fn to_usize(&self) -> Option<usize> {
        usize::try_from(*self as i128).ok()
    }
    fn to_isize(&self) -> Option<isize> {
        isize::try_from(*self as i128).ok()
    }
    fn to_u8(&self) -> Option<u8> {
        u8::try_from(*self as i128).ok()
    }
    fn to_i8(&self) -> Option<i8> {
        i8::try_from(*self as i128).ok()
    }
    fn to_u16(&self) -> Option<u16> {
        u16::try_from(*self as i128).ok()
    }
    fn to_i16(&self) -> Option<i16> {
        i16::try_from(*self as i128).ok()
    }
    fn to_f64(&self) -> Option<f64> {
        Some(*self as f64)
    }
    fn to_f32(&self) -> Option<f32> {
        Some(*self as f32)
    }
}

impl NumFromPrimitive for i16 {
    fn from_i64(n: i64) -> Option<i16> {
        i16::try_from(n).ok()
    }
    fn from_u64(n: u64) -> Option<i16> {
        i16::try_from(n).ok()
    }
    fn from_f64(n: f64) -> Option<i16> {
        if n.is_nan() || n < i16::MIN as f64 || n >= (i16::MAX as f64) + 1.0 {
            None
        } else {
            Some(n as i16)
        }
    }
    fn from_usize(n: usize) -> Option<i16> {
        i16::try_from(n).ok()
    }
    fn from_i32(n: i32) -> Option<i16> {
        i16::try_from(n).ok()
    }
    fn from_u32(n: u32) -> Option<i16> {
        i16::try_from(n).ok()
    }
}

impl NumZero for i32 {
    fn zero() -> i32 {
        0
    }
    fn is_zero(&self) -> bool {
        *self == 0
    }
}

impl NumOne for i32 {
    fn one() -> i32 {
        1
    }
    fn is_one(&self) -> bool {
        *self == 1
    }
}

impl NumBounded for i32 {
    fn min_value() -> i32 {
        i32::MIN
    }
    fn max_value() -> i32 {
        i32::MAX
    }
}

impl NumToPrimitive for i32 {
    fn to_i64(&self) -> Option<i64> {
        i64::try_from(*self as i128).ok()
    }
    fn to_u64(&self) -> Option<u64> {
        u64::try_from(*self as i128).ok()
    }
    fn to_i32(&self) -> Option<i32> {
        i32::try_from(*self as i128).ok()
    }
    fn to_u32(&self) -> Option<u32> {
        u32::try_from(*self as i128).ok()
    }
    fn to_usize(&self) -> Option<usize> {
        usize::try_from(*self as i128).ok()
    }
    fn to_isize(&self) -> Option<isize> {
        isize::try_from(*self as i128).ok()
    }
    fn to_u8(&self) -> Option<u8> {
        u8::try_from(*self as i128).ok()
    }
    fn to_i8(&self) -> Option<i8> {
        i8::try_from(*self as i128).ok()
    }
    fn to_u16(&self) -> Option<u16> {
        u16::try_from(*self as i128).ok()
    }
    fn to_i16(&self) -> Option<i16> {
        i16::try_from(*self as i128).ok()
    }
    fn to_f64(&self) -> Option<f64> {
        Some(*self as f64)
    }
    fn to_f32(&self) -> Option<f32> {
        Some(*self as f32)
    }
}

impl NumFromPrimitive for i32 {
    fn from_i64(n: i64) -> Option<i32> {
        i32::try_from(n).ok()
    }
    fn from_u64(n: u64) -> Option<i32> {
        i32::try_from(n).ok()
    }
    fn from_f64(n: f64) -> Option<i32> {
        if n.is_nan() || n < i32::MIN as f64 || n >= (i32::MAX as f64) + 1.0 {
            None
        } else {
            Some(n as i32)
        }
    }
    fn from_usize(n: usize) -> Option<i32> {
        i32::try_from(n).ok()
    }
    fn from_i32(n: i32) -> Option<i32> {
        i32::try_from(n).ok()
    }
    fn from_u32(n: u32) -> Option<i32> {
        i32::try_from(n).ok()
    }
}

impl NumZero for i64 {
    fn zero() -> i64 {
        0
    }
    fn is_zero(&self) -> bool {
        *self == 0
    }
}

impl NumOne for i64 {
    fn one() -> i64 {
        1
    }
    fn is_one(&self) -> bool {
        *self == 1
    }
}

impl NumBounded for i64 {
    fn min_value() -> i64 {
        i64::MIN
    }
    fn max_value() -> i64 {
        i64::MAX
    }
}

impl NumToPrimitive for i64 {
    fn to_i64(&self) -> Option<i64> {
        i64::try_from(*self as i128).ok()
    }
    fn to_u64(&self) -> Option<u64> {
        u64::try_from(*self as i128).ok()
    }
    fn to_i32(&self) -> Option<i32> {
        i32::try_from(*self as i128).ok()
    }
    fn to_u32(&self) -> Option<u32> {
        u32::try_from(*self as i128).ok()
    }
    fn to_usize(&self) -> Option<usize> {
        usize::try_from(*self as i128).ok()
    }
    fn to_isize(&self) -> Option<isize> {
        isize::try_from(*self as i128).ok()
    }
    fn to_u8(&self) -> Option<u8> {
        u8::try_from(*self as i128).ok()
    }
    fn to_i8(&self) -> Option<i8> {
        i8::try_from(*self as i128).ok()
    }
    fn to_u16(&self) -> Option<u16> {
        u16::try_from(*self as i128).ok()
    }
    fn to_i16(&self) -> Option<i16> {
        i16::try_from(*self as i128).ok()
    }
    fn to_f64(&self) -> Option<f64> {
        Some(*self as f64)
    }
    fn to_f32(&self) -> Option<f32> {
        Some(*self as f32)
    }
}

impl NumFromPrimitive for i64 {
    fn from_i64(n: i64) -> Option<i64> {
        i64::try_from(n).ok()
    }
    fn from_u64(n: u64) -> Option<i64> {
        i64::try_from(n).ok()
    }
    fn from_f64(n: f64) -> Option<i64> {
        if n.is_nan() || n < i64::MIN as f64 || n >= (i64::MAX as f64) + 1.0 {
            None
        } else {
            Some(n as i64)
        }
    }
    fn from_usize(n: usize) -> Option<i64> {
        i64::try_from(n).ok()
    }
    fn from_i32(n: i32) -> Option<i64> {
        i64::try_from(n).ok()
    }
    fn from_u32(n: u32) -> Option<i64> {
        i64::try_from(n).ok()
    }
}

impl NumZero for isize {
    fn zero() -> isize {
        0
    }
    fn is_zero(&self) -> bool {
        *self == 0
    }
}

impl NumOne for isize {
    fn one() -> isize {
        1
    }
    fn is_one(&self) -> bool {
        *self == 1
    }
}

impl NumBounded for isize {
    fn min_value() -> isize {
        isize::MIN
    }
    fn max_value() -> isize {
        isize::MAX
    }
}

impl NumToPrimitive for isize {
    fn to_i64(&self) -> Option<i64> {
        i64::try_from(*self as i128).ok()
    }
    fn to_u64(&self) -> Option<u64> {
        u64::try_from(*self as i128).ok()
    }
    fn to_i32(&self) -> Option<i32> {
        i32::try_from(*self as i128).ok()
    }
    fn to_u32(&self) -> Option<u32> {
        u32::try_from(*self as i128).ok()
    }
    fn to_usize(&self) -> Option<usize> {
        usize::try_from(*self as i128).ok()
    }
    fn to_isize(&self) -> Option<isize> {
        isize::try_from(*self as i128).ok()
    }
    fn to_u8(&self) -> Option<u8> {
        u8::try_from(*self as i128).ok()
    }
    fn to_i8(&self) -> Option<i8> {
        i8::try_from(*self as i128).ok()
    }
    fn to_u16(&self) -> Option<u16> {
        u16::try_from(*self as i128).ok()
    }
    fn to_i16(&self) -> Option<i16> {
        i16::try_from(*self as i128).ok()
    }
    fn to_f64(&self) -> Option<f64> {
        Some(*self as f64)
    }
    fn to_f32(&self) -> Option<f32> {
        Some(*self as f32)
    }
}

impl NumFromPrimitive for isize {
    fn from_i64(n: i64) -> Option<isize> {
        isize::try_from(n).ok()
    }
    fn from_u64(n: u64) -> Option<isize> {
        isize::try_from(n).ok()
    }
    fn from_f64(n: f64) -> Option<isize> {
        if n.is_nan() || n < isize::MIN as f64 || n >= (isize::MAX as f64) + 1.0 {
            None
        } else {
            Some(n as isize)
        }
    }
    fn from_usize(n: usize) -> Option<isize> {
        isize::try_from(n).ok()
    }
    fn from_i32(n: i32) -> Option<isize> {
        isize::try_from(n).ok()
    }
    fn from_u32(n: u32) -> Option<isize> {
        isize::try_from(n).ok()
    }
}

impl NumZero for u8 {
    fn zero() -> u8 {
        0
    }
    fn is_zero(&self) -> bool {
        *self == 0
    }
}

impl NumOne for u8 {
    fn one() -> u8 {
        1
    }
    fn is_one(&self) -> bool {
        *self == 1
    }
}

impl NumBounded for u8 {
    fn min_value() -> u8 {
        u8::MIN
    }
    fn max_value() -> u8 {
        u8::MAX
    }
}

impl NumToPrimitive for u8 {
    fn to_i64(&self) -> Option<i64> {
        i64::try_from(*self as i128).ok()
    }
    fn to_u64(&self) -> Option<u64> {
        u64::try_from(*self as i128).ok()
    }
    fn to_i32(&self) -> Option<i32> {
        i32::try_from(*self as i128).ok()
    }
    fn to_u32(&self) -> Option<u32> {
        u32::try_from(*self as i128).ok()
    }
    fn to_usize(&self) -> Option<usize> {
        usize::try_from(*self as i128).ok()
    }
    fn to_isize(&self) -> Option<isize> {
        isize::try_from(*self as i128).ok()
    }
    fn to_u8(&self) -> Option<u8> {
        u8::try_from(*self as i128).ok()
    }
    fn to_i8(&self) -> Option<i8> {
        i8::try_from(*self as i128).ok()
    }
    fn to_u16(&self) -> Option<u16> {
        u16::try_from(*self as i128).ok()
    }
    fn to_i16(&self) -> Option<i16> {
        i16::try_from(*self as i128).ok()
    }
    fn to_f64(&self) -> Option<f64> {
        Some(*self as f64)
    }
    fn to_f32(&self) -> Option<f32> {
        Some(*self as f32)
    }
}

impl NumFromPrimitive for u8 {
    fn from_i64(n: i64) -> Option<u8> {
        u8::try_from(n).ok()
    }
    fn from_u64(n: u64) -> Option<u8> {
        u8::try_from(n).ok()
    }
    fn from_f64(n: f64) -> Option<u8> {
        if n.is_nan() || n < u8::MIN as f64 || n >= (u8::MAX as f64) + 1.0 {
            None
        } else {
            Some(n as u8)
        }
    }
    fn from_usize(n: usize) -> Option<u8> {
        u8::try_from(n).ok()
    }
    fn from_i32(n: i32) -> Option<u8> {
        u8::try_from(n).ok()
    }
    fn from_u32(n: u32) -> Option<u8> {
        u8::try_from(n).ok()
    }
}

impl NumZero for u16 {
    fn zero() -> u16 {
        0
    }
    fn is_zero(&self) -> bool {
        *self == 0
    }
}

impl NumOne for u16 {
    fn one() -> u16 {
        1
    }
    fn is_one(&self) -> bool {
        *self == 1
    }
}

impl NumBounded for u16 {
    fn min_value() -> u16 {
        u16::MIN
    }
    fn max_value() -> u16 {
        u16::MAX
    }
}

impl NumToPrimitive for u16 {
    fn to_i64(&self) -> Option<i64> {
        i64::try_from(*self as i128).ok()
    }
    fn to_u64(&self) -> Option<u64> {
        u64::try_from(*self as i128).ok()
    }
    fn to_i32(&self) -> Option<i32> {
        i32::try_from(*self as i128).ok()
    }
    fn to_u32(&self) -> Option<u32> {
        u32::try_from(*self as i128).ok()
    }
    fn to_usize(&self) -> Option<usize> {
        usize::try_from(*self as i128).ok()
    }
    fn to_isize(&self) -> Option<isize> {
        isize::try_from(*self as i128).ok()
    }
    fn to_u8(&self) -> Option<u8> {
        u8::try_from(*self as i128).ok()
    }
    fn to_i8(&self) -> Option<i8> {
        i8::try_from(*self as i128).ok()
    }
    fn to_u16(&self) -> Option<u16> {
        u16::try_from(*self as i128).ok()
    }
    fn to_i16(&self) -> Option<i16> {
        i16::try_from(*self as i128).ok()
    }
    fn to_f64(&self) -> Option<f64> {
        Some(*self as f64)
    }
    fn to_f32(&self) -> Option<f32> {
        Some(*self as f32)
    }
}

impl NumFromPrimitive for u16 {
    fn from_i64(n: i64) -> Option<u16> {
        u16::try_from(n).ok()
    }
    fn from_u64(n: u64) -> Option<u16> {
        u16::try_from(n).ok()
    }
    fn from_f64(n: f64) -> Option<u16> {
        if n.is_nan() || n < u16::MIN as f64 || n >= (u16::MAX as f64) + 1.0 {
            None
        } else {
            Some(n as u16)
        }
    }
    fn from_usize(n: usize) -> Option<u16> {
        u16::try_from(n).ok()
    }
    fn from_i32(n: i32) -> Option<u16> {
        u16::try_from(n).ok()
    }
    fn from_u32(n: u32) -> Option<u16> {
        u16::try_from(n).ok()
    }
}

impl NumZero for u32 {
    fn zero() -> u32 {
        0
    }
    fn is_zero(&self) -> bool {
        *self == 0
    }
}

impl NumOne for u32 {
    fn one() -> u32 {
        1
    }
    fn is_one(&self) -> bool {
        *self == 1
    }
}

impl NumBounded for u32 {
    fn min_value() -> u32 {
        u32::MIN
    }
    fn max_value() -> u32 {
        u32::MAX
    }
}

impl NumToPrimitive for u32 {
    fn to_i64(&self) -> Option<i64> {
        i64::try_from(*self as i128).ok()
    }
    fn to_u64(&self) -> Option<u64> {
        u64::try_from(*self as i128).ok()
    }
    fn to_i32(&self) -> Option<i32> {
        i32::try_from(*self as i128).ok()
    }
    fn to_u32(&self) -> Option<u32> {
        u32::try_from(*self as i128).ok()
    }
    fn to_usize(&self) -> Option<usize> {
        usize::try_from(*self as i128).ok()
    }
    fn to_isize(&self) -> Option<isize> {
        isize::try_from(*self as i128).ok()
    }
    fn to_u8(&self) -> Option<u8> {
        u8::try_from(*self as i128).ok()
    }
    fn to_i8(&self) -> Option<i8> {
        i8::try_from(*self as i128).ok()
    }
    fn to_u16(&self) -> Option<u16> {
        u16::try_from(*self as i128).ok()
    }
    fn to_i16(&self) -> Option<i16> {
        i16::try_from(*self as i128).ok()
    }
    fn to_f64(&self) -> Option<f64> {
        Some(*self as f64)
    }
    fn to_f32(&self) -> Option<f32> {
        Some(*self as f32)
    }
}

impl NumFromPrimitive for u32 {
    fn from_i64(n: i64) -> Option<u32> {
        u32::try_from(n).ok()
    }
    fn from_u64(n: u64) -> Option<u32> {
        u32::try_from(n).ok()
    }
    fn from_f64(n: f64) -> Option<u32> {
        if n.is_nan() || n < u32::MIN as f64 || n >= (u32::MAX as f64) + 1.0 {
            None
        } else {
            Some(n as u32)
        }
    }
    fn from_usize(n: usize) -> Option<u32> {
        u32::try_from(n).ok()
    }
    fn from_i32(n: i32) -> Option<u32> {
        u32::try_from(n).ok()
    }
    fn from_u32(n: u32) -> Option<u32> {
        u32::try_from(n).ok()
    }
}

impl NumZero for u64 {
    fn zero() -> u64 {
        0
    }
    fn is_zero(&self) -> bool {
        *self == 0
    }
}

impl NumOne for u64 {
    fn one() -> u64 {
        1
    }
    fn is_one(&self) -> bool {
        *self == 1
    }
}

impl NumBounded for u64 {
    fn min_value() -> u64 {
        u64::MIN
    }
    fn max_value() -> u64 {
        u64::MAX
    }
}

impl NumToPrimitive for u64 {
    fn to_i64(&self) -> Option<i64> {
        i64::try_from(*self as i128).ok()
    }
    fn to_u64(&self) -> Option<u64> {
        u64::try_from(*self as i128).ok()
    }
    fn to_i32(&self) -> Option<i32> {
        i32::try_from(*self as i128).ok()
    }
    fn to_u32(&self) -> Option<u32> {
        u32::try_from(*self as i128).ok()
    }
    fn to_usize(&self) -> Option<usize> {
        usize::try_from(*self as i128).ok()
    }
    fn to_isize(&self) -> Option<isize> {
        isize::try_from(*self as i128).ok()
    }
    fn to_u8(&self) -> Option<u8> {
        u8::try_from(*self as i128).ok()
    }
    fn to_i8(&self) -> Option<i8> {
        i8::try_from(*self as i128).ok()
    }
    fn to_u16(&self) -> Option<u16> {
        u16::try_from(*self as i128).ok()
    }
    fn to_i16(&self) -> Option<i16> {
        i16::try_from(*self as i128).ok()
    }
    fn to_f64(&self) -> Option<f64> {
        Some(*self as f64)
    }
    fn to_f32(&self) -> Option<f32> {
        Some(*self as f32)
    }
}

impl NumFromPrimitive for u64 {
    fn from_i64(n: i64) -> Option<u64> {
        u64::try_from(n).ok()
    }
    fn from_u64(n: u64) -> Option<u64> {
        u64::try_from(n).ok()
    }
    fn from_f64(n: f64) -> Option<u64> {
        if n.is_nan() || n < u64::MIN as f64 || n >= (u64::MAX as f64) + 1.0 {
            None
        } else {
            Some(n as u64)
        }
    }
    fn from_usize(n: usize) -> Option<u64> {
        u64::try_from(n).ok()
    }
    fn from_i32(n: i32) -> Option<u64> {
        u64::try_from(n).ok()
    }
    fn from_u32(n: u32) -> Option<u64> {
        u64::try_from(n).ok()
    }
}

impl NumZero for usize {
    fn zero() -> usize {
        0
    }
    fn is_zero(&self) -> bool {
        *self == 0
    }
}

impl NumOne for usize {
    fn one() -> usize {
        1
    }
    fn is_one(&self) -> bool {
        *self == 1
    }
}

impl NumBounded for usize {
    fn min_value() -> usize {
        usize::MIN
    }
    fn max_value() -> usize {
        usize::MAX
    }
}

impl NumToPrimitive for usize {
    fn to_i64(&self) -> Option<i64> {
        i64::try_from(*self as i128).ok()
    }
    fn to_u64(&self) -> Option<u64> {
        u64::try_from(*self as i128).ok()
    }
    fn to_i32(&self) -> Option<i32> {
        i32::try_from(*self as i128).ok()
    }
    fn to_u32(&self) -> Option<u32> {
        u32::try_from(*self as i128).ok()
    }
    fn to_usize(&self) -> Option<usize> {
        usize::try_from(*self as i128).ok()
    }
    fn to_isize(&self) -> Option<isize> {
        isize::try_from(*self as i128).ok()
    }
    fn to_u8(&self) -> Option<u8> {
        u8::try_from(*self as i128).ok()
    }
    fn to_i8(&self) -> Option<i8> {
        i8::try_from(*self as i128).ok()
    }
    fn to_u16(&self) -> Option<u16> {
        u16::try_from(*self as i128).ok()
    }
    fn to_i16(&self) -> Option<i16> {
        i16::try_from(*self as i128).ok()
    }
    fn to_f64(&self) -> Option<f64> {
        Some(*self as f64)
    }
    fn to_f32(&self) -> Option<f32> {
        Some(*self as f32)
    }
}

impl NumFromPrimitive for usize {
    fn from_i64(n: i64) -> Option<usize> {
        usize::try_from(n).ok()
    }
    fn from_u64(n: u64) -> Option<usize> {
        usize::try_from(n).ok()
    }
    fn from_f64(n: f64) -> Option<usize> {
        if n.is_nan() || n < usize::MIN as f64 || n >= (usize::MAX as f64) + 1.0 {
            None
        } else {
            Some(n as usize)
        }
    }
    fn from_usize(n: usize) -> Option<usize> {
        usize::try_from(n).ok()
    }
    fn from_i32(n: i32) -> Option<usize> {
        usize::try_from(n).ok()
    }
    fn from_u32(n: u32) -> Option<usize> {
        usize::try_from(n).ok()
    }
}

impl NumZero for f32 {
    fn zero() -> f32 {
        0.0
    }
    fn is_zero(&self) -> bool {
        *self == 0.0
    }
}

impl NumOne for f32 {
    fn one() -> f32 {
        1.0
    }
    fn is_one(&self) -> bool {
        *self == 1.0
    }
}

impl NumBounded for f32 {
    fn min_value() -> f32 {
        -3.40282347e38
    }
    fn max_value() -> f32 {
        3.40282347e38
    }
}

impl NumToPrimitive for f32 {
    fn to_i64(&self) -> Option<i64> {
        i64::try_from(*self as i128).ok()
    }
    fn to_u64(&self) -> Option<u64> {
        u64::try_from(*self as i128).ok()
    }
    fn to_i32(&self) -> Option<i32> {
        i32::try_from(*self as i128).ok()
    }
    fn to_u32(&self) -> Option<u32> {
        u32::try_from(*self as i128).ok()
    }
    fn to_usize(&self) -> Option<usize> {
        usize::try_from(*self as i128).ok()
    }
    fn to_isize(&self) -> Option<isize> {
        isize::try_from(*self as i128).ok()
    }
    fn to_u8(&self) -> Option<u8> {
        u8::try_from(*self as i128).ok()
    }
    fn to_i8(&self) -> Option<i8> {
        i8::try_from(*self as i128).ok()
    }
    fn to_u16(&self) -> Option<u16> {
        u16::try_from(*self as i128).ok()
    }
    fn to_i16(&self) -> Option<i16> {
        i16::try_from(*self as i128).ok()
    }
    fn to_f64(&self) -> Option<f64> {
        Some(*self as f64)
    }
    fn to_f32(&self) -> Option<f32> {
        Some(*self as f32)
    }
}

impl NumFromPrimitive for f32 {
    fn from_i64(n: i64) -> Option<f32> {
        Some(n as f32)
    }
    fn from_u64(n: u64) -> Option<f32> {
        Some(n as f32)
    }
    fn from_f64(n: f64) -> Option<f32> {
        Some(n as f32)
    }
    fn from_usize(n: usize) -> Option<f32> {
        Some(n as f32)
    }
    fn from_i32(n: i32) -> Option<f32> {
        Some(n as f32)
    }
    fn from_u32(n: u32) -> Option<f32> {
        Some(n as f32)
    }
}

impl NumZero for f64 {
    fn zero() -> f64 {
        0.0
    }
    fn is_zero(&self) -> bool {
        *self == 0.0
    }
}

impl NumOne for f64 {
    fn one() -> f64 {
        1.0
    }
    fn is_one(&self) -> bool {
        *self == 1.0
    }
}

impl NumBounded for f64 {
    fn min_value() -> f64 {
        f64::MIN
    }
    fn max_value() -> f64 {
        f64::MAX
    }
}

impl NumToPrimitive for f64 {
    fn to_i64(&self) -> Option<i64> {
        i64::try_from(*self as i128).ok()
    }
    fn to_u64(&self) -> Option<u64> {
        u64::try_from(*self as i128).ok()
    }
    fn to_i32(&self) -> Option<i32> {
        i32::try_from(*self as i128).ok()
    }
    fn to_u32(&self) -> Option<u32> {
        u32::try_from(*self as i128).ok()
    }
    fn to_usize(&self) -> Option<usize> {
        usize::try_from(*self as i128).ok()
    }
    fn to_isize(&self) -> Option<isize> {
        isize::try_from(*self as i128).ok()
    }
    fn to_u8(&self) -> Option<u8> {
        u8::try_from(*self as i128).ok()
    }
    fn to_i8(&self) -> Option<i8> {
        i8::try_from(*self as i128).ok()
    }
    fn to_u16(&self) -> Option<u16> {
        u16::try_from(*self as i128).ok()
    }
    fn to_i16(&self) -> Option<i16> {
        i16::try_from(*self as i128).ok()
    }
    fn to_f64(&self) -> Option<f64> {
        Some(*self as f64)
    }
    fn to_f32(&self) -> Option<f32> {
        Some(*self as f32)
    }
}

impl NumFromPrimitive for f64 {
    fn from_i64(n: i64) -> Option<f64> {
        Some(n as f64)
    }
    fn from_u64(n: u64) -> Option<f64> {
        Some(n as f64)
    }
    fn from_f64(n: f64) -> Option<f64> {
        Some(n as f64)
    }
    fn from_usize(n: usize) -> Option<f64> {
        Some(n as f64)
    }
    fn from_i32(n: i32) -> Option<f64> {
        Some(n as f64)
    }
    fn from_u32(n: u32) -> Option<f64> {
        Some(n as f64)
    }
}

impl NumSigned for i8 {
    fn abs(&self) -> i8 {
        if *self < 0 { -*self } else { *self }
    }
    fn signum(&self) -> i8 {
        if *self > 0 { 1 } else if *self < 0 { -1 } else { 0 }
    }
    fn is_positive(&self) -> bool {
        *self > 0
    }
    fn is_negative(&self) -> bool {
        *self < 0
    }
    fn abs_sub(&self, other: &i8) -> i8 {
        if *self <= *other { 0 } else { *self - *other }
    }
}

impl NumSigned for i16 {
    fn abs(&self) -> i16 {
        if *self < 0 { -*self } else { *self }
    }
    fn signum(&self) -> i16 {
        if *self > 0 { 1 } else if *self < 0 { -1 } else { 0 }
    }
    fn is_positive(&self) -> bool {
        *self > 0
    }
    fn is_negative(&self) -> bool {
        *self < 0
    }
    fn abs_sub(&self, other: &i16) -> i16 {
        if *self <= *other { 0 } else { *self - *other }
    }
}

impl NumSigned for i32 {
    fn abs(&self) -> i32 {
        if *self < 0 { -*self } else { *self }
    }
    fn signum(&self) -> i32 {
        if *self > 0 { 1 } else if *self < 0 { -1 } else { 0 }
    }
    fn is_positive(&self) -> bool {
        *self > 0
    }
    fn is_negative(&self) -> bool {
        *self < 0
    }
    fn abs_sub(&self, other: &i32) -> i32 {
        if *self <= *other { 0 } else { *self - *other }
    }
}

impl NumSigned for i64 {
    fn abs(&self) -> i64 {
        if *self < 0 { -*self } else { *self }
    }
    fn signum(&self) -> i64 {
        if *self > 0 { 1 } else if *self < 0 { -1 } else { 0 }
    }
    fn is_positive(&self) -> bool {
        *self > 0
    }
    fn is_negative(&self) -> bool {
        *self < 0
    }
    fn abs_sub(&self, other: &i64) -> i64 {
        if *self <= *other { 0 } else { *self - *other }
    }
}

impl NumSigned for isize {
    fn abs(&self) -> isize {
        if *self < 0 { -*self } else { *self }
    }
    fn signum(&self) -> isize {
        if *self > 0 { 1 } else if *self < 0 { -1 } else { 0 }
    }
    fn is_positive(&self) -> bool {
        *self > 0
    }
    fn is_negative(&self) -> bool {
        *self < 0
    }
    fn abs_sub(&self, other: &isize) -> isize {
        if *self <= *other { 0 } else { *self - *other }
    }
}

impl NumSigned for f32 {
    fn abs(&self) -> f32 {
        if *self < 0.0 { -*self } else { *self }
    }
    fn signum(&self) -> f32 {
        self.signum()
    }
    fn is_positive(&self) -> bool {
        *self > 0.0
    }
    fn is_negative(&self) -> bool {
        *self < 0.0
    }
    fn abs_sub(&self, other: &f32) -> f32 {
        if *self <= *other { 0.0 } else { *self - *other }
    }
}

impl NumSigned for f64 {
    fn abs(&self) -> f64 {
        if *self < 0.0 { -*self } else { *self }
    }
    fn signum(&self) -> f64 {
        self.signum()
    }
    fn is_positive(&self) -> bool {
        *self > 0.0
    }
    fn is_negative(&self) -> bool {
        *self < 0.0
    }
    fn abs_sub(&self, other: &f64) -> f64 {
        if *self <= *other { 0.0 } else { *self - *other }
    }
}

/// `num::integer::gcd(a, b)`.
pub fn __num_gcd<T: NumInteger>(a: T, b: T) -> T {
    a.gcd(&b)
}

/// `num::integer::lcm(a, b)`.
pub fn __num_lcm<T: NumInteger>(a: T, b: T) -> T {
    a.lcm(&b)
}

/// `num::zero()`.
pub fn __num_zero<T: NumZero>() -> T {
    T::zero()
}

/// `num::one()`.
pub fn __num_one<T: NumOne>() -> T {
    T::one()
}

/// `num::abs(x)`.
pub fn __num_abs<T: NumSigned>(x: T) -> T {
    x.abs()
}

/// `num::signum(x)`.
pub fn __num_signum<T: NumSigned>(x: T) -> T {
    x.signum()
}

/// `num::pow(base, exp)`: by squaring.
pub fn __num_pow<T: Clone + NumOne + std::ops::Mul<Output = T>>(base: T, exp: usize) -> T {
    let mut result = T::one();
    let mut b = base;
    let mut e = exp;
    while e > 0 {
        if e % 2 == 1 {
            result = result * b.clone();
        }
        e /= 2;
        if e > 0 {
            b = b.clone() * b.clone();
        }
    }
    result
}

/// `num::clamp(x, lo, hi)`.
pub fn __num_clamp<T: PartialOrd>(x: T, lo: T, hi: T) -> T {
    if x < lo {
        lo
    } else if x > hi {
        hi
    } else {
        x
    }
}
