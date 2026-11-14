//! A native stand-in for CarciniScript's built-in `js` crate, so `test/js/*.rs` run under cargo
//! too: `JSON` is serde_json (errors without their position, which CarciniScript doesn't
//! report), and `Date` is a UTC-only clock (the JS side runs with `TZ=UTC`).
use std::cell::Cell;
use std::fmt;

use serde::Serialize;
use serde::de::DeserializeOwned;

pub struct JSON;

impl JSON {
    pub fn stringify<T: Serialize + ?Sized>(value: &T) -> String {
        serde_json::to_string(value).unwrap()
    }

    pub fn stringify_pretty<T: Serialize + ?Sized>(value: &T) -> String {
        serde_json::to_string_pretty(value).unwrap()
    }

    pub fn parse<T: DeserializeOwned>(text: &str) -> Result<T, JsonError> {
        serde_json::from_str(text).map_err(|e| {
            let s = e.to_string();
            JsonError(match s.rfind(" at line ") {
                Some(i) => s[..i].to_string(),
                None => s,
            })
        })
    }
}

#[derive(Debug, Clone)]
pub struct JsonError(String);

impl JsonError {
    pub fn message(&self) -> String {
        self.0.clone()
    }
}

impl fmt::Display for JsonError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for JsonError {}

/// Days since 1970-01-01 → (year, month 1-12, day 1-31) (Howard Hinnant's civil_from_days).
fn civil(days: i64) -> (i64, i64, i64) {
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// (year, month 1-12, day 1-31) → days since 1970-01-01.
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

const DAY: f64 = 86_400_000.0;
const DAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

#[derive(Clone)]
pub struct Date {
    ms: Cell<f64>,
}

impl Date {
    pub fn now() -> f64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as f64
    }

    pub fn new() -> Date {
        Date::from_millis(Date::now())
    }

    pub fn from_millis(ms: f64) -> Date {
        Date { ms: Cell::new(ms) }
    }

    /// `Date.UTC(..)` (months from 0, overflowing fields carry like JS).
    pub fn utc(year: i32, month: i32, day: i32, hours: i32, minutes: i32, seconds: i32, ms: i32) -> f64 {
        let y = year as i64 + (month as i64).div_euclid(12);
        let m = (month as i64).rem_euclid(12) + 1;
        let days = days_from_civil(y, m, 1) + day as i64 - 1;
        days as f64 * DAY + hours as f64 * 3_600_000.0 + minutes as f64 * 60_000.0 + seconds as f64 * 1000.0 + ms as f64
    }

    /// A local date: local time is UTC here.
    pub fn local(year: i32, month: i32, day: i32, hours: i32, minutes: i32, seconds: i32, ms: i32) -> Date {
        Date::from_millis(Date::utc(year, month, day, hours, minutes, seconds, ms))
    }

    /// `YYYY-MM-DDTHH:MM:SS(.mmm)Z` (and a bare date); NaN otherwise.
    pub fn parse(s: &str) -> f64 {
        let num = |a: usize, b: usize| s.get(a..b).and_then(|x| x.parse::<i32>().ok());
        let (Some(y), Some(mo), Some(d)) = (num(0, 4), num(5, 7), num(8, 10)) else {
            return f64::NAN;
        };
        if s.len() == 10 {
            return Date::utc(y, mo - 1, d, 0, 0, 0, 0);
        }
        let (Some(h), Some(mi), Some(sec)) = (num(11, 13), num(14, 16), num(17, 19)) else {
            return f64::NAN;
        };
        let ms = if s.as_bytes().get(19) == Some(&b'.') { num(20, 23).unwrap_or(0) } else { 0 };
        Date::utc(y, mo - 1, d, h, mi, sec, ms)
    }

    fn parts(&self) -> (i64, i64, i64, i64, i64, i64, i64, i64) {
        let t = self.ms.get();
        let days = (t / DAY).floor() as i64;
        let rest = t as i64 - days * DAY as i64;
        let (y, m, d) = civil(days);
        let weekday = (days + 4).rem_euclid(7);
        (y, m, d, rest / 3_600_000, rest / 60_000 % 60, rest / 1000 % 60, rest % 1000, weekday)
    }

    pub fn get_time(&self) -> f64 {
        self.ms.get()
    }
    pub fn get_utc_full_year(&self) -> i32 {
        self.parts().0 as i32
    }
    pub fn get_utc_month(&self) -> i32 {
        self.parts().1 as i32 - 1
    }
    pub fn get_utc_date(&self) -> i32 {
        self.parts().2 as i32
    }
    pub fn get_utc_day(&self) -> i32 {
        self.parts().7 as i32
    }
    pub fn get_utc_hours(&self) -> i32 {
        self.parts().3 as i32
    }
    pub fn get_utc_minutes(&self) -> i32 {
        self.parts().4 as i32
    }
    pub fn get_utc_seconds(&self) -> i32 {
        self.parts().5 as i32
    }
    pub fn get_utc_milliseconds(&self) -> i32 {
        self.parts().6 as i32
    }
    pub fn get_full_year(&self) -> i32 {
        self.get_utc_full_year()
    }
    pub fn get_month(&self) -> i32 {
        self.get_utc_month()
    }
    pub fn get_date(&self) -> i32 {
        self.get_utc_date()
    }
    pub fn get_day(&self) -> i32 {
        self.get_utc_day()
    }
    pub fn get_hours(&self) -> i32 {
        self.get_utc_hours()
    }
    pub fn get_minutes(&self) -> i32 {
        self.get_utc_minutes()
    }
    pub fn get_seconds(&self) -> i32 {
        self.get_utc_seconds()
    }
    pub fn get_milliseconds(&self) -> i32 {
        self.get_utc_milliseconds()
    }
    pub fn get_timezone_offset(&self) -> i32 {
        0
    }

    pub fn set_time(&self, ms: f64) -> f64 {
        self.ms.set(ms);
        ms
    }

    /// The time with some UTC fields replaced (JS carries overflow into the next field).
    fn with(&self, f: impl Fn(&mut [i64; 7])) -> f64 {
        let (y, m, d, h, mi, s, ms, _) = self.parts();
        let mut p = [y, m - 1, d, h, mi, s, ms];
        f(&mut p);
        let t = Date::utc(p[0] as i32, p[1] as i32, p[2] as i32, p[3] as i32, p[4] as i32, p[5] as i32, p[6] as i32);
        self.ms.set(t);
        t
    }

    pub fn set_utc_date(&self, day: i32) -> f64 {
        self.with(|p| p[2] = day as i64)
    }
    pub fn set_utc_hours(&self, hours: i32) -> f64 {
        self.with(|p| p[3] = hours as i64)
    }
    pub fn set_utc_minutes(&self, minutes: i32) -> f64 {
        self.with(|p| p[4] = minutes as i64)
    }
    pub fn set_date(&self, day: i32) -> f64 {
        self.set_utc_date(day)
    }
    pub fn set_hours(&self, hours: i32) -> f64 {
        self.set_utc_hours(hours)
    }

    pub fn to_iso_string(&self) -> String {
        let (y, m, d, h, mi, s, ms, _) = self.parts();
        format!("{y:04}-{m:02}-{d:02}T{h:02}:{mi:02}:{s:02}.{ms:03}Z")
    }

    pub fn to_utc_string(&self) -> String {
        let (y, m, d, h, mi, s, _, wd) = self.parts();
        format!(
            "{}, {d:02} {} {y:04} {h:02}:{mi:02}:{s:02} GMT",
            DAYS[wd as usize],
            MONTHS[m as usize - 1]
        )
    }

    pub fn to_date_string(&self) -> String {
        let (y, m, d, _, _, _, _, wd) = self.parts();
        format!("{} {} {d:02} {y:04}", DAYS[wd as usize], MONTHS[m as usize - 1])
    }
}

impl fmt::Debug for Date {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Date({})", self.to_iso_string())
    }
}

impl fmt::Display for Date {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let (_, _, _, h, mi, s, _, _) = self.parts();
        write!(
            f,
            "{} {h:02}:{mi:02}:{s:02} GMT+0000 (Coordinated Universal Time)",
            self.to_date_string()
        )
    }
}
