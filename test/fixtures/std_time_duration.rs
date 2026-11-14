// `std::time::Duration`: constructors, accessors, checked / saturating arithmetic, operators,
// float conversions, ordering, sums, and `{:?}` in every unit with precision, width and `+`.
use std::collections::HashSet;
use std::time::Duration;

fn main() {
    let a = Duration::new(5, 1_500_000_000);
    println!("{:?} {} {} {}", a, a.as_secs(), a.subsec_millis(), a.subsec_nanos());
    let ds = [
        Duration::from_secs(90),
        Duration::from_millis(1_234),
        Duration::from_micros(1_234_567),
        Duration::from_nanos(1_234_567_891),
        Duration::from_millis(7),
        Duration::from_micros(15),
        Duration::from_nanos(999),
        Duration::from_nanos(1_500),
        Duration::from_nanos(1_000_001),
        Duration::ZERO,
        Duration::from_secs_f64(2.7),
        Duration::from_secs_f64(0.000_000_3),
        Duration::from_secs_f32(0.25),
    ];
    for d in ds.iter() {
        println!(
            "{:?} | {:.0?} | {:.1?} | {:.3?} | {:.12?} | {:>10?}| {:<10?}| {:^10?}| {:+?} | {:*>9.1?}",
            d, d, d, d, d, d, d, d, d, d
        );
        println!(
            "  {} {} {} {} {} {}",
            d.as_millis(),
            d.as_micros(),
            d.as_nanos(),
            d.as_secs_f64(),
            d.subsec_micros(),
            d.is_zero()
        );
    }
    // rounding carries into the integer part and the unit stays
    println!("{:.2?} {:.0?} {:.1?}", Duration::from_nanos(1_999_999_999), Duration::from_micros(999_999), Duration::from_nanos(999_960));

    let x = Duration::from_millis(1_500);
    let y = Duration::from_millis(700);
    println!("{:?} {:?} {:?} {:?}", x + y, x - y, x * 3, x / 4);
    println!("{:?} {:?}", x.checked_sub(y), y.checked_sub(x));
    println!("{:?} {:?}", x.saturating_sub(Duration::from_secs(9)), Duration::MAX.saturating_add(x) == Duration::MAX);
    println!("{:?} {:?} {:?}", x.checked_mul(2), x.checked_div(0), Duration::from_secs(1).checked_div(3));
    println!("{:?} {:?}", x.abs_diff(y), y.abs_diff(x));
    println!("{:?} {:?} {:?}", x.mul_f64(1.5), x.div_f64(3.0), x.div_duration_f64(y));
    let mut z = Duration::from_secs(1);
    z += Duration::from_millis(250);
    z -= Duration::from_millis(50);
    z *= 3;
    z /= 2;
    println!("{:?}", z);

    println!("{} {} {:?}", x > y, x == Duration::from_micros(1_500_000), x.cmp(&y));
    let mut v = vec![x, y, Duration::from_secs(1), Duration::ZERO];
    v.sort();
    println!("{:?}", v);
    println!("{:?} {:?}", v.iter().max(), v.iter().min());
    let total: Duration = v.iter().copied().sum();
    println!("{:?} {:?}", total, Duration::default());
    let set: HashSet<Duration> = v.iter().copied().collect();
    println!("{}", set.len());

    #[derive(Debug)]
    struct Lap {
        name: String,
        time: Duration,
    }
    let laps = vec![
        Lap { name: "one".to_string(), time: Duration::from_millis(61_250) },
        Lap { name: "two".to_string(), time: Duration::from_micros(59_870_400) },
    ];
    println!("{:?}", laps);
    println!("{:?}", Some(Duration::from_millis(3)));
}
