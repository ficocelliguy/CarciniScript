// The bundled `chrono-tz`: IANA zones (constants, imported names, parsed), offsets and
// abbreviations, DST gaps and folds.
use chrono::{TimeZone, Utc, Offset};
use chrono_tz::Tz;
use chrono_tz::Europe::London;
fn main() {
    let t = Utc.with_ymd_and_hms(2024, 7, 1, 12, 0, 0).unwrap();
    for z in [London, chrono_tz::America::New_York, chrono_tz::Europe::Berlin, chrono_tz::Asia::Tokyo, chrono_tz::Asia::Kolkata, chrono_tz::Australia::Sydney, chrono_tz::UTC] {
        let l = t.with_timezone(&z);
        println!("{} | {:?} | {} | {:?} {} {}", l, l, l.format("%Z %z"), z, z, z.name());
    }
    let w = Utc.with_ymd_and_hms(2024, 1, 1, 12, 0, 0).unwrap().with_timezone(&London);
    println!("{} {:?}", w, w.offset());
    println!("{:?}", "America/New_York".parse::<Tz>());
    match "Mars/Base".parse::<Tz>() { Ok(_) => {}, Err(e) => println!("{} / {:?}", e, e) }
    let ny: Tz = "America/New_York".parse().unwrap();
    println!("{}", ny.with_ymd_and_hms(2024, 3, 10, 2, 30, 0).single().is_none());
    println!("{:?}", ny.with_ymd_and_hms(2024, 11, 3, 1, 30, 0));
    println!("{}", w.offset().fix());
}
