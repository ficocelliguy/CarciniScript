// Format specs: `{:x}`/`{:X}`/`{:o}`/`{:b}` (two's complement for negatives, `#` prefix, zero
// padding after it), `{:e}`/`{:E}` with precision, `{:#?}` pretty Debug of nested values,
// width and precision from arguments (`width$`, `1$`, `.*`, `.prec$`), and `{:.N}` rounding ties
// to even like Rust.
use std::collections::BTreeMap;

#[derive(Debug, Clone)]
struct Server {
    name: String,
    ports: Vec<u16>,
    root: Option<bool>,
    tag: (char, i32),
}

#[derive(Debug)]
enum Job {
    Hack { target: String, threads: u32 },
    Idle,
    Wait(f64),
}

fn main() {
    println!("{:x} {:X} {:o} {:b}", 255, 255, 8, 5);
    println!("{:#x} {:#X} {:#o} {:#b}", 255, 255, 8, 5);
    println!("{:08b} {:#010x} {:>6x}| {:<#6x}|", 5u8, 255, 255, 10);
    println!("{:x} {:x} {:b} {:x}", -1i8, -1i32, -2i16, -1i64);
    println!("{:e} {:E} {:.2e} {:e} {:e}", 1234.5, 0.00012, 6.02214e23, 1500, 0.0);
    let w = 8;
    let p = 3;
    println!("[{:w$}] [{:>1$}] [{:.*}] [{:.p$}] [{:>w$.p$}]", "ab", 6, 2, 3.14159, 2.71828, 1.41421);
    println!("[{:^width$}]", "mid", width = 9);
    println!("{:.0} {:.0} {:.0} {:.1} {:.2} {:.1}", 0.5, 1.5, 2.5, 0.25, 1.005, -0.35);

    let s = Server {
        name: "n00dles".into(),
        ports: vec![22, 80],
        root: Some(true),
        tag: ('x', -1),
    };
    println!("{:#?}", s);
    let jobs = vec![
        Job::Hack { target: "a, b".into(), threads: 3 },
        Job::Idle,
        Job::Wait(1.5),
    ];
    println!("{:#?}", jobs);
    let mut m = BTreeMap::new();
    m.insert("k", vec![(1, "one")]);
    m.insert("e", vec![]);
    println!("{:#?}", m);
    println!("{:#?} {:#?} {:#?}", (1,), Some(5).is_some(), Vec::<i32>::new());
}
