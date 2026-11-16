// The bundled `chrono`: dates, times, date-times in UTC and fixed offsets, `TimeDelta`,
// `Days` / `Months`, weekdays, ISO weeks, `format` / `parse_from_str`, RFC 3339 / 2822, parse
// errors. (`now()` is only checked for being plausible.)
use chrono::prelude::*;
use chrono::{Days, Duration, Months, NaiveDate, NaiveDateTime, NaiveTime, TimeDelta};
use std::collections::BTreeMap;

fn age(born: NaiveDate, on: NaiveDate) -> u32 {
    on.years_since(born).unwrap_or(0)
}

fn main() {
    let d = NaiveDate::from_ymd_opt(2024, 2, 28).unwrap();
    println!("{} {:?} {}", d, d, d.weekday());
    let next = d.succ_opt().unwrap();
    println!("{} {} leap {}", next, next.succ_opt().unwrap(), d.leap_year());
    println!("{:?}", NaiveDate::from_ymd_opt(2023, 2, 29));
    println!("y={} m={} d={} ord={} ce={}", d.year(), d.month(), d.day(), d.ordinal(), d.num_days_from_ce());
    println!("{} {}", d + Duration::days(400), d - TimeDelta::weeks(10));
    println!("{}", d.checked_add_months(Months::new(12)).unwrap());
    let jan31 = NaiveDate::from_ymd_opt(2023, 1, 31).unwrap();
    println!("{} {}", jan31 + Months::new(1), jan31 - Days::new(31));
    let span = NaiveDate::from_ymd_opt(2025, 1, 1).unwrap() - d;
    println!("{} days, {:?}, {}", span.num_days(), span, span);
    println!("age {}", age(NaiveDate::from_ymd_opt(1990, 3, 1).unwrap(), d));
    println!("{:?} {}", d.iso_week(), NaiveDate::from_ymd_opt(2021, 1, 3).unwrap().iso_week().week());
    let week: Vec<String> = d.iter_days().take(4).map(|x| x.format("%a %d").to_string()).collect();
    println!("{:?}", week);
    println!("{}", d.with_day(1).unwrap().with_month(12).unwrap());
    println!("{:?}", d.with_day(31));
    println!("{}", NaiveDate::from_weekday_of_month_opt(2024, 11, Weekday::Thu, 4).unwrap());

    let t = NaiveTime::from_hms_milli_opt(9, 5, 7, 250).unwrap();
    println!("{} {:?} h={} m={} s={} ns={}", t, t, t.hour(), t.minute(), t.second(), t.nanosecond());
    println!("{}", t + Duration::hours(20));
    println!("{}", t.format("%I:%M %p|%-H|%k|%.3f|%f"));

    let dt = d.and_hms_opt(23, 59, 30).unwrap();
    println!("{} {:?}", dt, dt);
    let later = dt + Duration::seconds(45) + Duration::milliseconds(1500);
    println!("{} {}", later, later.signed_duration_since(dt));
    println!("{}", dt.format("%Y-%m-%d %H:%M:%S | %A %B %e | %j | %D | %F %T | %y %C | %U %W %V %G | %%"));
    println!("{}", dt.format("%c | %r | %R | %v | %u %w"));
    println!("{}", dt.and_utc().timestamp());

    let utc = Utc.with_ymd_and_hms(2024, 6, 1, 12, 0, 0).unwrap();
    println!("{} | {:?} | {}", utc, utc, utc.to_rfc3339());
    println!("{}", utc.to_rfc2822());
    println!("{}", utc.to_rfc3339_opts(SecondsFormat::Millis, true));
    let tokyo = FixedOffset::east_opt(9 * 3600).unwrap();
    let there = utc.with_timezone(&tokyo);
    println!("{} | {:?} | {} {}", there, there, there.hour(), there.offset());
    println!("{}", there.format("%Y-%m-%dT%H:%M:%S%:z %z %Z"));
    println!("same instant {}", there == utc.with_timezone(&tokyo));
    println!("{}", (utc + Duration::days(2)).timestamp() - utc.timestamp());
    let ny = FixedOffset::west_opt(5 * 3600).unwrap();
    let local = ny.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();
    println!("{} -> {}", local, local.with_timezone(&Utc));
    println!("{}", DateTime::from_timestamp(1_700_000_000, 0).unwrap());
    println!("{:?}", DateTime::from_timestamp_millis(-1));

    let parsed = DateTime::parse_from_rfc3339("2024-03-10T08:15:30.125+02:00").unwrap();
    println!("{} {}", parsed, parsed.timestamp_millis());
    let z: DateTime<Utc> = "2024-03-10T08:15:30Z".parse().unwrap();
    println!("{:?}", z);
    println!("{}", DateTime::parse_from_rfc2822("Sun, 10 Mar 2024 08:15:30 +0000").unwrap());
    println!("{:?}", NaiveDate::parse_from_str("2024-13-01", "%Y-%m-%d"));
    println!("{:?}", NaiveDate::parse_from_str("2024-02-30", "%Y-%m-%d"));
    println!("{:?}", NaiveDate::parse_from_str("2024-02-", "%Y-%m-%d"));
    println!("{:?}", NaiveDate::parse_from_str("2024-02-03x", "%Y-%m-%d"));
    println!("{:?}", NaiveDate::parse_from_str("2024/02/03", "%Y-%m-%d"));
    println!("{:?}", NaiveDateTime::parse_from_str("5 Mar 2024 7:04 pm", "%d %b %Y %I:%M %p"));
    println!("{:?}", NaiveTime::parse_from_str("10:30", "%H:%M"));
    println!("{:?}", NaiveTime::parse_from_str("10", "%H"));
    println!("{:?}", "2020-01-05".parse::<NaiveDate>());
    match "nope".parse::<NaiveDate>() {
        Ok(_) => println!("parsed?"),
        Err(e) => println!("error: {}", e),
    }
    println!("{:?}", DateTime::parse_from_str("2024-01-02 03:04:05 +0530", "%Y-%m-%d %H:%M:%S %z"));

    let mut by_month: BTreeMap<NaiveDate, u32> = BTreeMap::new();
    for s in ["2024-03-05", "2024-01-20", "2024-03-30", "2023-12-31"] {
        let day: NaiveDate = s.parse().unwrap();
        *by_month.entry(day.with_day(1).unwrap()).or_insert(0) += 1;
    }
    for (m, n) in &by_month {
        println!("{} {}", m.format("%b %Y"), n);
    }
    let mut dates = vec![d, jan31, next];
    dates.sort();
    println!("{:?} max {}", dates, dates.iter().max().unwrap());

    let w = Weekday::Sat;
    println!("{} {:?} {} {} {}", w, w.succ(), w.number_from_monday(), w.num_days_from_sunday(), w.pred());
    println!("{} {:?}", Month::February.name(), Month::try_from(11u8).unwrap());

    let dur = Duration::minutes(-90) + Duration::milliseconds(250);
    println!("{} {} {} {:?}", dur, dur.num_minutes(), dur.num_seconds(), dur);
    println!("{} {}", Duration::zero(), Duration::seconds(3600).num_hours());
    println!("{:?} {:?}", Duration::seconds(5).to_std(), Duration::seconds(-5).to_std().is_err());
    let total: Duration = [Duration::minutes(5), Duration::seconds(30)].into_iter().sum();
    println!("{}", total.num_seconds());

    // (The host's time zone, which differs between the builds when the tests set `TZ`: only
    // facts that hold in any zone.)
    for (m, h) in [(1, 9), (7, 18)] {
        let l = Local.with_ymd_and_hms(2024, m, 15, h, 30, 0).unwrap();
        let back = l.with_timezone(&Utc).with_timezone(&Local);
        println!("{} {} {}", back == l, l.naive_local(), l.hour());
        let off = l.offset().local_minus_utc();
        println!("{}", l.naive_utc() + Duration::seconds(off as i64) == l.naive_local());
    }
    let now = Utc::now();
    println!("now ok: {}", now.year() >= 2024 && now > utc);
    let local_now = Local::now();
    println!("local ok: {}", (local_now.with_timezone(&Utc) - now).num_seconds().abs() < 5);
    println!("local format: {}", local_now.format("%Y").to_string().len());
}
