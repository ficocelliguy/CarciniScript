// (std::time prelude)
// What CarciniScript compiles in for `std::time` when a program uses it: `Duration` as std
// defines it (seconds and nanoseconds, the same arithmetic, `{:?}` and panics), `Instant` on
// `performance.now()` and `SystemTime` on `Date.now()`. `u64` seconds are exact up to 2^53.

/// `std::time::Duration`.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Duration {
    __secs: u64,
    __nanos: u32,
}

impl Duration {
    pub const ZERO: Duration = Duration {
        __secs: 0,
        __nanos: 0,
    };
    pub const MAX: Duration = Duration {
        __secs: u64::MAX,
        __nanos: 999_999_999,
    };

    pub fn new(secs: u64, nanos: u32) -> Duration {
        if nanos < 1_000_000_000 {
            return Duration {
                __secs: secs,
                __nanos: nanos,
            };
        }
        let extra = (nanos / 1_000_000_000) as u64;
        let secs = secs.checked_add(extra).expect("overflow in Duration::new");
        Duration {
            __secs: secs,
            __nanos: nanos % 1_000_000_000,
        }
    }

    pub fn from_secs(secs: u64) -> Duration {
        Duration {
            __secs: secs,
            __nanos: 0,
        }
    }

    pub fn from_millis(millis: u64) -> Duration {
        Duration {
            __secs: millis / 1_000,
            __nanos: (millis % 1_000) as u32 * 1_000_000,
        }
    }

    pub fn from_micros(micros: u64) -> Duration {
        Duration {
            __secs: micros / 1_000_000,
            __nanos: (micros % 1_000_000) as u32 * 1_000,
        }
    }

    pub fn from_nanos(nanos: u64) -> Duration {
        Duration {
            __secs: nanos / 1_000_000_000,
            __nanos: (nanos % 1_000_000_000) as u32,
        }
    }

    pub fn is_zero(&self) -> bool {
        self.__secs == 0 && self.__nanos == 0
    }

    pub fn as_secs(&self) -> u64 {
        self.__secs
    }

    pub fn subsec_millis(&self) -> u32 {
        self.__nanos / 1_000_000
    }

    pub fn subsec_micros(&self) -> u32 {
        self.__nanos / 1_000
    }

    pub fn subsec_nanos(&self) -> u32 {
        self.__nanos
    }

    pub fn as_millis(&self) -> u128 {
        self.__secs as u128 * 1_000 + (self.__nanos / 1_000_000) as u128
    }

    pub fn as_micros(&self) -> u128 {
        self.__secs as u128 * 1_000_000 + (self.__nanos / 1_000) as u128
    }

    pub fn as_nanos(&self) -> u128 {
        self.__secs as u128 * 1_000_000_000 + self.__nanos as u128
    }

    pub fn abs_diff(self, other: Duration) -> Duration {
        if let Some(d) = self.checked_sub(other) {
            d
        } else {
            other - self
        }
    }

    pub fn checked_add(self, rhs: Duration) -> Option<Duration> {
        let mut secs = self.__secs.checked_add(rhs.__secs)?;
        let mut nanos = self.__nanos + rhs.__nanos;
        if nanos >= 1_000_000_000 {
            nanos -= 1_000_000_000;
            secs = secs.checked_add(1)?;
        }
        Some(Duration {
            __secs: secs,
            __nanos: nanos,
        })
    }

    pub fn saturating_add(self, rhs: Duration) -> Duration {
        match self.checked_add(rhs) {
            Some(d) => d,
            None => Duration::MAX,
        }
    }

    pub fn checked_sub(self, rhs: Duration) -> Option<Duration> {
        let mut secs = self.__secs.checked_sub(rhs.__secs)?;
        let nanos = if self.__nanos >= rhs.__nanos {
            self.__nanos - rhs.__nanos
        } else {
            secs = secs.checked_sub(1)?;
            self.__nanos + 1_000_000_000 - rhs.__nanos
        };
        Some(Duration {
            __secs: secs,
            __nanos: nanos,
        })
    }

    pub fn saturating_sub(self, rhs: Duration) -> Duration {
        match self.checked_sub(rhs) {
            Some(d) => d,
            None => Duration::ZERO,
        }
    }

    pub fn checked_mul(self, rhs: u32) -> Option<Duration> {
        let total_nanos = self.__nanos as u64 * rhs as u64;
        let extra_secs = total_nanos / 1_000_000_000;
        let nanos = (total_nanos % 1_000_000_000) as u32;
        let secs = self.__secs.checked_mul(rhs as u64)?.checked_add(extra_secs)?;
        Some(Duration {
            __secs: secs,
            __nanos: nanos,
        })
    }

    pub fn saturating_mul(self, rhs: u32) -> Duration {
        match self.checked_mul(rhs) {
            Some(d) => d,
            None => Duration::MAX,
        }
    }

    pub fn checked_div(self, rhs: u32) -> Option<Duration> {
        if rhs == 0 {
            return None;
        }
        let secs = self.__secs / rhs as u64;
        let extra_secs = self.__secs % rhs as u64;
        let mut nanos = self.__nanos / rhs;
        let extra_nanos = self.__nanos % rhs;
        nanos += ((extra_secs as u128 * 1_000_000_000 + extra_nanos as u128) / rhs as u128) as u32;
        Some(Duration::new(secs, nanos))
    }

    pub fn as_secs_f64(&self) -> f64 {
        self.__secs as f64 + self.__nanos as f64 / 1e9
    }

    pub fn as_secs_f32(&self) -> f32 {
        self.__secs as f32 + self.__nanos as f32 / 1e9
    }

    pub fn from_secs_f64(secs: f64) -> Duration {
        if secs < 0.0 {
            panic!("cannot convert float seconds to Duration: value is negative");
        }
        if !(secs < 18446744073709551616.0) {
            panic!("cannot convert float seconds to Duration: value is either too big or NaN");
        }
        let whole = secs.trunc();
        let mut nanos = ((secs - whole) * 1e9).round();
        let mut s = whole;
        if nanos >= 1e9 {
            nanos -= 1e9;
            s += 1.0;
        }
        Duration {
            __secs: s as u64,
            __nanos: nanos as u32,
        }
    }

    pub fn from_secs_f32(secs: f32) -> Duration {
        Duration::from_secs_f64(secs as f64)
    }

    pub fn try_from_secs_f64(secs: f64) -> Result<Duration, TryFromFloatSecsError> {
        if secs < 0.0 {
            return Err(TryFromFloatSecsError { __negative: true });
        }
        if !(secs < 18446744073709551616.0) {
            return Err(TryFromFloatSecsError { __negative: false });
        }
        Ok(Duration::from_secs_f64(secs))
    }

    pub fn try_from_secs_f32(secs: f32) -> Result<Duration, TryFromFloatSecsError> {
        Duration::try_from_secs_f64(secs as f64)
    }

    pub fn mul_f64(self, rhs: f64) -> Duration {
        Duration::from_secs_f64(rhs * self.as_secs_f64())
    }

    pub fn mul_f32(self, rhs: f32) -> Duration {
        Duration::from_secs_f64((rhs * self.as_secs_f32()) as f64)
    }

    pub fn div_f64(self, rhs: f64) -> Duration {
        Duration::from_secs_f64(self.as_secs_f64() / rhs)
    }

    pub fn div_f32(self, rhs: f32) -> Duration {
        Duration::from_secs_f64((self.as_secs_f32() / rhs) as f64)
    }

    pub fn div_duration_f64(self, rhs: Duration) -> f64 {
        let a = self.__secs as f64 * 1e9 + self.__nanos as f64;
        let b = rhs.__secs as f64 * 1e9 + rhs.__nanos as f64;
        a / b
    }

    pub fn div_duration_f32(self, rhs: Duration) -> f32 {
        let a = self.__secs as f32 * 1e9 + self.__nanos as f32;
        let b = rhs.__secs as f32 * 1e9 + rhs.__nanos as f32;
        a / b
    }
}

impl std::ops::Add for Duration {
    type Output = Duration;
    fn add(self, rhs: Duration) -> Duration {
        self.checked_add(rhs)
            .expect("overflow when adding durations")
    }
}

impl std::ops::AddAssign for Duration {
    fn add_assign(&mut self, rhs: Duration) {
        *self = *self + rhs;
    }
}

impl std::ops::Sub for Duration {
    type Output = Duration;
    fn sub(self, rhs: Duration) -> Duration {
        self.checked_sub(rhs)
            .expect("overflow when subtracting durations")
    }
}

impl std::ops::SubAssign for Duration {
    fn sub_assign(&mut self, rhs: Duration) {
        *self = *self - rhs;
    }
}

impl std::ops::Mul<u32> for Duration {
    type Output = Duration;
    fn mul(self, rhs: u32) -> Duration {
        self.checked_mul(rhs)
            .expect("overflow when multiplying duration by scalar")
    }
}

impl std::ops::MulAssign<u32> for Duration {
    fn mul_assign(&mut self, rhs: u32) {
        *self = *self * rhs;
    }
}

impl std::ops::Div<u32> for Duration {
    type Output = Duration;
    fn div(self, rhs: u32) -> Duration {
        self.checked_div(rhs)
            .expect("divide by zero error when dividing duration by scalar")
    }
}

impl std::ops::DivAssign<u32> for Duration {
    fn div_assign(&mut self, rhs: u32) {
        *self = *self / rhs;
    }
}

impl std::iter::Sum for Duration {
    fn sum<I: Iterator<Item = Duration>>(iter: I) -> Duration {
        let mut total = Duration::ZERO;
        for d in iter {
            total = total.checked_add(d).expect("overflow in iter::sum over durations");
        }
        total
    }
}

impl std::fmt::Debug for Duration {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let prefix = if f.sign_plus() { "+" } else { "" };
        if self.__secs > 0 {
            __fmt_decimal(f, self.__secs, self.__nanos, 100_000_000, prefix, "s")
        } else if self.__nanos >= 1_000_000 {
            __fmt_decimal(
                f,
                (self.__nanos / 1_000_000) as u64,
                self.__nanos % 1_000_000,
                100_000,
                prefix,
                "ms",
            )
        } else if self.__nanos >= 1_000 {
            __fmt_decimal(
                f,
                (self.__nanos / 1_000) as u64,
                self.__nanos % 1_000,
                100,
                prefix,
                "µs",
            )
        } else {
            __fmt_decimal(f, self.__nanos as u64, 0, 1, prefix, "ns")
        }
    }
}

/// `Duration`'s `{:?}`: the integer part, up to nine fractional digits (or the precision,
/// rounded half up), the unit, padded to the width.
fn __fmt_decimal(
    f: &mut std::fmt::Formatter<'_>,
    integer_part: u64,
    fractional_part: u32,
    divisor: u32,
    prefix: &str,
    postfix: &str,
) -> std::fmt::Result {
    let mut integer_part = integer_part;
    let mut fractional_part = fractional_part;
    let mut divisor = divisor;
    let mut buf = [0u32; 9];
    let mut pos = 0;
    let end = match f.precision() {
        Some(p) => p.min(9),
        None => 9,
    };
    while fractional_part > 0 && pos < end {
        buf[pos] = fractional_part / divisor;
        fractional_part %= divisor;
        divisor /= 10;
        pos += 1;
    }
    let mut overflow = false;
    if fractional_part > 0 && fractional_part >= divisor * 5 {
        let mut rev_pos = pos;
        let mut carry = true;
        while carry && rev_pos > 0 {
            rev_pos -= 1;
            if buf[rev_pos] < 9 {
                buf[rev_pos] += 1;
                carry = false;
            } else {
                buf[rev_pos] = 0;
            }
        }
        if carry {
            match integer_part.checked_add(1) {
                Some(i) => integer_part = i,
                None => overflow = true,
            }
        }
    }
    let end = match f.precision() {
        Some(p) => p.min(9),
        None => pos,
    };
    let mut s = String::from(prefix);
    if overflow {
        s.push_str("18446744073709551616");
    } else {
        s.push_str(&integer_part.to_string());
    }
    if end > 0 {
        s.push('.');
        for d in buf.iter().take(end) {
            s.push_str(&d.to_string());
        }
        let width = f.precision().unwrap_or(pos);
        for _ in end..width {
            s.push('0');
        }
    }
    s.push_str(postfix);
    f.__pad_whole(&s)
}

/// `std::time::Instant`: `performance.now()`, as the time since the page loaded.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Instant {
    __t: Duration,
}

impl Instant {
    pub fn now() -> Instant {
        let ms = js::performance::now();
        Instant {
            __t: Duration::from_nanos((ms * 1e6).round() as u64),
        }
    }

    pub fn duration_since(&self, earlier: Instant) -> Duration {
        self.__t.saturating_sub(earlier.__t)
    }

    pub fn checked_duration_since(&self, earlier: Instant) -> Option<Duration> {
        self.__t.checked_sub(earlier.__t)
    }

    pub fn saturating_duration_since(&self, earlier: Instant) -> Duration {
        self.__t.saturating_sub(earlier.__t)
    }

    pub fn elapsed(&self) -> Duration {
        Instant::now().duration_since(*self)
    }

    pub fn checked_add(&self, duration: Duration) -> Option<Instant> {
        Some(Instant {
            __t: self.__t.checked_add(duration)?,
        })
    }

    pub fn checked_sub(&self, duration: Duration) -> Option<Instant> {
        Some(Instant {
            __t: self.__t.checked_sub(duration)?,
        })
    }
}

impl std::ops::Add<Duration> for Instant {
    type Output = Instant;
    fn add(self, rhs: Duration) -> Instant {
        self.checked_add(rhs)
            .expect("overflow when adding duration to instant")
    }
}

impl std::ops::AddAssign<Duration> for Instant {
    fn add_assign(&mut self, rhs: Duration) {
        *self = *self + rhs;
    }
}

impl std::ops::Sub<Duration> for Instant {
    type Output = Instant;
    fn sub(self, rhs: Duration) -> Instant {
        self.checked_sub(rhs)
            .expect("overflow when subtracting duration from instant")
    }
}

impl std::ops::SubAssign<Duration> for Instant {
    fn sub_assign(&mut self, rhs: Duration) {
        *self = *self - rhs;
    }
}

impl std::ops::Sub<Instant> for Instant {
    type Output = Duration;
    fn sub(self, rhs: Instant) -> Duration {
        self.duration_since(rhs)
    }
}

impl std::fmt::Debug for Instant {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Instant {{ t: {:?} }}", self.__t)
    }
}

/// `std::time::SystemTime`: `Date.now()`, as nanoseconds from the Unix epoch (negative before
/// it, as on every platform std supports).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SystemTime {
    __ns: i128,
}

pub const UNIX_EPOCH: SystemTime = SystemTime { __ns: 0 };

fn __duration_of_nanos(ns: u128) -> Duration {
    Duration {
        __secs: (ns / 1_000_000_000) as u64,
        __nanos: (ns % 1_000_000_000) as u32,
    }
}

impl SystemTime {
    pub const UNIX_EPOCH: SystemTime = SystemTime { __ns: 0 };

    pub fn now() -> SystemTime {
        SystemTime {
            __ns: js::Date::now() as i128 * 1_000_000,
        }
    }

    pub fn duration_since(&self, earlier: SystemTime) -> Result<Duration, SystemTimeError> {
        if self.__ns >= earlier.__ns {
            Ok(__duration_of_nanos((self.__ns - earlier.__ns) as u128))
        } else {
            Err(SystemTimeError(__duration_of_nanos((earlier.__ns - self.__ns) as u128)))
        }
    }

    pub fn elapsed(&self) -> Result<Duration, SystemTimeError> {
        SystemTime::now().duration_since(*self)
    }

    pub fn checked_add(&self, duration: Duration) -> Option<SystemTime> {
        Some(SystemTime {
            __ns: self.__ns + duration.as_nanos() as i128,
        })
    }

    pub fn checked_sub(&self, duration: Duration) -> Option<SystemTime> {
        Some(SystemTime {
            __ns: self.__ns - duration.as_nanos() as i128,
        })
    }
}

impl std::ops::Add<Duration> for SystemTime {
    type Output = SystemTime;
    fn add(self, rhs: Duration) -> SystemTime {
        self.checked_add(rhs)
            .expect("overflow when adding duration to instant")
    }
}

impl std::ops::AddAssign<Duration> for SystemTime {
    fn add_assign(&mut self, rhs: Duration) {
        *self = *self + rhs;
    }
}

impl std::ops::Sub<Duration> for SystemTime {
    type Output = SystemTime;
    fn sub(self, rhs: Duration) -> SystemTime {
        self.checked_sub(rhs)
            .expect("overflow when subtracting duration from instant")
    }
}

impl std::ops::SubAssign<Duration> for SystemTime {
    fn sub_assign(&mut self, rhs: Duration) {
        *self = *self - rhs;
    }
}

impl std::fmt::Debug for SystemTime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut secs = self.__ns / 1_000_000_000;
        let mut nanos = self.__ns % 1_000_000_000;
        if nanos < 0 {
            secs -= 1;
            nanos += 1_000_000_000;
        }
        write!(f, "SystemTime {{ tv_sec: {}, tv_nsec: {} }}", secs, nanos)
    }
}

/// `SystemTime::duration_since` with a later time.
#[derive(Clone, Debug)]
pub struct SystemTimeError(Duration);

impl SystemTimeError {
    pub fn duration(&self) -> Duration {
        self.0
    }
}

impl std::fmt::Display for SystemTimeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "second time provided was later than self")
    }
}

impl std::error::Error for SystemTimeError {}

#[derive(Clone, PartialEq, Eq)]
pub struct TryFromFloatSecsError {
    __negative: bool,
}

impl std::fmt::Debug for TryFromFloatSecsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let kind = if self.__negative { "Negative" } else { "OverflowOrNan" };
        write!(f, "TryFromFloatSecsError {{ kind: {kind} }}")
    }
}

impl std::fmt::Display for TryFromFloatSecsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.__negative {
            write!(f, "cannot convert float seconds to Duration: value is negative")
        } else {
            write!(f, "cannot convert float seconds to Duration: value is either too big or NaN")
        }
    }
}

impl std::error::Error for TryFromFloatSecsError {}
