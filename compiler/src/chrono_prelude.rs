// (chrono prelude)
// The bundled `chrono`: `NaiveDate`, `NaiveTime`, `NaiveDateTime`, `DateTime<Tz>` with the time
// zones `Utc`, `Local` (the JS host's) and `FixedOffset`, `TimeDelta` (`Duration`), `Days`,
// `Months`, `Weekday`, `Month`, `IsoWeek`, the `Datelike` / `Timelike` / `TimeZone` traits,
// strftime-style `format` / `parse_from_str`, RFC 3339 / 2822, and `ParseError`.

// ---- the calendar

fn __chrono_is_leap(y: i32) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

fn __chrono_month_len(y: i32, m: u32) -> u32 {
    match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        _ => {
            if __chrono_is_leap(y) {
                29
            } else {
                28
            }
        }
    }
}

/// Days from 1970-01-01 to a proleptic Gregorian date.
fn __chrono_days_from_civil(y: i32, m: u32, d: u32) -> i32 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (m as i32 + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d as i32 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

/// The date `z` days after 1970-01-01.
fn __chrono_civil_from_days(z: i32) -> (i32, u32, u32) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m as u32, d as u32)
}

const __CHRONO_MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

const __CHRONO_DAYS: [&str; 7] = [
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
    "Sunday",
];

// ---- Weekday and Month

/// `chrono::Weekday`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ChronoWeekday {
    Mon,
    Tue,
    Wed,
    Thu,
    Fri,
    Sat,
    Sun,
}

impl ChronoWeekday {
    fn __of(n: u32) -> ChronoWeekday {
        match n % 7 {
            0 => ChronoWeekday::Mon,
            1 => ChronoWeekday::Tue,
            2 => ChronoWeekday::Wed,
            3 => ChronoWeekday::Thu,
            4 => ChronoWeekday::Fri,
            5 => ChronoWeekday::Sat,
            _ => ChronoWeekday::Sun,
        }
    }
    pub fn num_days_from_monday(&self) -> u32 {
        match self {
            ChronoWeekday::Mon => 0,
            ChronoWeekday::Tue => 1,
            ChronoWeekday::Wed => 2,
            ChronoWeekday::Thu => 3,
            ChronoWeekday::Fri => 4,
            ChronoWeekday::Sat => 5,
            ChronoWeekday::Sun => 6,
        }
    }
    pub fn num_days_from_sunday(&self) -> u32 {
        (self.num_days_from_monday() + 1) % 7
    }
    pub fn number_from_monday(&self) -> u32 {
        self.num_days_from_monday() + 1
    }
    pub fn number_from_sunday(&self) -> u32 {
        self.num_days_from_sunday() + 1
    }
    pub fn days_since(&self, other: ChronoWeekday) -> u32 {
        (self.num_days_from_monday() + 7 - other.num_days_from_monday()) % 7
    }
    pub fn succ(&self) -> ChronoWeekday {
        ChronoWeekday::__of(self.num_days_from_monday() + 1)
    }
    pub fn pred(&self) -> ChronoWeekday {
        ChronoWeekday::__of(self.num_days_from_monday() + 6)
    }
}

impl std::fmt::Display for ChronoWeekday {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.pad(&__CHRONO_DAYS[self.num_days_from_monday() as usize][..3])
    }
}

impl std::str::FromStr for ChronoWeekday {
    type Err = ChronoParseWeekdayError;
    fn from_str(s: &str) -> Result<ChronoWeekday, ChronoParseWeekdayError> {
        let l = s.to_lowercase();
        for (i, name) in __CHRONO_DAYS.iter().enumerate() {
            let n = name.to_lowercase();
            if l == n || l == n[..3] {
                return Ok(ChronoWeekday::__of(i as u32));
            }
        }
        Err(ChronoParseWeekdayError {})
    }
}

/// `chrono::ParseWeekdayError`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChronoParseWeekdayError {}

impl std::fmt::Display for ChronoParseWeekdayError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "ParseWeekdayError {{ .. }}")
    }
}

impl std::error::Error for ChronoParseWeekdayError {}

/// `chrono::Month`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ChronoMonth {
    January,
    February,
    March,
    April,
    May,
    June,
    July,
    August,
    September,
    October,
    November,
    December,
}

impl ChronoMonth {
    fn __of(n: u32) -> ChronoMonth {
        match n % 12 {
            0 => ChronoMonth::January,
            1 => ChronoMonth::February,
            2 => ChronoMonth::March,
            3 => ChronoMonth::April,
            4 => ChronoMonth::May,
            5 => ChronoMonth::June,
            6 => ChronoMonth::July,
            7 => ChronoMonth::August,
            8 => ChronoMonth::September,
            9 => ChronoMonth::October,
            10 => ChronoMonth::November,
            _ => ChronoMonth::December,
        }
    }
    pub fn number_from_month(&self) -> u32 {
        match self {
            ChronoMonth::January => 1,
            ChronoMonth::February => 2,
            ChronoMonth::March => 3,
            ChronoMonth::April => 4,
            ChronoMonth::May => 5,
            ChronoMonth::June => 6,
            ChronoMonth::July => 7,
            ChronoMonth::August => 8,
            ChronoMonth::September => 9,
            ChronoMonth::October => 10,
            ChronoMonth::November => 11,
            ChronoMonth::December => 12,
        }
    }
    pub fn name(&self) -> &'static str {
        __CHRONO_MONTHS[self.number_from_month() as usize - 1]
    }
    pub fn succ(&self) -> ChronoMonth {
        ChronoMonth::__of(self.number_from_month())
    }
    pub fn pred(&self) -> ChronoMonth {
        ChronoMonth::__of(self.number_from_month() + 10)
    }
    pub fn num_days(&self, year: i32) -> Option<u8> {
        Some(__chrono_month_len(year, self.number_from_month()) as u8)
    }
}

impl TryFrom<u8> for ChronoMonth {
    type Error = ChronoOutOfRange;
    fn try_from(n: u8) -> Result<ChronoMonth, ChronoOutOfRange> {
        if (1..=12).contains(&n) {
            Ok(ChronoMonth::__of(n as u32 - 1))
        } else {
            Err(ChronoOutOfRange {})
        }
    }
}

/// `chrono::OutOfRange`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChronoOutOfRange {}

impl std::fmt::Display for ChronoOutOfRange {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "out of range")
    }
}

impl std::error::Error for ChronoOutOfRange {}

// ---- TimeDelta

/// `chrono::TimeDelta` (`chrono::Duration`): seconds and nanoseconds (0 to 1e9), so negative
/// ones have a positive nanosecond part.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct ChronoTimeDelta {
    secs: i64,
    nanos: i32,
}

impl ChronoTimeDelta {
    fn __norm(secs: i64, nanos: i64) -> ChronoTimeDelta {
        ChronoTimeDelta {
            secs: secs + nanos.div_euclid(1_000_000_000),
            nanos: nanos.rem_euclid(1_000_000_000) as i32,
        }
    }
    pub fn new(secs: i64, nanos: u32) -> Option<ChronoTimeDelta> {
        if nanos >= 1_000_000_000 {
            return None;
        }
        Some(ChronoTimeDelta { secs, nanos: nanos as i32 })
    }
    pub fn zero() -> ChronoTimeDelta {
        ChronoTimeDelta { secs: 0, nanos: 0 }
    }
    pub fn weeks(n: i64) -> ChronoTimeDelta {
        ChronoTimeDelta { secs: n * 604800, nanos: 0 }
    }
    pub fn days(n: i64) -> ChronoTimeDelta {
        ChronoTimeDelta { secs: n * 86400, nanos: 0 }
    }
    pub fn hours(n: i64) -> ChronoTimeDelta {
        ChronoTimeDelta { secs: n * 3600, nanos: 0 }
    }
    pub fn minutes(n: i64) -> ChronoTimeDelta {
        ChronoTimeDelta { secs: n * 60, nanos: 0 }
    }
    pub fn seconds(n: i64) -> ChronoTimeDelta {
        ChronoTimeDelta { secs: n, nanos: 0 }
    }
    pub fn milliseconds(n: i64) -> ChronoTimeDelta {
        ChronoTimeDelta::__norm(n.div_euclid(1000), n.rem_euclid(1000) * 1_000_000)
    }
    pub fn microseconds(n: i64) -> ChronoTimeDelta {
        ChronoTimeDelta::__norm(n.div_euclid(1_000_000), n.rem_euclid(1_000_000) * 1000)
    }
    pub fn nanoseconds(n: i64) -> ChronoTimeDelta {
        ChronoTimeDelta::__norm(0, n)
    }
    pub fn try_weeks(n: i64) -> Option<ChronoTimeDelta> {
        Some(ChronoTimeDelta::weeks(n))
    }
    pub fn try_days(n: i64) -> Option<ChronoTimeDelta> {
        Some(ChronoTimeDelta::days(n))
    }
    pub fn try_hours(n: i64) -> Option<ChronoTimeDelta> {
        Some(ChronoTimeDelta::hours(n))
    }
    pub fn try_minutes(n: i64) -> Option<ChronoTimeDelta> {
        Some(ChronoTimeDelta::minutes(n))
    }
    pub fn try_seconds(n: i64) -> Option<ChronoTimeDelta> {
        Some(ChronoTimeDelta::seconds(n))
    }
    pub fn try_milliseconds(n: i64) -> Option<ChronoTimeDelta> {
        Some(ChronoTimeDelta::milliseconds(n))
    }
    pub fn num_weeks(&self) -> i64 {
        self.num_seconds() / 604800
    }
    pub fn num_days(&self) -> i64 {
        self.num_seconds() / 86400
    }
    pub fn num_hours(&self) -> i64 {
        self.num_seconds() / 3600
    }
    pub fn num_minutes(&self) -> i64 {
        self.num_seconds() / 60
    }
    pub fn num_seconds(&self) -> i64 {
        if self.secs < 0 && self.nanos > 0 {
            self.secs + 1
        } else {
            self.secs
        }
    }
    pub fn subsec_nanos(&self) -> i32 {
        if self.secs < 0 && self.nanos > 0 {
            self.nanos - 1_000_000_000
        } else {
            self.nanos
        }
    }
    pub fn subsec_millis(&self) -> i32 {
        self.subsec_nanos() / 1_000_000
    }
    pub fn subsec_micros(&self) -> i32 {
        self.subsec_nanos() / 1000
    }
    pub fn num_milliseconds(&self) -> i64 {
        self.num_seconds() * 1000 + (self.subsec_nanos() / 1_000_000) as i64
    }
    pub fn num_microseconds(&self) -> Option<i64> {
        Some(self.num_seconds() * 1_000_000 + (self.subsec_nanos() / 1000) as i64)
    }
    pub fn num_nanoseconds(&self) -> Option<i64> {
        Some(self.num_seconds() * 1_000_000_000 + self.subsec_nanos() as i64)
    }
    pub fn as_seconds_f64(&self) -> f64 {
        self.secs as f64 + self.nanos as f64 / 1e9
    }
    pub fn as_seconds_f32(&self) -> f32 {
        self.as_seconds_f64() as f32
    }
    pub fn is_zero(&self) -> bool {
        self.secs == 0 && self.nanos == 0
    }
    pub fn abs(&self) -> ChronoTimeDelta {
        if self.secs < 0 {
            ChronoTimeDelta::__norm(-self.secs, -(self.nanos as i64))
        } else {
            *self
        }
    }
    pub fn checked_add(&self, rhs: &ChronoTimeDelta) -> Option<ChronoTimeDelta> {
        Some(*self + *rhs)
    }
    pub fn checked_sub(&self, rhs: &ChronoTimeDelta) -> Option<ChronoTimeDelta> {
        Some(*self - *rhs)
    }
    pub fn checked_mul(&self, rhs: i32) -> Option<ChronoTimeDelta> {
        Some(*self * rhs)
    }
    pub fn checked_div(&self, rhs: i32) -> Option<ChronoTimeDelta> {
        if rhs == 0 {
            None
        } else {
            Some(*self / rhs)
        }
    }
    pub fn to_std(&self) -> Result<Duration, ChronoOutOfRangeError> {
        if self.secs < 0 {
            return Err(ChronoOutOfRangeError {});
        }
        Ok(Duration::new(self.secs as u64, self.nanos as u32))
    }
    pub fn from_std(d: Duration) -> Result<ChronoTimeDelta, ChronoOutOfRangeError> {
        Ok(ChronoTimeDelta {
            secs: d.as_secs() as i64,
            nanos: d.subsec_nanos() as i32,
        })
    }
}

impl std::ops::Add<ChronoTimeDelta> for ChronoTimeDelta {
    type Output = ChronoTimeDelta;
    fn add(self, rhs: ChronoTimeDelta) -> ChronoTimeDelta {
        ChronoTimeDelta::__norm(self.secs + rhs.secs, (self.nanos + rhs.nanos) as i64)
    }
}

impl std::ops::Sub<ChronoTimeDelta> for ChronoTimeDelta {
    type Output = ChronoTimeDelta;
    fn sub(self, rhs: ChronoTimeDelta) -> ChronoTimeDelta {
        ChronoTimeDelta::__norm(self.secs - rhs.secs, (self.nanos - rhs.nanos) as i64)
    }
}

impl std::ops::Neg for ChronoTimeDelta {
    type Output = ChronoTimeDelta;
    fn neg(self) -> ChronoTimeDelta {
        ChronoTimeDelta::__norm(-self.secs, -(self.nanos as i64))
    }
}

impl std::ops::Mul<i32> for ChronoTimeDelta {
    type Output = ChronoTimeDelta;
    fn mul(self, rhs: i32) -> ChronoTimeDelta {
        let total = (self.secs as i128 * 1_000_000_000 + self.nanos as i128) * rhs as i128;
        ChronoTimeDelta {
            secs: total.div_euclid(1_000_000_000) as i64,
            nanos: total.rem_euclid(1_000_000_000) as i32,
        }
    }
}

impl std::ops::Div<i32> for ChronoTimeDelta {
    type Output = ChronoTimeDelta;
    fn div(self, rhs: i32) -> ChronoTimeDelta {
        if rhs == 0 {
            panic!("`i32` is zero");
        }
        let total = self.secs as i128 * 1_000_000_000 + self.nanos as i128;
        let q = total / rhs as i128;
        ChronoTimeDelta {
            secs: q.div_euclid(1_000_000_000) as i64,
            nanos: q.rem_euclid(1_000_000_000) as i32,
        }
    }
}

impl std::ops::AddAssign<ChronoTimeDelta> for ChronoTimeDelta {
    fn add_assign(&mut self, rhs: ChronoTimeDelta) {
        *self = *self + rhs;
    }
}

impl std::ops::SubAssign<ChronoTimeDelta> for ChronoTimeDelta {
    fn sub_assign(&mut self, rhs: ChronoTimeDelta) {
        *self = *self - rhs;
    }
}

impl std::iter::Sum for ChronoTimeDelta {
    fn sum<I: Iterator<Item = ChronoTimeDelta>>(iter: I) -> ChronoTimeDelta {
        let mut t = ChronoTimeDelta::zero();
        for d in iter {
            t = t + d;
        }
        t
    }
}

impl std::fmt::Debug for ChronoTimeDelta {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "TimeDelta {{ secs: {}, nanos: {} }}", self.secs, self.nanos)
    }
}

impl std::fmt::Display for ChronoTimeDelta {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let (abs, sign) = if self.secs < 0 { (-*self, "-") } else { (*self, "") };
        write!(f, "{sign}P")?;
        if abs.secs == 0 && abs.nanos == 0 {
            return f.write_str("0D");
        }
        write!(f, "T{}", abs.secs)?;
        if abs.nanos > 0 {
            let mut figures = 9;
            let mut digits = abs.nanos;
            while digits % 10 == 0 {
                digits /= 10;
                figures -= 1;
            }
            write!(f, ".{:0w$}", digits, w = figures)?;
        }
        f.write_str("S")
    }
}

/// `chrono::OutOfRangeError` (`TimeDelta::to_std` of a negative one).
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct ChronoOutOfRangeError {}

impl std::fmt::Debug for ChronoOutOfRangeError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "OutOfRangeError(())")
    }
}

impl std::fmt::Display for ChronoOutOfRangeError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "Source duration value is out of range for the target type")
    }
}

impl std::error::Error for ChronoOutOfRangeError {}

/// `chrono::Days`.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ChronoDays(u64);

impl ChronoDays {
    pub fn new(n: u64) -> ChronoDays {
        ChronoDays(n)
    }
}

impl std::fmt::Debug for ChronoDays {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "Days({})", self.0)
    }
}

/// `chrono::Months`.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ChronoMonths(u32);

impl ChronoMonths {
    pub fn new(n: u32) -> ChronoMonths {
        ChronoMonths(n)
    }
    pub fn as_u32(&self) -> u32 {
        self.0
    }
}

impl std::fmt::Debug for ChronoMonths {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "Months({})", self.0)
    }
}

// ---- Datelike and Timelike

/// `chrono::Datelike`.
pub trait ChronoDatelike {
    fn __date(&self) -> ChronoNaiveDate;
    fn __with_date(&self, d: ChronoNaiveDate) -> Option<Self>
    where
        Self: Sized;
    fn year(&self) -> i32 {
        __chrono_civil_from_days(self.__date().days).0
    }
    fn month(&self) -> u32 {
        __chrono_civil_from_days(self.__date().days).1
    }
    fn month0(&self) -> u32 {
        self.month() - 1
    }
    fn day(&self) -> u32 {
        __chrono_civil_from_days(self.__date().days).2
    }
    fn day0(&self) -> u32 {
        self.day() - 1
    }
    fn ordinal(&self) -> u32 {
        let d = self.__date().days;
        let y = __chrono_civil_from_days(d).0;
        (d - __chrono_days_from_civil(y, 1, 1) + 1) as u32
    }
    fn ordinal0(&self) -> u32 {
        self.ordinal() - 1
    }
    fn weekday(&self) -> ChronoWeekday {
        ChronoWeekday::__of((self.__date().days + 3).rem_euclid(7) as u32)
    }
    fn iso_week(&self) -> ChronoIsoWeek {
        __chrono_iso_week(self.__date())
    }
    fn num_days_from_ce(&self) -> i32 {
        self.__date().days + 719163
    }
    fn year_ce(&self) -> (bool, u32) {
        let y = self.year();
        if y < 1 {
            (false, (1 - y) as u32)
        } else {
            (true, y as u32)
        }
    }
    fn with_year(&self, year: i32) -> Option<Self>
    where
        Self: Sized,
    {
        let (_, m, d) = __chrono_civil_from_days(self.__date().days);
        self.__with_date(ChronoNaiveDate::from_ymd_opt(year, m, d)?)
    }
    fn with_month(&self, month: u32) -> Option<Self>
    where
        Self: Sized,
    {
        let (y, _, d) = __chrono_civil_from_days(self.__date().days);
        self.__with_date(ChronoNaiveDate::from_ymd_opt(y, month, d)?)
    }
    fn with_month0(&self, month0: u32) -> Option<Self>
    where
        Self: Sized,
    {
        self.with_month(month0 + 1)
    }
    fn with_day(&self, day: u32) -> Option<Self>
    where
        Self: Sized,
    {
        let (y, m, _) = __chrono_civil_from_days(self.__date().days);
        self.__with_date(ChronoNaiveDate::from_ymd_opt(y, m, day)?)
    }
    fn with_day0(&self, day0: u32) -> Option<Self>
    where
        Self: Sized,
    {
        self.with_day(day0 + 1)
    }
    fn with_ordinal(&self, ordinal: u32) -> Option<Self>
    where
        Self: Sized,
    {
        self.__with_date(ChronoNaiveDate::from_yo_opt(self.year(), ordinal)?)
    }
}

/// `chrono::Timelike`.
pub trait ChronoTimelike {
    fn __time(&self) -> ChronoNaiveTime;
    fn __with_time(&self, t: ChronoNaiveTime) -> Option<Self>
    where
        Self: Sized;
    fn hour(&self) -> u32 {
        self.__time().secs / 3600
    }
    fn minute(&self) -> u32 {
        self.__time().secs / 60 % 60
    }
    fn second(&self) -> u32 {
        self.__time().secs % 60
    }
    fn nanosecond(&self) -> u32 {
        self.__time().frac
    }
    fn num_seconds_from_midnight(&self) -> u32 {
        self.__time().secs
    }
    fn hour12(&self) -> (bool, u32) {
        let h = self.hour();
        let h12 = if h % 12 == 0 { 12 } else { h % 12 };
        (h >= 12, h12)
    }
    fn with_hour(&self, hour: u32) -> Option<Self>
    where
        Self: Sized,
    {
        let t = self.__time();
        self.__with_time(ChronoNaiveTime::from_hms_nano_opt(hour, t.secs / 60 % 60, t.secs % 60, t.frac)?)
    }
    fn with_minute(&self, min: u32) -> Option<Self>
    where
        Self: Sized,
    {
        let t = self.__time();
        self.__with_time(ChronoNaiveTime::from_hms_nano_opt(t.secs / 3600, min, t.secs % 60, t.frac)?)
    }
    fn with_second(&self, sec: u32) -> Option<Self>
    where
        Self: Sized,
    {
        let t = self.__time();
        self.__with_time(ChronoNaiveTime::from_hms_nano_opt(t.secs / 3600, t.secs / 60 % 60, sec, t.frac)?)
    }
    fn with_nanosecond(&self, nano: u32) -> Option<Self>
    where
        Self: Sized,
    {
        let t = self.__time();
        self.__with_time(ChronoNaiveTime::from_hms_nano_opt(t.secs / 3600, t.secs / 60 % 60, t.secs % 60, nano)?)
    }
}

/// `chrono::IsoWeek`.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ChronoIsoWeek {
    y: i32,
    w: u32,
}

impl ChronoIsoWeek {
    pub fn year(&self) -> i32 {
        self.y
    }
    pub fn week(&self) -> u32 {
        self.w
    }
    pub fn week0(&self) -> u32 {
        self.w - 1
    }
}

impl std::fmt::Debug for ChronoIsoWeek {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        if (0..=9999).contains(&self.y) {
            write!(f, "{:04}-W{:02}", self.y, self.w)
        } else {
            write!(f, "{:+05}-W{:02}", self.y, self.w)
        }
    }
}

fn __chrono_weeks_in_year(y: i32) -> u32 {
    let jan1 = (__chrono_days_from_civil(y, 1, 1) + 3).rem_euclid(7);
    if jan1 == 3 || (jan1 == 2 && __chrono_is_leap(y)) {
        53
    } else {
        52
    }
}

fn __chrono_iso_week(d: ChronoNaiveDate) -> ChronoIsoWeek {
    let y = d.year();
    let wd = d.weekday().num_days_from_monday() as i32;
    let w = (d.ordinal() as i32 - wd + 9) / 7;
    if w < 1 {
        ChronoIsoWeek { y: y - 1, w: __chrono_weeks_in_year(y - 1) }
    } else if w as u32 > __chrono_weeks_in_year(y) {
        ChronoIsoWeek { y: y + 1, w: 1 }
    } else {
        ChronoIsoWeek { y, w: w as u32 }
    }
}

// ---- NaiveDate

/// `chrono::NaiveDate`: days since 1970-01-01.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ChronoNaiveDate {
    days: i32,
}

impl ChronoNaiveDate {
    pub const MIN: ChronoNaiveDate = ChronoNaiveDate { days: -96465658 };
    pub const MAX: ChronoNaiveDate = ChronoNaiveDate { days: 95026601 };

    fn __ok(days: i32) -> Option<ChronoNaiveDate> {
        if (-96465658..=95026601).contains(&days) {
            Some(ChronoNaiveDate { days })
        } else {
            None
        }
    }
    pub fn from_ymd_opt(year: i32, month: u32, day: u32) -> Option<ChronoNaiveDate> {
        if !(1..=12).contains(&month) || day < 1 || day > __chrono_month_len(year, month) {
            return None;
        }
        if !(-262143..=262142).contains(&year) {
            return None;
        }
        ChronoNaiveDate::__ok(__chrono_days_from_civil(year, month, day))
    }
    pub fn from_ymd(year: i32, month: u32, day: u32) -> ChronoNaiveDate {
        ChronoNaiveDate::from_ymd_opt(year, month, day).expect("invalid or out-of-range date")
    }
    pub fn from_yo_opt(year: i32, ordinal: u32) -> Option<ChronoNaiveDate> {
        let len = if __chrono_is_leap(year) { 366 } else { 365 };
        if ordinal < 1 || ordinal > len || !(-262143..=262142).contains(&year) {
            return None;
        }
        ChronoNaiveDate::__ok(__chrono_days_from_civil(year, 1, 1) + ordinal as i32 - 1)
    }
    pub fn from_isoywd_opt(year: i32, week: u32, weekday: ChronoWeekday) -> Option<ChronoNaiveDate> {
        if week < 1 || week > __chrono_weeks_in_year(year) {
            return None;
        }
        let jan4 = __chrono_days_from_civil(year, 1, 4);
        let monday = jan4 - (jan4 + 3).rem_euclid(7);
        ChronoNaiveDate::__ok(monday + (week as i32 - 1) * 7 + weekday.num_days_from_monday() as i32)
    }
    pub fn from_num_days_from_ce_opt(days: i32) -> Option<ChronoNaiveDate> {
        ChronoNaiveDate::__ok(days - 719163)
    }
    pub fn from_weekday_of_month_opt(year: i32, month: u32, weekday: ChronoWeekday, n: u8) -> Option<ChronoNaiveDate> {
        if n == 0 {
            return None;
        }
        let first = ChronoNaiveDate::from_ymd_opt(year, month, 1)?;
        let off = weekday.days_since(first.weekday());
        let day = 1 + off + (n as u32 - 1) * 7;
        ChronoNaiveDate::from_ymd_opt(year, month, day)
    }
    pub fn from_epoch_days(days: i32) -> Option<ChronoNaiveDate> {
        ChronoNaiveDate::__ok(days)
    }
    pub fn and_time(&self, time: ChronoNaiveTime) -> ChronoNaiveDateTime {
        ChronoNaiveDateTime { date: *self, time }
    }
    pub fn and_hms_opt(&self, hour: u32, min: u32, sec: u32) -> Option<ChronoNaiveDateTime> {
        Some(self.and_time(ChronoNaiveTime::from_hms_opt(hour, min, sec)?))
    }
    pub fn and_hms(&self, hour: u32, min: u32, sec: u32) -> ChronoNaiveDateTime {
        self.and_hms_opt(hour, min, sec).expect("invalid time")
    }
    pub fn and_hms_milli_opt(&self, hour: u32, min: u32, sec: u32, milli: u32) -> Option<ChronoNaiveDateTime> {
        Some(self.and_time(ChronoNaiveTime::from_hms_milli_opt(hour, min, sec, milli)?))
    }
    pub fn and_hms_micro_opt(&self, hour: u32, min: u32, sec: u32, micro: u32) -> Option<ChronoNaiveDateTime> {
        Some(self.and_time(ChronoNaiveTime::from_hms_micro_opt(hour, min, sec, micro)?))
    }
    pub fn and_hms_nano_opt(&self, hour: u32, min: u32, sec: u32, nano: u32) -> Option<ChronoNaiveDateTime> {
        Some(self.and_time(ChronoNaiveTime::from_hms_nano_opt(hour, min, sec, nano)?))
    }
    pub fn succ_opt(&self) -> Option<ChronoNaiveDate> {
        ChronoNaiveDate::__ok(self.days + 1)
    }
    pub fn pred_opt(&self) -> Option<ChronoNaiveDate> {
        ChronoNaiveDate::__ok(self.days - 1)
    }
    pub fn checked_add_signed(self, rhs: ChronoTimeDelta) -> Option<ChronoNaiveDate> {
        let days = rhs.num_days();
        if days.abs() > 200_000_000 {
            return None;
        }
        ChronoNaiveDate::__ok(self.days + days as i32)
    }
    pub fn checked_sub_signed(self, rhs: ChronoTimeDelta) -> Option<ChronoNaiveDate> {
        self.checked_add_signed(-rhs)
    }
    pub fn checked_add_days(self, days: ChronoDays) -> Option<ChronoNaiveDate> {
        if days.0 > 200_000_000 {
            return None;
        }
        ChronoNaiveDate::__ok(self.days + days.0 as i32)
    }
    pub fn checked_sub_days(self, days: ChronoDays) -> Option<ChronoNaiveDate> {
        if days.0 > 200_000_000 {
            return None;
        }
        ChronoNaiveDate::__ok(self.days - days.0 as i32)
    }
    fn __add_months(self, months: i64) -> Option<ChronoNaiveDate> {
        let (y, m, d) = __chrono_civil_from_days(self.days);
        let total = y as i64 * 12 + (m as i64 - 1) + months;
        let ny = total.div_euclid(12);
        if !(-262143..=262142).contains(&ny) {
            return None;
        }
        let ny = ny as i32;
        let nm = total.rem_euclid(12) as u32 + 1;
        let nd = d.min(__chrono_month_len(ny, nm));
        ChronoNaiveDate::from_ymd_opt(ny, nm, nd)
    }
    pub fn checked_add_months(self, months: ChronoMonths) -> Option<ChronoNaiveDate> {
        self.__add_months(months.0 as i64)
    }
    pub fn checked_sub_months(self, months: ChronoMonths) -> Option<ChronoNaiveDate> {
        self.__add_months(-(months.0 as i64))
    }
    pub fn signed_duration_since(self, rhs: ChronoNaiveDate) -> ChronoTimeDelta {
        ChronoTimeDelta::days((self.days - rhs.days) as i64)
    }
    pub fn years_since(&self, base: ChronoNaiveDate) -> Option<u32> {
        let mut years = self.year() - base.year();
        if (self.month() << 5 | self.day()) < (base.month() << 5 | base.day()) {
            years -= 1;
        }
        if years >= 0 {
            Some(years as u32)
        } else {
            None
        }
    }
    pub fn leap_year(&self) -> bool {
        __chrono_is_leap(self.year())
    }
    pub fn to_epoch_days(&self) -> i32 {
        self.days
    }
    pub fn iter_days(&self) -> impl Iterator<Item = ChronoNaiveDate> {
        let mut next = Some(*self);
        std::iter::from_fn(move || {
            let cur = next?;
            next = cur.succ_opt();
            Some(cur)
        })
    }
    pub fn iter_weeks(&self) -> impl Iterator<Item = ChronoNaiveDate> {
        let mut next = Some(*self);
        std::iter::from_fn(move || {
            let cur = next?;
            next = ChronoNaiveDate::__ok(cur.days + 7);
            Some(cur)
        })
    }
    pub fn week(&self, start: ChronoWeekday) -> ChronoNaiveWeek {
        ChronoNaiveWeek { date: *self, start }
    }
    pub fn format(&self, fmt: &str) -> String {
        __chrono_format(Some(*self), None, None, "", fmt)
    }
    pub fn parse_from_str(s: &str, fmt: &str) -> Result<ChronoNaiveDate, ChronoParseError> {
        __chrono_parse(s, fmt)?.date()
    }
    pub fn parse_and_remainder<'a>(s: &'a str, fmt: &str) -> Result<(ChronoNaiveDate, &'a str), ChronoParseError> {
        let (p, rest) = __chrono_parse_prefix(s, fmt)?;
        Ok((p.date()?, &s[s.len() - rest..]))
    }
}

impl Default for ChronoNaiveDate {
    fn default() -> ChronoNaiveDate {
        ChronoNaiveDate { days: 0 }
    }
}

impl ChronoDatelike for ChronoNaiveDate {
    fn __date(&self) -> ChronoNaiveDate {
        *self
    }
    fn __with_date(&self, d: ChronoNaiveDate) -> Option<ChronoNaiveDate> {
        Some(d)
    }
}

/// `chrono::NaiveWeek`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChronoNaiveWeek {
    date: ChronoNaiveDate,
    start: ChronoWeekday,
}

impl ChronoNaiveWeek {
    pub fn first_day(&self) -> ChronoNaiveDate {
        let back = self.date.weekday().days_since(self.start);
        ChronoNaiveDate { days: self.date.days - back as i32 }
    }
    pub fn last_day(&self) -> ChronoNaiveDate {
        ChronoNaiveDate { days: self.first_day().days + 6 }
    }
}

impl std::ops::Add<ChronoTimeDelta> for ChronoNaiveDate {
    type Output = ChronoNaiveDate;
    fn add(self, rhs: ChronoTimeDelta) -> ChronoNaiveDate {
        self.checked_add_signed(rhs).expect("`NaiveDate + TimeDelta` overflowed")
    }
}

impl std::ops::Sub<ChronoTimeDelta> for ChronoNaiveDate {
    type Output = ChronoNaiveDate;
    fn sub(self, rhs: ChronoTimeDelta) -> ChronoNaiveDate {
        self.checked_sub_signed(rhs).expect("`NaiveDate - TimeDelta` overflowed")
    }
}

impl std::ops::Sub<ChronoNaiveDate> for ChronoNaiveDate {
    type Output = ChronoTimeDelta;
    fn sub(self, rhs: ChronoNaiveDate) -> ChronoTimeDelta {
        self.signed_duration_since(rhs)
    }
}

impl std::ops::Add<ChronoDays> for ChronoNaiveDate {
    type Output = ChronoNaiveDate;
    fn add(self, days: ChronoDays) -> ChronoNaiveDate {
        self.checked_add_days(days).expect("`NaiveDate + Days` out of range")
    }
}

impl std::ops::Sub<ChronoDays> for ChronoNaiveDate {
    type Output = ChronoNaiveDate;
    fn sub(self, days: ChronoDays) -> ChronoNaiveDate {
        self.checked_sub_days(days).expect("`NaiveDate - Days` out of range")
    }
}

impl std::ops::Add<ChronoMonths> for ChronoNaiveDate {
    type Output = ChronoNaiveDate;
    fn add(self, months: ChronoMonths) -> ChronoNaiveDate {
        self.checked_add_months(months).expect("`NaiveDate + Months` out of range")
    }
}

impl std::ops::Sub<ChronoMonths> for ChronoNaiveDate {
    type Output = ChronoNaiveDate;
    fn sub(self, months: ChronoMonths) -> ChronoNaiveDate {
        self.checked_sub_months(months).expect("`NaiveDate - Months` out of range")
    }
}

impl std::ops::AddAssign<ChronoTimeDelta> for ChronoNaiveDate {
    fn add_assign(&mut self, rhs: ChronoTimeDelta) {
        *self = *self + rhs;
    }
}

impl std::ops::SubAssign<ChronoTimeDelta> for ChronoNaiveDate {
    fn sub_assign(&mut self, rhs: ChronoTimeDelta) {
        *self = *self - rhs;
    }
}

fn __chrono_write_year(f: &mut String, y: i32) {
    if (0..=9999).contains(&y) {
        f.push_str(&format!("{:04}", y));
    } else {
        f.push_str(&format!("{:+05}", y));
    }
}

impl std::fmt::Debug for ChronoNaiveDate {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let (y, m, d) = __chrono_civil_from_days(self.days);
        let mut s = String::new();
        __chrono_write_year(&mut s, y);
        write!(f, "{}-{:02}-{:02}", s, m, d)
    }
}

impl std::fmt::Display for ChronoNaiveDate {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let (y, m, d) = __chrono_civil_from_days(self.days);
        let mut s = String::new();
        __chrono_write_year(&mut s, y);
        f.pad(&format!("{}-{:02}-{:02}", s, m, d))
    }
}

impl std::str::FromStr for ChronoNaiveDate {
    type Err = ChronoParseError;
    fn from_str(s: &str) -> Result<ChronoNaiveDate, ChronoParseError> {
        __chrono_parse(s, "%Y-%m-%d")?.date()
    }
}

// ---- NaiveTime

/// `chrono::NaiveTime`: seconds from midnight and nanoseconds.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct ChronoNaiveTime {
    secs: u32,
    frac: u32,
}

impl ChronoNaiveTime {
    pub const MIN: ChronoNaiveTime = ChronoNaiveTime { secs: 0, frac: 0 };

    pub fn from_hms_opt(hour: u32, min: u32, sec: u32) -> Option<ChronoNaiveTime> {
        ChronoNaiveTime::from_hms_nano_opt(hour, min, sec, 0)
    }
    pub fn from_hms(hour: u32, min: u32, sec: u32) -> ChronoNaiveTime {
        ChronoNaiveTime::from_hms_opt(hour, min, sec).expect("invalid time")
    }
    pub fn from_hms_milli_opt(hour: u32, min: u32, sec: u32, milli: u32) -> Option<ChronoNaiveTime> {
        if milli >= 2000 {
            return None;
        }
        ChronoNaiveTime::from_hms_nano_opt(hour, min, sec, milli * 1_000_000)
    }
    pub fn from_hms_micro_opt(hour: u32, min: u32, sec: u32, micro: u32) -> Option<ChronoNaiveTime> {
        if micro >= 2_000_000 {
            return None;
        }
        ChronoNaiveTime::from_hms_nano_opt(hour, min, sec, micro * 1000)
    }
    pub fn from_hms_nano_opt(hour: u32, min: u32, sec: u32, nano: u32) -> Option<ChronoNaiveTime> {
        if hour >= 24 || min >= 60 || sec >= 60 || nano >= 1_000_000_000 {
            return None;
        }
        Some(ChronoNaiveTime { secs: hour * 3600 + min * 60 + sec, frac: nano })
    }
    pub fn from_num_seconds_from_midnight_opt(secs: u32, nano: u32) -> Option<ChronoNaiveTime> {
        if secs >= 86400 || nano >= 1_000_000_000 {
            return None;
        }
        Some(ChronoNaiveTime { secs, frac: nano })
    }
    fn __nanos(&self) -> i64 {
        self.secs as i64 * 1_000_000_000 + self.frac as i64
    }
    fn __of_nanos(n: i64) -> ChronoNaiveTime {
        let n = n.rem_euclid(86_400_000_000_000);
        ChronoNaiveTime {
            secs: (n / 1_000_000_000) as u32,
            frac: (n % 1_000_000_000) as u32,
        }
    }
    pub fn signed_duration_since(self, rhs: ChronoNaiveTime) -> ChronoTimeDelta {
        ChronoTimeDelta::nanoseconds(self.__nanos() - rhs.__nanos())
    }
    pub fn overflowing_add_signed(&self, rhs: ChronoTimeDelta) -> (ChronoNaiveTime, i64) {
        let total = self.__nanos() + rhs.secs * 1_000_000_000 + rhs.nanos as i64;
        let days = total.div_euclid(86_400_000_000_000);
        (ChronoNaiveTime::__of_nanos(total), days * 86400)
    }
    pub fn overflowing_sub_signed(&self, rhs: ChronoTimeDelta) -> (ChronoNaiveTime, i64) {
        let (t, d) = self.overflowing_add_signed(-rhs);
        (t, -d)
    }
    pub fn format(&self, fmt: &str) -> String {
        __chrono_format(None, Some(*self), None, "", fmt)
    }
    pub fn parse_from_str(s: &str, fmt: &str) -> Result<ChronoNaiveTime, ChronoParseError> {
        __chrono_parse(s, fmt)?.time()
    }
}

impl ChronoTimelike for ChronoNaiveTime {
    fn __time(&self) -> ChronoNaiveTime {
        *self
    }
    fn __with_time(&self, t: ChronoNaiveTime) -> Option<ChronoNaiveTime> {
        Some(t)
    }
}

impl std::ops::Add<ChronoTimeDelta> for ChronoNaiveTime {
    type Output = ChronoNaiveTime;
    fn add(self, rhs: ChronoTimeDelta) -> ChronoNaiveTime {
        self.overflowing_add_signed(rhs).0
    }
}

impl std::ops::Sub<ChronoTimeDelta> for ChronoNaiveTime {
    type Output = ChronoNaiveTime;
    fn sub(self, rhs: ChronoTimeDelta) -> ChronoNaiveTime {
        self.overflowing_add_signed(-rhs).0
    }
}

impl std::ops::Sub<ChronoNaiveTime> for ChronoNaiveTime {
    type Output = ChronoTimeDelta;
    fn sub(self, rhs: ChronoNaiveTime) -> ChronoTimeDelta {
        self.signed_duration_since(rhs)
    }
}

impl std::ops::AddAssign<ChronoTimeDelta> for ChronoNaiveTime {
    fn add_assign(&mut self, rhs: ChronoTimeDelta) {
        *self = *self + rhs;
    }
}

impl std::ops::SubAssign<ChronoTimeDelta> for ChronoNaiveTime {
    fn sub_assign(&mut self, rhs: ChronoTimeDelta) {
        *self = *self - rhs;
    }
}

fn __chrono_fraction(nano: u32) -> String {
    if nano == 0 {
        String::new()
    } else if nano % 1_000_000 == 0 {
        format!(".{:03}", nano / 1_000_000)
    } else if nano % 1000 == 0 {
        format!(".{:06}", nano / 1000)
    } else {
        format!(".{:09}", nano)
    }
}

fn __chrono_time_str(t: &ChronoNaiveTime) -> String {
    format!(
        "{:02}:{:02}:{:02}{}",
        t.secs / 3600,
        t.secs / 60 % 60,
        t.secs % 60,
        __chrono_fraction(t.frac)
    )
}

impl std::fmt::Debug for ChronoNaiveTime {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str(&__chrono_time_str(self))
    }
}

impl std::fmt::Display for ChronoNaiveTime {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.pad(&__chrono_time_str(self))
    }
}

impl std::str::FromStr for ChronoNaiveTime {
    type Err = ChronoParseError;
    fn from_str(s: &str) -> Result<ChronoNaiveTime, ChronoParseError> {
        match __chrono_parse(s, "%H:%M:%S%.f") {
            Ok(p) => p.time(),
            Err(e) => match __chrono_parse(s, "%H:%M") {
                Ok(p) => p.time(),
                Err(_) => Err(e),
            },
        }
    }
}

// ---- NaiveDateTime

/// `chrono::NaiveDateTime`.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct ChronoNaiveDateTime {
    date: ChronoNaiveDate,
    time: ChronoNaiveTime,
}

impl ChronoNaiveDateTime {
    pub const MIN: ChronoNaiveDateTime = ChronoNaiveDateTime {
        date: ChronoNaiveDate { days: -96465658 },
        time: ChronoNaiveTime { secs: 0, frac: 0 },
    };

    pub fn new(date: ChronoNaiveDate, time: ChronoNaiveTime) -> ChronoNaiveDateTime {
        ChronoNaiveDateTime { date, time }
    }
    fn __secs(&self) -> i64 {
        self.date.days as i64 * 86400 + self.time.secs as i64
    }
    fn __of_secs(secs: i64, nano: u32) -> Option<ChronoNaiveDateTime> {
        let days = secs.div_euclid(86400);
        if days.abs() > 200_000_000 {
            return None;
        }
        Some(ChronoNaiveDateTime {
            date: ChronoNaiveDate::__ok(days as i32)?,
            time: ChronoNaiveTime { secs: secs.rem_euclid(86400) as u32, frac: nano },
        })
    }
    pub fn from_timestamp_opt(secs: i64, nsecs: u32) -> Option<ChronoNaiveDateTime> {
        if nsecs >= 1_000_000_000 {
            return None;
        }
        ChronoNaiveDateTime::__of_secs(secs, nsecs)
    }
    pub fn from_timestamp_millis(ms: i64) -> Option<ChronoNaiveDateTime> {
        ChronoNaiveDateTime::__of_secs(ms.div_euclid(1000), (ms.rem_euclid(1000) * 1_000_000) as u32)
    }
    pub fn date(&self) -> ChronoNaiveDate {
        self.date
    }
    pub fn time(&self) -> ChronoNaiveTime {
        self.time
    }
    pub fn timestamp(&self) -> i64 {
        self.__secs()
    }
    pub fn timestamp_millis(&self) -> i64 {
        self.__secs() * 1000 + (self.time.frac / 1_000_000) as i64
    }
    pub fn timestamp_micros(&self) -> i64 {
        self.__secs() * 1_000_000 + (self.time.frac / 1000) as i64
    }
    pub fn timestamp_subsec_millis(&self) -> u32 {
        self.time.frac / 1_000_000
    }
    pub fn and_utc(&self) -> ChronoDateTime<ChronoUtc> {
        ChronoDateTime { utc: *self, off: 0, tz: ChronoUtc }
    }
    pub fn and_local_timezone<Tz: ChronoTimeZone>(&self, tz: Tz) -> ChronoLocalResult<ChronoDateTime<Tz>> {
        tz.from_local_datetime(self)
    }
    pub fn checked_add_signed(self, rhs: ChronoTimeDelta) -> Option<ChronoNaiveDateTime> {
        let nanos = self.time.frac as i64 + rhs.nanos as i64;
        ChronoNaiveDateTime::__of_secs(
            self.__secs() + rhs.secs + nanos.div_euclid(1_000_000_000),
            nanos.rem_euclid(1_000_000_000) as u32,
        )
    }
    pub fn checked_sub_signed(self, rhs: ChronoTimeDelta) -> Option<ChronoNaiveDateTime> {
        self.checked_add_signed(-rhs)
    }
    pub fn checked_add_days(self, days: ChronoDays) -> Option<ChronoNaiveDateTime> {
        Some(ChronoNaiveDateTime { date: self.date.checked_add_days(days)?, time: self.time })
    }
    pub fn checked_sub_days(self, days: ChronoDays) -> Option<ChronoNaiveDateTime> {
        Some(ChronoNaiveDateTime { date: self.date.checked_sub_days(days)?, time: self.time })
    }
    pub fn checked_add_months(self, months: ChronoMonths) -> Option<ChronoNaiveDateTime> {
        Some(ChronoNaiveDateTime { date: self.date.checked_add_months(months)?, time: self.time })
    }
    pub fn checked_sub_months(self, months: ChronoMonths) -> Option<ChronoNaiveDateTime> {
        Some(ChronoNaiveDateTime { date: self.date.checked_sub_months(months)?, time: self.time })
    }
    pub fn signed_duration_since(self, rhs: ChronoNaiveDateTime) -> ChronoTimeDelta {
        ChronoTimeDelta::__norm(self.__secs() - rhs.__secs(), self.time.frac as i64 - rhs.time.frac as i64)
    }
    pub fn format(&self, fmt: &str) -> String {
        __chrono_format(Some(self.date), Some(self.time), None, "", fmt)
    }
    pub fn parse_from_str(s: &str, fmt: &str) -> Result<ChronoNaiveDateTime, ChronoParseError> {
        __chrono_parse(s, fmt)?.datetime()
    }
}

impl ChronoDatelike for ChronoNaiveDateTime {
    fn __date(&self) -> ChronoNaiveDate {
        self.date
    }
    fn __with_date(&self, d: ChronoNaiveDate) -> Option<ChronoNaiveDateTime> {
        Some(ChronoNaiveDateTime { date: d, time: self.time })
    }
}

impl ChronoTimelike for ChronoNaiveDateTime {
    fn __time(&self) -> ChronoNaiveTime {
        self.time
    }
    fn __with_time(&self, t: ChronoNaiveTime) -> Option<ChronoNaiveDateTime> {
        Some(ChronoNaiveDateTime { date: self.date, time: t })
    }
}

impl std::ops::Add<ChronoTimeDelta> for ChronoNaiveDateTime {
    type Output = ChronoNaiveDateTime;
    fn add(self, rhs: ChronoTimeDelta) -> ChronoNaiveDateTime {
        self.checked_add_signed(rhs).expect("`NaiveDateTime + TimeDelta` overflowed")
    }
}

impl std::ops::Sub<ChronoTimeDelta> for ChronoNaiveDateTime {
    type Output = ChronoNaiveDateTime;
    fn sub(self, rhs: ChronoTimeDelta) -> ChronoNaiveDateTime {
        self.checked_sub_signed(rhs).expect("`NaiveDateTime - TimeDelta` overflowed")
    }
}

impl std::ops::Sub<ChronoNaiveDateTime> for ChronoNaiveDateTime {
    type Output = ChronoTimeDelta;
    fn sub(self, rhs: ChronoNaiveDateTime) -> ChronoTimeDelta {
        self.signed_duration_since(rhs)
    }
}

impl std::ops::Add<ChronoDays> for ChronoNaiveDateTime {
    type Output = ChronoNaiveDateTime;
    fn add(self, days: ChronoDays) -> ChronoNaiveDateTime {
        self.checked_add_days(days).expect("`NaiveDateTime + Days` out of range")
    }
}

impl std::ops::Add<ChronoMonths> for ChronoNaiveDateTime {
    type Output = ChronoNaiveDateTime;
    fn add(self, months: ChronoMonths) -> ChronoNaiveDateTime {
        self.checked_add_months(months).expect("`NaiveDateTime + Months` out of range")
    }
}

impl std::ops::AddAssign<ChronoTimeDelta> for ChronoNaiveDateTime {
    fn add_assign(&mut self, rhs: ChronoTimeDelta) {
        *self = *self + rhs;
    }
}

impl std::ops::SubAssign<ChronoTimeDelta> for ChronoNaiveDateTime {
    fn sub_assign(&mut self, rhs: ChronoTimeDelta) {
        *self = *self - rhs;
    }
}

impl std::fmt::Debug for ChronoNaiveDateTime {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{:?}T{:?}", self.date, self.time)
    }
}

impl std::fmt::Display for ChronoNaiveDateTime {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.pad(&format!("{} {}", self.date, self.time))
    }
}

impl std::str::FromStr for ChronoNaiveDateTime {
    type Err = ChronoParseError;
    fn from_str(s: &str) -> Result<ChronoNaiveDateTime, ChronoParseError> {
        __chrono_parse(s, "%Y-%m-%dT%H:%M:%S%.f")?.datetime()
    }
}

// ---- time zones

/// `chrono::LocalResult` / `MappedLocalTime`: a local time can be missing or ambiguous.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ChronoLocalResult<T> {
    Single(T),
    Ambiguous(T, T),
    None,
}

impl<T: std::fmt::Debug + Copy> ChronoLocalResult<T> {
    pub fn single(self) -> Option<T> {
        match self {
            ChronoLocalResult::Single(t) => Some(t),
            _ => None,
        }
    }
    pub fn earliest(self) -> Option<T> {
        match self {
            ChronoLocalResult::Single(t) => Some(t),
            ChronoLocalResult::Ambiguous(t, _) => Some(t),
            ChronoLocalResult::None => None,
        }
    }
    pub fn latest(self) -> Option<T> {
        match self {
            ChronoLocalResult::Single(t) => Some(t),
            ChronoLocalResult::Ambiguous(_, t) => Some(t),
            ChronoLocalResult::None => None,
        }
    }
    pub fn unwrap(self) -> T {
        match self {
            ChronoLocalResult::None => panic!("No such local time"),
            ChronoLocalResult::Single(t) => t,
            ChronoLocalResult::Ambiguous(a, b) => {
                panic!("Ambiguous local time, ranging from {:?} to {:?}", a, b)
            }
        }
    }
}

/// `chrono::TimeZone`: `Utc`, `Local` and `FixedOffset`.
pub trait ChronoTimeZone {
    fn __offset_at_utc(&self, utc_secs: i64) -> i32;
    fn __is_utc(&self) -> bool;
    fn __of_offset(off: i32) -> Self;
    /// How `{}` and `%Z` name the zone: `UTC`, `+01:00`, `BST`.
    fn __display_name(&self, off: i32, _utc_secs: i64) -> String {
        if self.__is_utc() {
            "UTC".to_string()
        } else {
            __chrono_offset_str(off, true, false)
        }
    }
    /// How `{:?}` names it: `Z`, `+01:00`, `BST`.
    fn __debug_name(&self, off: i32, _utc_secs: i64) -> String {
        if self.__is_utc() {
            "Z".to_string()
        } else {
            __chrono_offset_str(off, true, false)
        }
    }
    fn __offset_at_local(&self, local_secs: i64) -> ChronoLocalResult<i32> {
        let a = self.__offset_at_utc(local_secs - 86400);
        let b = self.__offset_at_utc(local_secs + 86400);
        let fits_a = self.__offset_at_utc(local_secs - a as i64) == a;
        let fits_b = a != b && self.__offset_at_utc(local_secs - b as i64) == b;
        match (fits_a, fits_b) {
            (true, true) => {
                if a > b {
                    ChronoLocalResult::Ambiguous(a, b)
                } else {
                    ChronoLocalResult::Ambiguous(b, a)
                }
            }
            (true, false) => ChronoLocalResult::Single(a),
            (false, true) => ChronoLocalResult::Single(b),
            (false, false) => ChronoLocalResult::None,
        }
    }
    fn from_utc_datetime(&self, utc: &ChronoNaiveDateTime) -> ChronoDateTime<Self>
    where
        Self: Sized,
    {
        ChronoDateTime { utc: *utc, off: self.__offset_at_utc(utc.__secs()), tz: *self }
    }
    fn from_local_datetime(&self, local: &ChronoNaiveDateTime) -> ChronoLocalResult<ChronoDateTime<Self>>
    where
        Self: Sized,
    {
        let at = |off: i32| ChronoDateTime {
            utc: *local - ChronoTimeDelta::seconds(off as i64),
            off,
            tz: *self,
        };
        match self.__offset_at_local(local.__secs()) {
            ChronoLocalResult::Single(o) => ChronoLocalResult::Single(at(o)),
            ChronoLocalResult::Ambiguous(o1, o2) => ChronoLocalResult::Ambiguous(at(o1), at(o2)),
            ChronoLocalResult::None => ChronoLocalResult::None,
        }
    }
    fn with_ymd_and_hms(&self, year: i32, month: u32, day: u32, hour: u32, min: u32, sec: u32) -> ChronoLocalResult<ChronoDateTime<Self>>
    where
        Self: Sized,
    {
        match ChronoNaiveDate::from_ymd_opt(year, month, day).and_then(|d| d.and_hms_opt(hour, min, sec)) {
            Some(dt) => self.from_local_datetime(&dt),
            None => ChronoLocalResult::None,
        }
    }
    fn timestamp_opt(&self, secs: i64, nsecs: u32) -> ChronoLocalResult<ChronoDateTime<Self>>
    where
        Self: Sized,
    {
        match ChronoNaiveDateTime::from_timestamp_opt(secs, nsecs) {
            Some(dt) => ChronoLocalResult::Single(self.from_utc_datetime(&dt)),
            None => ChronoLocalResult::None,
        }
    }
    fn timestamp_millis_opt(&self, ms: i64) -> ChronoLocalResult<ChronoDateTime<Self>>
    where
        Self: Sized,
    {
        match ChronoNaiveDateTime::from_timestamp_millis(ms) {
            Some(dt) => ChronoLocalResult::Single(self.from_utc_datetime(&dt)),
            None => ChronoLocalResult::None,
        }
    }
    fn timestamp_nanos(&self, nanos: i64) -> ChronoDateTime<Self>
    where
        Self: Sized,
    {
        let dt = ChronoNaiveDateTime::__of_secs(nanos.div_euclid(1_000_000_000), nanos.rem_euclid(1_000_000_000) as u32).unwrap();
        self.from_utc_datetime(&dt)
    }
}

/// `chrono::Utc`.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct ChronoUtc;

impl ChronoUtc {
    pub fn now() -> ChronoDateTime<ChronoUtc> {
        let ms = js::Date::now();
        ChronoUtc.timestamp_millis_opt(ms as i64).unwrap()
    }
}

impl ChronoTimeZone for ChronoUtc {
    fn __offset_at_utc(&self, _utc_secs: i64) -> i32 {
        0
    }
    fn __is_utc(&self) -> bool {
        true
    }
    fn __of_offset(_off: i32) -> ChronoUtc {
        ChronoUtc
    }
}

impl std::fmt::Debug for ChronoUtc {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "Utc")
    }
}

impl std::fmt::Display for ChronoUtc {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "UTC")
    }
}

/// `chrono::Local`: the JS host's time zone.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct ChronoLocal;

impl ChronoLocal {
    pub fn now() -> ChronoDateTime<ChronoLocal> {
        let ms = js::Date::now();
        ChronoLocal.timestamp_millis_opt(ms as i64).unwrap()
    }
}

impl ChronoTimeZone for ChronoLocal {
    fn __offset_at_utc(&self, utc_secs: i64) -> i32 {
        -js::Date::from_millis(utc_secs as f64 * 1000.0).get_timezone_offset() * 60
    }
    fn __is_utc(&self) -> bool {
        false
    }
    fn __of_offset(_off: i32) -> ChronoLocal {
        ChronoLocal
    }
}

impl std::fmt::Debug for ChronoLocal {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "Local")
    }
}

/// `chrono::FixedOffset`: seconds east of UTC.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct ChronoFixedOffset {
    secs: i32,
}

impl ChronoFixedOffset {
    pub fn east_opt(secs: i32) -> Option<ChronoFixedOffset> {
        if -86400 < secs && secs < 86400 {
            Some(ChronoFixedOffset { secs })
        } else {
            None
        }
    }
    pub fn west_opt(secs: i32) -> Option<ChronoFixedOffset> {
        ChronoFixedOffset::east_opt(-secs)
    }
    pub fn east(secs: i32) -> ChronoFixedOffset {
        ChronoFixedOffset::east_opt(secs).expect("FixedOffset::east out of bounds")
    }
    pub fn west(secs: i32) -> ChronoFixedOffset {
        ChronoFixedOffset::west_opt(secs).expect("FixedOffset::west out of bounds")
    }
    pub fn local_minus_utc(&self) -> i32 {
        self.secs
    }
    pub fn utc_minus_local(&self) -> i32 {
        -self.secs
    }
    /// `Offset::fix`.
    pub fn fix(&self) -> ChronoFixedOffset {
        *self
    }
}

impl ChronoTimeZone for ChronoFixedOffset {
    fn __offset_at_utc(&self, _utc_secs: i64) -> i32 {
        self.secs
    }
    fn __is_utc(&self) -> bool {
        false
    }
    fn __of_offset(off: i32) -> ChronoFixedOffset {
        ChronoFixedOffset { secs: off }
    }
}

fn __chrono_offset_str(off: i32, colon: bool, secs: bool) -> String {
    let (sign, a) = if off < 0 { ('-', -off) } else { ('+', off) };
    let (h, m, s) = (a / 3600, a / 60 % 60, a % 60);
    let sep = if colon { ":" } else { "" };
    if secs || s != 0 && colon {
        format!("{}{:02}{}{:02}{}{:02}", sign, h, sep, m, sep, s)
    } else {
        format!("{}{:02}{}{:02}", sign, h, sep, m)
    }
}

impl std::fmt::Debug for ChronoFixedOffset {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str(&__chrono_offset_str(self.secs, true, false))
    }
}

impl std::fmt::Display for ChronoFixedOffset {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str(&__chrono_offset_str(self.secs, true, false))
    }
}

impl std::str::FromStr for ChronoFixedOffset {
    type Err = ChronoParseError;
    fn from_str(s: &str) -> Result<ChronoFixedOffset, ChronoParseError> {
        let p = __chrono_parse(s, "%z")?;
        Ok(ChronoFixedOffset { secs: p.offset.unwrap_or(0) })
    }
}

/// What `DateTime::offset()` gives: the offset, printed as its zone prints it (`Utc`'s `Z` /
/// `UTC`, a `FixedOffset`'s `+01:00`, a chrono-tz zone's `BST`).
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct ChronoOffset {
    secs: i32,
    dbg: String,
    disp: String,
}

impl ChronoOffset {
    pub fn fix(&self) -> ChronoFixedOffset {
        ChronoFixedOffset { secs: self.secs }
    }
    pub fn local_minus_utc(&self) -> i32 {
        self.secs
    }
    pub fn utc_minus_local(&self) -> i32 {
        -self.secs
    }
}

impl std::fmt::Debug for ChronoOffset {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str(&self.dbg)
    }
}

impl std::fmt::Display for ChronoOffset {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str(&self.disp)
    }
}

// ---- chrono-tz

/// `chrono_tz::Tz`: an IANA time zone, by name, with offsets and abbreviations from the JS
/// host's `Intl` data.
/// (The zone is its number in the runtime's list of names, so it's `Copy`.)
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ChronoTz {
    id: u32,
}

impl ChronoTz {
    pub const UTC: ChronoTz = ChronoTz { id: 0 };

    pub fn __of_name(name: &str) -> ChronoTz {
        ChronoTz { id: js::__tz_intern(name) }
    }
    pub fn name(&self) -> String {
        js::__tz_name(self.id)
    }
}

impl ChronoTimeZone for ChronoTz {
    fn __offset_at_utc(&self, utc_secs: i64) -> i32 {
        js::__tz_offset(&self.name(), utc_secs as f64 * 1000.0)
    }
    fn __is_utc(&self) -> bool {
        false
    }
    fn __of_offset(_off: i32) -> ChronoTz {
        ChronoTz::UTC
    }
    fn __display_name(&self, _off: i32, utc_secs: i64) -> String {
        js::__tz_abbr(&self.name(), utc_secs as f64 * 1000.0)
    }
    fn __debug_name(&self, _off: i32, utc_secs: i64) -> String {
        js::__tz_abbr(&self.name(), utc_secs as f64 * 1000.0)
    }
}

impl std::fmt::Debug for ChronoTz {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str(&self.name())
    }
}

impl std::fmt::Display for ChronoTz {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.pad(&self.name())
    }
}

impl std::str::FromStr for ChronoTz {
    type Err = ChronoTzParseError;
    fn from_str(s: &str) -> Result<ChronoTz, ChronoTzParseError> {
        if js::__tz_valid(s) {
            Ok(ChronoTz::__of_name(s))
        } else {
            Err(ChronoTzParseError {})
        }
    }
}

/// `chrono_tz::ParseError`.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct ChronoTzParseError {}

impl std::fmt::Debug for ChronoTzParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str("ParseError(())")
    }
}

impl std::fmt::Display for ChronoTzParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str("failed to parse timezone")
    }
}

impl std::error::Error for ChronoTzParseError {}

// ---- DateTime

/// `chrono::SecondsFormat`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ChronoSecondsFormat {
    Secs,
    Millis,
    Micros,
    Nanos,
    AutoSi,
}

/// `chrono::DateTime<Tz>`: a UTC moment, its offset in `Tz` and the zone.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ChronoDateTime<Tz> {
    utc: ChronoNaiveDateTime,
    off: i32,
    tz: Tz,
}

impl ChronoDateTime<ChronoUtc> {
    pub const UNIX_EPOCH: ChronoDateTime<ChronoUtc> = ChronoDateTime {
        utc: ChronoNaiveDateTime {
            date: ChronoNaiveDate { days: 0 },
            time: ChronoNaiveTime { secs: 0, frac: 0 },
        },
        off: 0,
        tz: ChronoUtc,
    };

    pub fn from_timestamp(secs: i64, nsecs: u32) -> Option<ChronoDateTime<ChronoUtc>> {
        Some(ChronoNaiveDateTime::from_timestamp_opt(secs, nsecs)?.and_utc())
    }
    pub fn from_timestamp_millis(ms: i64) -> Option<ChronoDateTime<ChronoUtc>> {
        Some(ChronoNaiveDateTime::from_timestamp_millis(ms)?.and_utc())
    }
    pub fn from_timestamp_secs(secs: i64) -> Option<ChronoDateTime<ChronoUtc>> {
        ChronoDateTime::from_timestamp(secs, 0)
    }
}

impl ChronoDateTime<ChronoFixedOffset> {
    pub fn parse_from_rfc3339(s: &str) -> Result<ChronoDateTime<ChronoFixedOffset>, ChronoParseError> {
        __chrono_parse_rfc3339(s)
    }
    pub fn parse_from_rfc2822(s: &str) -> Result<ChronoDateTime<ChronoFixedOffset>, ChronoParseError> {
        __chrono_parse(s.trim(), "%a, %d %b %Y %H:%M:%S %z")?.zoned()
    }
    pub fn parse_from_str(s: &str, fmt: &str) -> Result<ChronoDateTime<ChronoFixedOffset>, ChronoParseError> {
        __chrono_parse(s, fmt)?.zoned()
    }
}

impl<Tz: ChronoTimeZone + Copy> ChronoDateTime<Tz> {
    pub fn from_naive_utc_and_offset(utc: ChronoNaiveDateTime, tz: Tz) -> ChronoDateTime<Tz> {
        ChronoDateTime { utc, off: tz.__offset_at_utc(utc.__secs()), tz }
    }
    fn __local(&self) -> ChronoNaiveDateTime {
        self.utc + ChronoTimeDelta::seconds(self.off as i64)
    }
    pub fn naive_utc(&self) -> ChronoNaiveDateTime {
        self.utc
    }
    pub fn naive_local(&self) -> ChronoNaiveDateTime {
        self.__local()
    }
    pub fn date_naive(&self) -> ChronoNaiveDate {
        self.__local().date
    }
    pub fn time(&self) -> ChronoNaiveTime {
        self.__local().time
    }
    pub fn timestamp(&self) -> i64 {
        self.utc.timestamp()
    }
    pub fn timestamp_millis(&self) -> i64 {
        self.utc.timestamp_millis()
    }
    pub fn timestamp_micros(&self) -> i64 {
        self.utc.timestamp_micros()
    }
    pub fn timestamp_nanos_opt(&self) -> Option<i64> {
        Some(self.utc.timestamp() * 1_000_000_000 + self.utc.time.frac as i64)
    }
    pub fn timestamp_subsec_millis(&self) -> u32 {
        self.utc.time.frac / 1_000_000
    }
    pub fn timestamp_subsec_micros(&self) -> u32 {
        self.utc.time.frac / 1000
    }
    pub fn timestamp_subsec_nanos(&self) -> u32 {
        self.utc.time.frac
    }
    pub fn offset(&self) -> ChronoOffset {
        let at = self.utc.__secs();
        ChronoOffset {
            secs: self.off,
            dbg: self.tz.__debug_name(self.off, at),
            disp: self.tz.__display_name(self.off, at),
        }
    }
    pub fn timezone(&self) -> Tz {
        self.tz
    }
    pub fn with_timezone<Tz2: ChronoTimeZone + Copy>(&self, tz: &Tz2) -> ChronoDateTime<Tz2> {
        tz.from_utc_datetime(&self.utc)
    }
    pub fn fixed_offset(&self) -> ChronoDateTime<ChronoFixedOffset> {
        ChronoDateTime { utc: self.utc, off: self.off, tz: ChronoFixedOffset { secs: self.off } }
    }
    pub fn to_utc(&self) -> ChronoDateTime<ChronoUtc> {
        ChronoDateTime { utc: self.utc, off: 0, tz: ChronoUtc }
    }
    fn __moved(&self, utc: ChronoNaiveDateTime) -> ChronoDateTime<Tz> {
        ChronoDateTime { utc, off: self.tz.__offset_at_utc(utc.__secs()), tz: self.tz }
    }
    fn __relocal(&self, local: ChronoNaiveDateTime) -> Option<ChronoDateTime<Tz>> {
        self.tz.from_local_datetime(&local).single()
    }
    pub fn checked_add_signed(self, rhs: ChronoTimeDelta) -> Option<ChronoDateTime<Tz>> {
        Some(self.__moved(self.utc.checked_add_signed(rhs)?))
    }
    pub fn checked_sub_signed(self, rhs: ChronoTimeDelta) -> Option<ChronoDateTime<Tz>> {
        Some(self.__moved(self.utc.checked_sub_signed(rhs)?))
    }
    pub fn checked_add_days(self, days: ChronoDays) -> Option<ChronoDateTime<Tz>> {
        self.__relocal(self.__local().checked_add_days(days)?)
    }
    pub fn checked_sub_days(self, days: ChronoDays) -> Option<ChronoDateTime<Tz>> {
        self.__relocal(self.__local().checked_sub_days(days)?)
    }
    pub fn checked_add_months(self, months: ChronoMonths) -> Option<ChronoDateTime<Tz>> {
        self.__relocal(self.__local().checked_add_months(months)?)
    }
    pub fn checked_sub_months(self, months: ChronoMonths) -> Option<ChronoDateTime<Tz>> {
        self.__relocal(self.__local().checked_sub_months(months)?)
    }
    pub fn signed_duration_since<Tz2: ChronoTimeZone + Copy>(self, rhs: ChronoDateTime<Tz2>) -> ChronoTimeDelta {
        self.utc.signed_duration_since(rhs.utc)
    }
    pub fn years_since(&self, base: ChronoDateTime<Tz>) -> Option<u32> {
        let mut years = self.year() - base.year();
        let a = (self.month(), self.day(), self.time());
        let b = (base.month(), base.day(), base.time());
        if a < b {
            years -= 1;
        }
        if years >= 0 {
            Some(years as u32)
        } else {
            None
        }
    }
    pub fn format(&self, fmt: &str) -> String {
        let l = self.__local();
        let zone = self.tz.__display_name(self.off, self.utc.__secs());
        __chrono_format(Some(l.date), Some(l.time), Some(self.off), &zone, fmt)
    }
    pub fn to_rfc3339(&self) -> String {
        self.format("%Y-%m-%dT%H:%M:%S%.f%:z")
    }
    pub fn to_rfc3339_opts(&self, secform: ChronoSecondsFormat, use_z: bool) -> String {
        let frac = match secform {
            ChronoSecondsFormat::Secs => "",
            ChronoSecondsFormat::Millis => "%.3f",
            ChronoSecondsFormat::Micros => "%.6f",
            ChronoSecondsFormat::Nanos => "%.9f",
            ChronoSecondsFormat::AutoSi => {
                let n = self.utc.time.frac;
                if n == 0 {
                    ""
                } else if n % 1_000_000 == 0 {
                    "%.3f"
                } else if n % 1000 == 0 {
                    "%.6f"
                } else {
                    "%.9f"
                }
            }
        };
        let mut s = self.format(&format!("%Y-%m-%dT%H:%M:%S{}", frac));
        if use_z && self.off == 0 {
            s.push('Z');
        } else {
            s.push_str(&__chrono_offset_str(self.off, true, false));
        }
        s
    }
    pub fn to_rfc2822(&self) -> String {
        self.format("%a, %-d %b %Y %H:%M:%S %z")
    }
}

impl<Tz: ChronoTimeZone + Copy> ChronoDatelike for ChronoDateTime<Tz> {
    fn __date(&self) -> ChronoNaiveDate {
        self.__local().date
    }
    fn __with_date(&self, d: ChronoNaiveDate) -> Option<ChronoDateTime<Tz>> {
        self.__relocal(ChronoNaiveDateTime { date: d, time: self.__local().time })
    }
}

impl<Tz: ChronoTimeZone + Copy> ChronoTimelike for ChronoDateTime<Tz> {
    fn __time(&self) -> ChronoNaiveTime {
        self.__local().time
    }
    fn __with_time(&self, t: ChronoNaiveTime) -> Option<ChronoDateTime<Tz>> {
        self.__relocal(ChronoNaiveDateTime { date: self.__local().date, time: t })
    }
}

impl<Tz: ChronoTimeZone + Copy> std::ops::Add<ChronoTimeDelta> for ChronoDateTime<Tz> {
    type Output = ChronoDateTime<Tz>;
    fn add(self, rhs: ChronoTimeDelta) -> ChronoDateTime<Tz> {
        self.checked_add_signed(rhs).expect("`DateTime + TimeDelta` overflowed")
    }
}

impl<Tz: ChronoTimeZone + Copy> std::ops::Sub<ChronoTimeDelta> for ChronoDateTime<Tz> {
    type Output = ChronoDateTime<Tz>;
    fn sub(self, rhs: ChronoTimeDelta) -> ChronoDateTime<Tz> {
        self.checked_sub_signed(rhs).expect("`DateTime - TimeDelta` overflowed")
    }
}

impl<Tz: ChronoTimeZone + Copy> std::ops::Sub<ChronoDateTime<Tz>> for ChronoDateTime<Tz> {
    type Output = ChronoTimeDelta;
    fn sub(self, rhs: ChronoDateTime<Tz>) -> ChronoTimeDelta {
        self.utc.signed_duration_since(rhs.utc)
    }
}

impl<Tz: ChronoTimeZone + Copy> std::ops::Add<ChronoDays> for ChronoDateTime<Tz> {
    type Output = ChronoDateTime<Tz>;
    fn add(self, days: ChronoDays) -> ChronoDateTime<Tz> {
        self.checked_add_days(days).expect("`DateTime + Days` out of range")
    }
}

impl<Tz: ChronoTimeZone + Copy> std::ops::Add<ChronoMonths> for ChronoDateTime<Tz> {
    type Output = ChronoDateTime<Tz>;
    fn add(self, months: ChronoMonths) -> ChronoDateTime<Tz> {
        self.checked_add_months(months).expect("`DateTime + Months` out of range")
    }
}

impl<Tz: ChronoTimeZone + Copy> std::ops::AddAssign<ChronoTimeDelta> for ChronoDateTime<Tz> {
    fn add_assign(&mut self, rhs: ChronoTimeDelta) {
        *self = *self + rhs;
    }
}

impl<Tz: ChronoTimeZone + Copy> std::ops::SubAssign<ChronoTimeDelta> for ChronoDateTime<Tz> {
    fn sub_assign(&mut self, rhs: ChronoTimeDelta) {
        *self = *self - rhs;
    }
}

impl<Tz: ChronoTimeZone + Copy> std::fmt::Debug for ChronoDateTime<Tz> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let z = self.tz.__debug_name(self.off, self.utc.__secs());
        write!(f, "{:?}{}", self.__local(), z)
    }
}

impl<Tz: ChronoTimeZone + Copy> std::fmt::Display for ChronoDateTime<Tz> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let z = self.tz.__display_name(self.off, self.utc.__secs());
        f.pad(&format!("{} {}", self.__local(), z))
    }
}

impl<Tz: ChronoTimeZone + Copy> std::str::FromStr for ChronoDateTime<Tz> {
    type Err = ChronoParseError;
    fn from_str(s: &str) -> Result<ChronoDateTime<Tz>, ChronoParseError> {
        let d = __chrono_parse_rfc3339_relaxed(s)?;
        let tz = Tz::__of_offset(d.off);
        Ok(ChronoDateTime { utc: d.utc, off: tz.__offset_at_utc(d.utc.__secs()), tz })
    }
}

// ---- ParseError

/// `chrono::format::ParseErrorKind`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ChronoParseErrorKind {
    OutOfRange,
    Impossible,
    NotEnough,
    Invalid,
    TooShort,
    TooLong,
    BadFormat,
}

/// `chrono::ParseError`.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct ChronoParseError(ChronoParseErrorKind);

impl ChronoParseError {
    pub fn kind(&self) -> ChronoParseErrorKind {
        self.0
    }
}

impl std::fmt::Debug for ChronoParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "ParseError({:?})", self.0)
    }
}

impl std::fmt::Display for ChronoParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str(match self.0 {
            ChronoParseErrorKind::OutOfRange => "input is out of range",
            ChronoParseErrorKind::Impossible => "no possible date and time matching input",
            ChronoParseErrorKind::NotEnough => "input is not enough for unique date and time",
            ChronoParseErrorKind::Invalid => "input contains invalid characters",
            ChronoParseErrorKind::TooShort => "premature end of input",
            ChronoParseErrorKind::TooLong => "trailing input",
            ChronoParseErrorKind::BadFormat => "bad or unsupported format string",
        })
    }
}

impl std::error::Error for ChronoParseError {}

// ---- formatting

fn __chrono_num(out: &mut String, v: i64, width: usize, pad: char, modifier: char) {
    let pad = match modifier {
        '-' => ' ',
        '_' => ' ',
        '0' => '0',
        _ => pad,
    };
    let s = v.abs().to_string();
    if v < 0 {
        out.push('-');
    }
    if modifier != '-' {
        for _ in s.len()..width {
            out.push(pad);
        }
    }
    out.push_str(&s);
}

fn __chrono_format(
    date: Option<ChronoNaiveDate>,
    time: Option<ChronoNaiveTime>,
    off: Option<i32>,
    zname: &str,
    fmt: &str,
) -> String {
    let mut out = String::new();
    let chars: Vec<char> = fmt.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        i += 1;
        if c != '%' {
            out.push(c);
            continue;
        }
        let mut modifier = ' ';
        if i < chars.len() && (chars[i] == '-' || chars[i] == '_' || chars[i] == '0') {
            modifier = chars[i];
            i += 1;
        }
        let mut spec = String::new();
        // `%.3f`, `%3f`, `%:z`, `%::z`, `%.f`.
        while i < chars.len() && (chars[i] == '.' || chars[i] == ':' || chars[i] == '#' || chars[i].is_ascii_digit()) {
            spec.push(chars[i]);
            i += 1;
        }
        if i >= chars.len() {
            out.push('%');
            out.push_str(&spec);
            break;
        }
        spec.push(chars[i]);
        i += 1;
        let d = date.unwrap_or(ChronoNaiveDate { days: 0 });
        let t = time.unwrap_or(ChronoNaiveTime { secs: 0, frac: 0 });
        let (y, m, dd) = __chrono_civil_from_days(d.days);
        let wd = d.weekday();
        let hour = t.secs / 3600;
        let min = t.secs / 60 % 60;
        let sec = t.secs % 60;
        let o = off.unwrap_or(0);
        match spec.as_str() {
            "Y" => {
                if (0..=9999).contains(&y) || modifier != ' ' {
                    __chrono_num(&mut out, y as i64, 4, '0', modifier);
                } else {
                    out.push_str(&format!("{:+05}", y));
                }
            }
            "C" => __chrono_num(&mut out, y.div_euclid(100) as i64, 2, '0', modifier),
            "y" => __chrono_num(&mut out, y.rem_euclid(100) as i64, 2, '0', modifier),
            "m" => __chrono_num(&mut out, m as i64, 2, '0', modifier),
            "b" | "h" => out.push_str(&__CHRONO_MONTHS[m as usize - 1][..3]),
            "B" => out.push_str(__CHRONO_MONTHS[m as usize - 1]),
            "d" => __chrono_num(&mut out, dd as i64, 2, '0', modifier),
            "e" => __chrono_num(&mut out, dd as i64, 2, ' ', modifier),
            "a" => out.push_str(&__CHRONO_DAYS[wd.num_days_from_monday() as usize][..3]),
            "A" => out.push_str(__CHRONO_DAYS[wd.num_days_from_monday() as usize]),
            "w" => __chrono_num(&mut out, wd.num_days_from_sunday() as i64, 1, '0', modifier),
            "u" => __chrono_num(&mut out, wd.number_from_monday() as i64, 1, '0', modifier),
            "U" => {
                let yday = d.ordinal0() as i64;
                let w = (yday + 7 - wd.num_days_from_sunday() as i64) / 7;
                __chrono_num(&mut out, w, 2, '0', modifier);
            }
            "W" => {
                let yday = d.ordinal0() as i64;
                let w = (yday + 7 - wd.num_days_from_monday() as i64) / 7;
                __chrono_num(&mut out, w, 2, '0', modifier);
            }
            "G" => __chrono_num(&mut out, d.iso_week().year() as i64, 4, '0', modifier),
            "g" => __chrono_num(&mut out, d.iso_week().year().rem_euclid(100) as i64, 2, '0', modifier),
            "V" => __chrono_num(&mut out, d.iso_week().week() as i64, 2, '0', modifier),
            "j" => __chrono_num(&mut out, d.ordinal() as i64, 3, '0', modifier),
            "D" | "x" => out.push_str(&format!("{:02}/{:02}/{:02}", m, dd, y.rem_euclid(100))),
            "F" => {
                let mut ys = String::new();
                __chrono_write_year(&mut ys, y);
                out.push_str(&format!("{}-{:02}-{:02}", ys, m, dd));
            }
            "v" => out.push_str(&format!("{:>2}-{}-{:04}", dd, &__CHRONO_MONTHS[m as usize - 1][..3], y)),
            "H" => __chrono_num(&mut out, hour as i64, 2, '0', modifier),
            "k" => __chrono_num(&mut out, hour as i64, 2, ' ', modifier),
            "I" => __chrono_num(&mut out, if hour % 12 == 0 { 12 } else { hour % 12 } as i64, 2, '0', modifier),
            "l" => __chrono_num(&mut out, if hour % 12 == 0 { 12 } else { hour % 12 } as i64, 2, ' ', modifier),
            "P" => out.push_str(if hour < 12 { "am" } else { "pm" }),
            "p" => out.push_str(if hour < 12 { "AM" } else { "PM" }),
            "M" => __chrono_num(&mut out, min as i64, 2, '0', modifier),
            "S" => __chrono_num(&mut out, sec as i64, 2, '0', modifier),
            "f" => out.push_str(&format!("{:09}", t.frac)),
            ".f" => out.push_str(&__chrono_fraction(t.frac)),
            ".3f" => out.push_str(&format!(".{:03}", t.frac / 1_000_000)),
            ".6f" => out.push_str(&format!(".{:06}", t.frac / 1000)),
            ".9f" => out.push_str(&format!(".{:09}", t.frac)),
            "3f" => out.push_str(&format!("{:03}", t.frac / 1_000_000)),
            "6f" => out.push_str(&format!("{:06}", t.frac / 1000)),
            "9f" => out.push_str(&format!("{:09}", t.frac)),
            "R" => out.push_str(&format!("{:02}:{:02}", hour, min)),
            "T" | "X" => out.push_str(&format!("{:02}:{:02}:{:02}", hour, min, sec)),
            "r" => out.push_str(&format!(
                "{:02}:{:02}:{:02} {}",
                if hour % 12 == 0 { 12 } else { hour % 12 },
                min,
                sec,
                if hour < 12 { "AM" } else { "PM" }
            )),
            "c" => out.push_str(&format!(
                "{} {} {:>2} {:02}:{:02}:{:02} {}",
                &__CHRONO_DAYS[wd.num_days_from_monday() as usize][..3],
                &__CHRONO_MONTHS[m as usize - 1][..3],
                dd,
                hour,
                min,
                sec,
                y
            )),
            "Z" => {
                if !zname.is_empty() {
                    out.push_str(zname);
                } else {
                    out.push_str(&__chrono_offset_str(o, true, false));
                }
            }
            "z" => out.push_str(&__chrono_offset_str(o, false, false)),
            ":z" => out.push_str(&__chrono_offset_str(o, true, false)),
            "::z" => out.push_str(&__chrono_offset_str(o, true, true)),
            ":::z" => out.push_str(&format!("{}{:02}", if o < 0 { '-' } else { '+' }, o.abs() / 3600)),
            "#z" => out.push_str(&__chrono_offset_str(o, false, false)),
            "+" => {
                out.push_str(&__chrono_format(date, time, off, zname, "%Y-%m-%dT%H:%M:%S%.f%:z"));
            }
            "s" => {
                let secs = d.days as i64 * 86400 + t.secs as i64 - o as i64;
                __chrono_num(&mut out, secs, 1, '0', modifier);
            }
            "t" => out.push('\t'),
            "n" => out.push('\n'),
            "%" => out.push('%'),
            other => {
                out.push('%');
                out.push_str(other);
            }
        }
    }
    out
}

// ---- parsing

/// What `parse_from_str` read.
#[derive(Clone, Copy, Debug, Default)]
pub struct __ChronoParsed {
    year: Option<i32>,
    year2: Option<i32>,
    month: Option<u32>,
    day: Option<u32>,
    ordinal: Option<u32>,
    weekday: Option<ChronoWeekday>,
    hour: Option<u32>,
    hour12: Option<u32>,
    pm: Option<bool>,
    minute: Option<u32>,
    second: Option<u32>,
    nano: Option<u32>,
    offset: Option<i32>,
    timestamp: Option<i64>,
}

impl __ChronoParsed {
    fn date(&self) -> Result<ChronoNaiveDate, ChronoParseError> {
        if let Some(ts) = self.timestamp {
            return match ChronoNaiveDateTime::from_timestamp_opt(ts + self.offset.unwrap_or(0) as i64, 0) {
                Some(dt) => Ok(dt.date),
                None => Err(ChronoParseError(ChronoParseErrorKind::OutOfRange)),
            };
        }
        let year = match (self.year, self.year2) {
            (Some(y), _) => y,
            (None, Some(y)) => {
                if y >= 70 {
                    1900 + y
                } else {
                    2000 + y
                }
            }
            _ => return Err(ChronoParseError(ChronoParseErrorKind::NotEnough)),
        };
        let d = match (self.month, self.day, self.ordinal) {
            (Some(m), Some(d), _) => ChronoNaiveDate::from_ymd_opt(year, m, d),
            (_, _, Some(o)) => ChronoNaiveDate::from_yo_opt(year, o),
            _ => return Err(ChronoParseError(ChronoParseErrorKind::NotEnough)),
        };
        let Some(d) = d else {
            return Err(ChronoParseError(ChronoParseErrorKind::OutOfRange));
        };
        if let Some(w) = self.weekday
            && w != d.weekday()
        {
            return Err(ChronoParseError(ChronoParseErrorKind::Impossible));
        }
        Ok(d)
    }
    fn time(&self) -> Result<ChronoNaiveTime, ChronoParseError> {
        if let Some(ts) = self.timestamp {
            return match ChronoNaiveDateTime::from_timestamp_opt(ts + self.offset.unwrap_or(0) as i64, 0) {
                Some(dt) => Ok(dt.time),
                None => Err(ChronoParseError(ChronoParseErrorKind::OutOfRange)),
            };
        }
        let hour = match (self.hour, self.hour12, self.pm) {
            (Some(h), _, _) => h,
            (None, Some(h), Some(pm)) => h % 12 + if pm { 12 } else { 0 },
            _ => return Err(ChronoParseError(ChronoParseErrorKind::NotEnough)),
        };
        let Some(min) = self.minute else {
            return Err(ChronoParseError(ChronoParseErrorKind::NotEnough));
        };
        match ChronoNaiveTime::from_hms_nano_opt(hour, min, self.second.unwrap_or(0), self.nano.unwrap_or(0)) {
            Some(t) => Ok(t),
            None => Err(ChronoParseError(ChronoParseErrorKind::OutOfRange)),
        }
    }
    fn datetime(&self) -> Result<ChronoNaiveDateTime, ChronoParseError> {
        let date = self.date()?;
        let time = self.time()?;
        Ok(ChronoNaiveDateTime { date, time })
    }
    fn zoned(&self) -> Result<ChronoDateTime<ChronoFixedOffset>, ChronoParseError> {
        let Some(off) = self.offset else {
            return Err(ChronoParseError(ChronoParseErrorKind::NotEnough));
        };
        let local = self.datetime()?;
        Ok(ChronoDateTime {
            utc: local - ChronoTimeDelta::seconds(off as i64),
            off,
            tz: ChronoFixedOffset { secs: off },
        })
    }
}

/// Up to `max` digits (after an optional sign when `signed`), from byte `i` of `s`.
fn __chrono_scan_num(s: &[u8], i: &mut usize, min: usize, max: usize, signed: bool) -> Result<i64, ChronoParseError> {
    while *i < s.len() && (s[*i] == b' ' || s[*i] == b'\t') {
        *i += 1;
    }
    if *i >= s.len() {
        return Err(ChronoParseError(ChronoParseErrorKind::TooShort));
    }
    let mut neg = false;
    if signed && (s[*i] == b'+' || s[*i] == b'-') {
        neg = s[*i] == b'-';
        *i += 1;
    }
    let start = *i;
    let mut v: i64 = 0;
    while *i < s.len() && *i - start < max && s[*i].is_ascii_digit() {
        v = v * 10 + (s[*i] - b'0') as i64;
        *i += 1;
    }
    if *i - start < min {
        if *i >= s.len() {
            return Err(ChronoParseError(ChronoParseErrorKind::TooShort));
        }
        return Err(ChronoParseError(ChronoParseErrorKind::Invalid));
    }
    Ok(if neg { -v } else { v })
}

/// Whether two byte strings are equal, ignoring ASCII case.
fn __chrono_same(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b.iter()).all(|(x, y)| x.to_ascii_lowercase() == y.to_ascii_lowercase())
}

/// A month or weekday name (its first three letters, or the whole), case-insensitively.
fn __chrono_scan_name(s: &[u8], i: &mut usize, names: &[&str]) -> Result<usize, ChronoParseError> {
    if *i + 3 > s.len() {
        return Err(ChronoParseError(ChronoParseErrorKind::TooShort));
    }
    for (k, name) in names.iter().enumerate() {
        let n = name.as_bytes();
        if __chrono_same(&s[*i..*i + 3], &n[..3]) {
            *i += 3;
            if *i + n.len() - 3 <= s.len() && __chrono_same(&s[*i..*i + n.len() - 3], &n[3..]) {
                *i += n.len() - 3;
            }
            return Ok(k);
        }
    }
    Err(ChronoParseError(ChronoParseErrorKind::Invalid))
}

/// `+hh:mm`, `+hhmm` (or `Z` when `z`).
fn __chrono_scan_offset(s: &[u8], i: &mut usize, z: bool) -> Result<i32, ChronoParseError> {
    while *i < s.len() && s[*i] == b' ' {
        *i += 1;
    }
    if *i >= s.len() {
        return Err(ChronoParseError(ChronoParseErrorKind::TooShort));
    }
    if z && (s[*i] == b'Z' || s[*i] == b'z') {
        *i += 1;
        return Ok(0);
    }
    let neg = match s[*i] {
        b'+' => false,
        b'-' => true,
        _ => return Err(ChronoParseError(ChronoParseErrorKind::Invalid)),
    };
    *i += 1;
    let h = __chrono_scan_num(s, i, 2, 2, false)?;
    if *i < s.len() && s[*i] == b':' {
        *i += 1;
    }
    let m = if *i < s.len() && s[*i].is_ascii_digit() {
        __chrono_scan_num(s, i, 2, 2, false)?
    } else {
        0
    };
    if m >= 60 {
        return Err(ChronoParseError(ChronoParseErrorKind::OutOfRange));
    }
    let secs = (h * 3600 + m * 60) as i32;
    Ok(if neg { -secs } else { secs })
}

/// A fraction of a second after its `.` (as many digits as there are, nanoseconds kept).
fn __chrono_scan_nanos(s: &[u8], i: &mut usize, digits: usize) -> Result<u32, ChronoParseError> {
    let start = *i;
    let mut v: u64 = 0;
    let mut n = 0;
    while *i < s.len() && s[*i].is_ascii_digit() && (digits == 0 || *i - start < digits) {
        if n < 9 {
            v = v * 10 + (s[*i] - b'0') as u64;
            n += 1;
        }
        *i += 1;
    }
    if *i == start || (digits != 0 && *i - start < digits) {
        if *i >= s.len() {
            return Err(ChronoParseError(ChronoParseErrorKind::TooShort));
        }
        return Err(ChronoParseError(ChronoParseErrorKind::Invalid));
    }
    while n < 9 {
        v *= 10;
        n += 1;
    }
    Ok(v as u32)
}

fn __chrono_parse(s: &str, fmt: &str) -> Result<__ChronoParsed, ChronoParseError> {
    let (p, rest) = __chrono_parse_prefix(s, fmt)?;
    if rest > 0 {
        return Err(ChronoParseError(ChronoParseErrorKind::TooLong));
    }
    Ok(p)
}

/// Parses the start of `s` by `fmt`; also gives the length of what's left.
fn __chrono_parse_prefix(s: &str, fmt: &str) -> Result<(__ChronoParsed, usize), ChronoParseError> {
    let mut p = __ChronoParsed::default();
    let b = s.as_bytes();
    let f: Vec<char> = fmt.chars().collect();
    let mut i = 0;
    let mut k = 0;
    let range = |v: i64, lo: i64, hi: i64| {
        if v < lo || v > hi {
            Err(ChronoParseError(ChronoParseErrorKind::OutOfRange))
        } else {
            Ok(v)
        }
    };
    while k < f.len() {
        let c = f[k];
        k += 1;
        if c.is_whitespace() {
            while i < b.len() && (b[i] as char).is_ascii_whitespace() {
                i += 1;
            }
            continue;
        }
        if c != '%' {
            let lits = c.to_string();
            let lit = lits.as_bytes();
            if i >= b.len() {
                return Err(ChronoParseError(ChronoParseErrorKind::TooShort));
            }
            if b.len() - i < lit.len() || &b[i..i + lit.len()] != lit {
                return Err(ChronoParseError(ChronoParseErrorKind::Invalid));
            }
            i += lit.len();
            continue;
        }
        if k < f.len() && (f[k] == '-' || f[k] == '_' || f[k] == '0') {
            k += 1;
        }
        let mut spec = String::new();
        while k < f.len() && (f[k] == '.' || f[k] == ':' || f[k] == '#' || f[k].is_ascii_digit()) {
            spec.push(f[k]);
            k += 1;
        }
        if k >= f.len() {
            return Err(ChronoParseError(ChronoParseErrorKind::BadFormat));
        }
        spec.push(f[k]);
        k += 1;
        match spec.as_str() {
            "Y" => p.year = Some(__chrono_scan_num(b, &mut i, 1, 9, true)? as i32),
            "G" => p.year = Some(__chrono_scan_num(b, &mut i, 1, 9, true)? as i32),
            "y" => p.year2 = Some(range(__chrono_scan_num(b, &mut i, 1, 2, false)?, 0, 99)? as i32),
            "C" => {
                let c = __chrono_scan_num(b, &mut i, 1, 2, false)?;
                p.year = Some((c * 100) as i32 + p.year2.unwrap_or(0));
            }
            "m" => p.month = Some(range(__chrono_scan_num(b, &mut i, 1, 2, false)?, 1, 12)? as u32),
            "b" | "h" | "B" => p.month = Some(__chrono_scan_name(b, &mut i, &__CHRONO_MONTHS)? as u32 + 1),
            "d" | "e" => p.day = Some(range(__chrono_scan_num(b, &mut i, 1, 2, false)?, 1, 31)? as u32),
            "a" | "A" => p.weekday = Some(ChronoWeekday::__of(__chrono_scan_name(b, &mut i, &__CHRONO_DAYS)? as u32)),
            "w" => {
                let w = range(__chrono_scan_num(b, &mut i, 1, 1, false)?, 0, 6)?;
                p.weekday = Some(ChronoWeekday::__of((w as u32 + 6) % 7));
            }
            "u" => {
                let w = range(__chrono_scan_num(b, &mut i, 1, 1, false)?, 1, 7)?;
                p.weekday = Some(ChronoWeekday::__of(w as u32 - 1));
            }
            "j" => p.ordinal = Some(range(__chrono_scan_num(b, &mut i, 1, 3, false)?, 1, 366)? as u32),
            "H" | "k" => p.hour = Some(range(__chrono_scan_num(b, &mut i, 1, 2, false)?, 0, 23)? as u32),
            "I" | "l" => p.hour12 = Some(range(__chrono_scan_num(b, &mut i, 1, 2, false)?, 1, 12)? as u32),
            "M" => p.minute = Some(range(__chrono_scan_num(b, &mut i, 1, 2, false)?, 0, 59)? as u32),
            "S" => p.second = Some(range(__chrono_scan_num(b, &mut i, 1, 2, false)?, 0, 60)? as u32),
            "p" | "P" => {
                if i + 2 > b.len() {
                    return Err(ChronoParseError(ChronoParseErrorKind::TooShort));
                }
                let w = &b[i..i + 2];
                if __chrono_same(w, b"am") {
                    p.pm = Some(false);
                } else if __chrono_same(w, b"pm") {
                    p.pm = Some(true);
                } else {
                    return Err(ChronoParseError(ChronoParseErrorKind::Invalid));
                }
                i += 2;
            }
            "f" => p.nano = Some(__chrono_scan_nanos(b, &mut i, 0)?),
            "3f" => p.nano = Some(__chrono_scan_nanos(b, &mut i, 3)?),
            "6f" => p.nano = Some(__chrono_scan_nanos(b, &mut i, 6)?),
            "9f" => p.nano = Some(__chrono_scan_nanos(b, &mut i, 9)?),
            ".f" | ".3f" | ".6f" | ".9f" => {
                let digits = match spec.as_str() {
                    ".3f" => 3,
                    ".6f" => 6,
                    ".9f" => 9,
                    _ => 0,
                };
                if i < b.len() && b[i] == b'.' {
                    i += 1;
                    p.nano = Some(__chrono_scan_nanos(b, &mut i, digits)?);
                } else if digits != 0 {
                    if i >= b.len() {
                        return Err(ChronoParseError(ChronoParseErrorKind::TooShort));
                    }
                    return Err(ChronoParseError(ChronoParseErrorKind::Invalid));
                }
            }
            "z" | ":z" | "::z" | ":::z" => p.offset = Some(__chrono_scan_offset(b, &mut i, false)?),
            "#z" => p.offset = Some(__chrono_scan_offset(b, &mut i, true)?),
            "Z" => {
                while i < b.len() && (b[i] as char).is_ascii_alphanumeric() {
                    i += 1;
                }
            }
            "s" => p.timestamp = Some(__chrono_scan_num(b, &mut i, 1, 19, true)?),
            "D" | "x" => {
                let q = __chrono_parse_prefix(&s[i..], "%m/%d/%y")?;
                p.month = q.0.month;
                p.day = q.0.day;
                p.year2 = q.0.year2;
                i = b.len() - q.1;
            }
            "F" => {
                let q = __chrono_parse_prefix(&s[i..], "%Y-%m-%d")?;
                p.year = q.0.year;
                p.month = q.0.month;
                p.day = q.0.day;
                i = b.len() - q.1;
            }
            "R" | "T" | "X" => {
                let q = __chrono_parse_prefix(&s[i..], if spec == "R" { "%H:%M" } else { "%H:%M:%S" })?;
                p.hour = q.0.hour;
                p.minute = q.0.minute;
                p.second = q.0.second;
                i = b.len() - q.1;
            }
            "t" | "n" => {
                while i < b.len() && (b[i] as char).is_ascii_whitespace() {
                    i += 1;
                }
            }
            "%" => {
                if i >= b.len() {
                    return Err(ChronoParseError(ChronoParseErrorKind::TooShort));
                }
                if b[i] != b'%' {
                    return Err(ChronoParseError(ChronoParseErrorKind::Invalid));
                }
                i += 1;
            }
            _ => return Err(ChronoParseError(ChronoParseErrorKind::BadFormat)),
        }
    }
    Ok((p, b.len() - i))
}

/// A UTC moment and the offset it was written in.
#[derive(Clone, Copy)]
struct __ChronoZoned {
    utc: ChronoNaiveDateTime,
    off: i32,
}

/// RFC 3339: `2024-01-15T10:30:00Z`, `2024-01-15T10:30:00.5+01:00` (`T`, `t` or a space).
fn __chrono_parse_rfc3339(s: &str) -> Result<ChronoDateTime<ChronoFixedOffset>, ChronoParseError> {
    let b = s.as_bytes();
    if b.len() > 10 && (b[10] == b't' || b[10] == b' ') {
        let fixed = format!("{}T{}", &s[..10], &s[11..]);
        return __chrono_parse_rfc3339(&fixed);
    }
    let p = __chrono_parse(s, "%Y-%m-%dT%H:%M:%S%.f%#z")?;
    if b.len() < 4 || !b[..4].iter().all(|c| c.is_ascii_digit()) {
        return Err(ChronoParseError(ChronoParseErrorKind::Invalid));
    }
    p.zoned()
}

/// `DateTime`'s `FromStr`: RFC 3339, also with a space before the offset or a `UTC` suffix.
fn __chrono_parse_rfc3339_relaxed(s: &str) -> Result<__ChronoZoned, ChronoParseError> {
    let t = s.trim();
    let t = if let Some(x) = t.strip_suffix(" UTC") {
        format!("{}Z", x)
    } else {
        t.to_string()
    };
    let fixed = if t.len() > 10 && (t.as_bytes()[10] == b' ' || t.as_bytes()[10] == b't') {
        format!("{}T{}", &t[..10], &t[11..])
    } else {
        t.clone()
    };
    let p = __chrono_parse(&fixed, "%Y-%m-%dT%H:%M:%S%.f%#z")?;
    let d = p.zoned()?;
    Ok(__ChronoZoned { utc: d.utc, off: d.off })
}
