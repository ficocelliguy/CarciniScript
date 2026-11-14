// `js::Date`: building dates (`from_millis`, `utc`, `local`, `parse`), the UTC and local
// getters (local time is UTC in the tests), setters with carrying, and the string forms.
use js::Date;

fn describe(d: &Date) -> String {
    format!(
        "{}-{}-{} {}:{}:{}.{} day {}",
        d.get_utc_full_year(),
        d.get_utc_month() + 1,
        d.get_utc_date(),
        d.get_utc_hours(),
        d.get_utc_minutes(),
        d.get_utc_seconds(),
        d.get_utc_milliseconds(),
        d.get_utc_day()
    )
}

fn main() {
    let now = Date::now();
    println!("{}", now > 1.7e12 && Date::new().get_time() >= now);
    let d = Date::from_millis(1709294400123.0);
    println!("{} | {}", describe(&d), d.to_iso_string());
    println!("{} | {} | {}", d.to_utc_string(), d.to_date_string(), d);
    println!("{:?}", d);
    println!(
        "{} {} {} {} {} {} {}",
        d.get_full_year(),
        d.get_month(),
        d.get_date(),
        d.get_hours(),
        d.get_minutes(),
        d.get_seconds(),
        d.get_timezone_offset()
    );
    // `Date.UTC` with months from 0, and fields that carry over.
    let t = Date::utc(2024, 1, 29, 23, 59, 59, 999);
    println!("{} {}", t, Date::from_millis(t).to_iso_string());
    let carry = Date::utc(2023, 13, 35, 25, 61, 0, 0);
    println!("{}", Date::from_millis(carry).to_iso_string());
    let local = Date::local(2000, 0, 1, 0, 0, 0, 0);
    println!("{} {}", local.get_time(), describe(&local));
    // Parsing.
    println!("{} {}", Date::parse("2024-03-01T12:00:00.000Z"), Date::parse("1999-12-31"));
    println!("{}", Date::parse("not a date").is_nan());
    // Setters change the date in place and return the new time.
    let e = Date::from_millis(0.0);
    println!("{} {}", e.set_utc_hours(30), e.to_iso_string());
    println!("{} {}", e.set_utc_date(31), e.to_iso_string());
    e.set_time(86_400_000.0 * 365.0);
    println!("{} {}", e.get_utc_full_year(), describe(&e));
    // Leap years and the day of the week.
    for ms in [Date::utc(2000, 1, 29, 0, 0, 0, 0), Date::utc(1900, 1, 28, 0, 0, 0, 0), Date::utc(1969, 11, 31, 23, 0, 0, 0)] {
        println!("{}", describe(&Date::from_millis(ms)));
    }
}
