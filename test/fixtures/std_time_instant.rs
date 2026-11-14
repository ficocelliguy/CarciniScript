// `std::time::Instant` and `SystemTime`: elapsed time grows, instants order and subtract
// (saturating), `+` / `-` a `Duration`, and `SystemTime` against `UNIX_EPOCH`.
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

fn busy(n: u64) -> u64 {
    let mut x = 0u64;
    for i in 0..n {
        x = x.wrapping_add(i * i % 7);
    }
    x
}

fn main() {
    let start = Instant::now();
    let work = busy(200_000);
    let mid = Instant::now();
    let el = start.elapsed();
    println!("{} {}", work, el >= mid.duration_since(start));
    println!("{} {} {}", mid >= start, start <= mid, mid.duration_since(start) < Duration::from_secs(60));
    // earlier.duration_since(later) saturates to zero
    println!("{:?} {:?}", start.duration_since(mid + Duration::from_secs(1)), start.checked_duration_since(mid + Duration::from_secs(1)));
    println!("{:?}", start.saturating_duration_since(start));
    let later = start + Duration::from_millis(1_500);
    println!("{:?} {:?}", later - start, later.duration_since(start));
    println!("{:?}", (later - Duration::from_millis(500)) - start);
    let mut t = start;
    t += Duration::from_secs(2);
    t -= Duration::from_millis(250);
    println!("{:?} {}", t - start, t > start);
    println!("{:?}", start.checked_add(Duration::from_secs(5)).map(|i| i - start));

    let now = SystemTime::now();
    let since = now.duration_since(UNIX_EPOCH).unwrap();
    println!("{} {}", since.as_secs() > 1_700_000_000, since.as_millis() > 1_700_000_000_000);
    println!("{}", SystemTime::UNIX_EPOCH == UNIX_EPOCH);
    let err = UNIX_EPOCH.duration_since(now).unwrap_err();
    println!("{} {}", err, err.duration() == since);
    let soon = now + Duration::from_secs(10);
    println!("{:?}", soon.duration_since(now));
    println!("{}", now.elapsed().is_ok());
    println!("{:?}", (UNIX_EPOCH + Duration::from_secs(86_400)).duration_since(UNIX_EPOCH));
    println!("{:?}", UNIX_EPOCH.checked_sub(Duration::from_secs(1)).is_none());
}
