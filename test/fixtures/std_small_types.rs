// `ParseIntError::kind()` and `IntErrorKind`, `std::string::Drain` as a type, and
// `Pin::get_unchecked_mut` / `Pin<Box<T>>::as_mut` (`Pin` is erased), and
// `try_into()` / `<[u8; 4]>::try_from` on the bytes your own `Hasher::write` gets.
use std::num::IntErrorKind;
use std::hash::Hasher;
use std::pin::Pin;

fn take(d: std::string::Drain<'_>) -> String {
    d.collect()
}

struct S {
    n: u32,
}

fn bump(p: Pin<&mut S>) {
    unsafe {
        p.get_unchecked_mut().n += 1;
    }
}

struct H(u64);
impl Hasher for H {
    fn finish(&self) -> u64 {
        self.0
    }
    fn write(&mut self, bytes: &[u8]) {
        for c in bytes.chunks(4) {
            if let Ok(a) = <[u8; 4]>::try_from(c) {
                self.0 = self.0.wrapping_add(u32::from_le_bytes(a) as u64);
            } else {
                for b in c {
                    self.0 += *b as u64;
                }
            }
        }
        let first: Result<[u8; 2], _> = bytes[..2].try_into();
        self.0 += first.map(|a| a[0] as u64).unwrap_or(0);
    }
}

fn main() {
    for s in ["", "x", "999999", "-999999", "12"] {
        match s.parse::<i16>() {
            Ok(v) => println!("ok {v}"),
            Err(e) => match e.kind() {
                IntErrorKind::Empty => println!("empty"),
                IntErrorKind::InvalidDigit => println!("digit"),
                IntErrorKind::PosOverflow => println!("pos"),
                IntErrorKind::NegOverflow => println!("neg"),
                k => println!("{:?}", k),
            },
        }
    }
    let e = "".parse::<u8>().unwrap_err();
    println!("{:?} {}", e.kind(), *e.kind() == IntErrorKind::Empty);

    let mut s = String::from("hello world");
    let w = take(s.drain(..5));
    println!("{w}|{s}");
    let mut t = String::from("abc");
    let d: std::string::Drain = t.drain(1..);
    println!("{}", d.as_str());

    let mut s = S { n: 1 };
    bump(Pin::new(&mut s));
    let mut b = Box::pin(S { n: 5 });
    bump(b.as_mut());
    println!("{} {}", s.n, b.n);
    let mut h = H(0);
    h.write(&[1, 0, 0, 0, 2, 3]);
    println!("{}", h.finish());
}
