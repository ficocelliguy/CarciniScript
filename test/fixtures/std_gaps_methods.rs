// Std methods and macros filled in (PLAN "Std-library gaps"): more `Vec` / slice / `char` /
// integer / `str` methods, `Display::fmt` delegation (`self.0.fmt(f)`), `write_fmt`,
// `option_env!` and `compile_error!` under a `cfg` that's off.
use std::fmt::{self, Debug, Display, Write as _};

#[cfg(feature = "never-on")]
compile_error!("the `never-on` feature isn't supported");

#[cfg(not(any(unix, windows, target_arch = "wasm32")))]
compile_error!("unsupported platform");

struct Wrap<T>(T);

impl<T: Display> Display for Wrap<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

struct Id(u32);

impl Display for Id {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        Display::fmt(&self.0, f)
    }
}

#[derive(Debug)]
struct Inner {
    a: i32,
}

struct Outer {
    inner: Inner,
    v: Vec<u8>,
    x: f64,
    name: String,
}

impl Debug for Outer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.inner.fmt(f)?;
        f.write_str(" / ")?;
        fmt::Debug::fmt(&self.v, f)?;
        f.write_str(" / ")?;
        Debug::fmt(&self.x, f)?;
        f.write_str(" / ")?;
        fmt::Debug::fmt(&self.name, f)
    }
}

struct Money(f64);

impl Display for Money {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "$")?;
        Display::fmt(&self.0, f)
    }
}

struct Report {
    lines: Vec<String>,
}

impl Display for Report {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        for l in &self.lines {
            f.write_fmt(format_args!("- {}\n", l))?;
        }
        Ok(())
    }
}

fn main() {
    // Vec and slices
    let v = vec![1, 2, 3];
    let boxed: Box<[i32]> = v.clone().into_boxed_slice();
    println!("{:?} {}", boxed, boxed.len());
    let mut grow = v.clone();
    grow.resize(5, 0);
    println!("{:?}", grow);
    grow.resize(2, 9);
    println!("{:?}", grow);
    let mut names: Vec<String> = Vec::new();
    names.resize(2, String::from("x"));
    names[0].push('!');
    println!("{:?}", names);
    let mut counter = 0;
    let mut made: Vec<i32> = Vec::new();
    made.resize_with(3, || {
        counter += 10;
        counter
    });
    println!("{:?}", made);
    let mut cap: Vec<i32> = Vec::with_capacity(10);
    cap.reserve(5);
    cap.reserve_exact(3);
    cap.push(1);
    cap.shrink_to_fit();
    println!("{} {:?}", cap.capacity() >= cap.len(), cap);
    let mut grid = vec![1, 2, 3, 4, 5, 6, 7];
    for chunk in grid.chunks_mut(3) {
        chunk[0] *= 10;
    }
    println!("{:?}", grid);
    for pair in grid.chunks_exact_mut(2) {
        pair.swap(0, 1);
    }
    println!("{:?}", grid);
    let mut w = vec![1, 2, 3, 4, 5];
    w.copy_within(1..3, 3);
    println!("{:?}", w);
    w.retain_mut(|x| {
        *x += 1;
        *x % 2 == 0
    });
    println!("{:?}", w);
    let mut sp = vec![1, 2, 3, 4];
    let removed: Vec<i32> = sp.splice(1..3, vec![7, 8, 9]).collect();
    println!("{:?} {:?}", sp, removed);
    let doubled = [1, 2, 3].map(|x| x * 2);
    let names2 = ["a", "b"].map(|s| s.to_uppercase());
    println!("{:?} {:?}", doubled, names2);
    let sl = [1, 2, 3];
    println!("{:?} {:?} {:?}", sl.strip_prefix(&[1]), sl.strip_suffix(&[2, 3]), sl.strip_prefix(&[2]));
    println!("{} {}", "Hello".eq_ignore_ascii_case("hELLO"), b"Ab".eq_ignore_ascii_case(b"aB"));

    // Display / Debug delegation
    println!("[{}] [{:>5}] [{:<4}] [{:^5}]", Wrap(3), Wrap("ab"), Wrap('c'), Id(42));
    println!("[{:+}] [{:05}] [{:.1}]", Wrap(7), Id(9), Wrap(2.25));
    let o = Outer {
        inner: Inner { a: 1 },
        v: vec![1, 2],
        x: 2.0,
        name: "n\"q".into(),
    };
    println!("{:?}", o);
    println!("[{}] [{:.2}] [{:>8.1}]", Money(1.5), Money(2.0), Money(3.25));
    print!(
        "{}",
        Report {
            lines: vec!["a".into(), "b".into()]
        }
    );
    let mut out = String::new();
    out.write_fmt(format_args!("{}-{}", 1, "x")).unwrap();
    writeln!(out, "!").unwrap();
    print!("{}", out);

    // char
    println!("{} {} {} {}", 'a'.len_utf8(), 'é'.len_utf8(), '€'.len_utf8(), '🦀'.len_utf8());
    println!("{} {}", 'a'.len_utf16(), '🦀'.len_utf16());
    let mut buf = [0u8; 4];
    let s = 'é'.encode_utf8(&mut buf).to_string();
    println!("{} {} {:?}", s, s.len(), buf);
    println!("{} {} {}", '\n'.escape_debug(), '"'.escape_debug(), 'é'.escape_debug());
    println!("{} {}", 'é'.escape_default(), 'a'.escape_unicode());
    let escaped: String = '\t'.escape_default().to_string();
    println!("{:?}", escaped);

    // integers
    println!("{:?} {:?}", 258u16.to_le_bytes(), 258u32.to_be_bytes());
    println!("{:?} {:?}", (-2i32).to_le_bytes(), 1u64.to_be_bytes());
    println!("{:?}", 0x0102030405060708u64.to_le_bytes());
    println!("{} {}", u32::from_le_bytes([2, 1, 0, 0]), u16::from_be_bytes([1, 2]));
    println!("{} {}", i16::from_le_bytes([0xfe, 0xff]), u64::from_be_bytes([0, 0, 0, 0, 0, 0, 1, 0]));
    println!("{} {} {}", (-5i32).unsigned_abs(), i32::MIN.unsigned_abs(), (-7i64).unsigned_abs());
    println!("{} {} {}", 5u32.next_power_of_two(), 0u8.next_power_of_two(), 64usize.next_power_of_two());
    println!("{:?} {:?}", 200u8.checked_next_power_of_two(), 100u8.checked_next_power_of_two());

    // str
    let hello = "héllo";
    println!("{:?} {:?} {:?}", hello.get(0..1), hello.get(1..3), hello.get(1..2));
    println!("{:?} {:?}", hello.get(3..), hello.get(..30));
    let bs: Box<str> = String::from("boxed").into_boxed_str();
    println!("{}", bs);

    // option_env! / compile_error!
    let home: Option<&str> = option_env!("CARCINISCRIPT_SURELY_UNSET");
    println!("{:?}", home);
    if cfg!(feature = "never-on") {
        #[cfg(feature = "never-on")]
        compile_error!("not compiled");
    }
}
