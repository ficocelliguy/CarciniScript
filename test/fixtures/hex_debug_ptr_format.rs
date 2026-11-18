// `{:x?}` / `{:X?}`: `Debug` with each integer in hex, the `#`, `0` and width applying to each
// (`{:02x?}` on bytes, `{:#x?}` pretty with `0x` prefixes), on integers, `Vec`s, arrays, slices,
// options, results, tuples and maps, and inside a hand-written `Debug`. `{:p}`: an address
// (made up here), the same for the same value.
use std::collections::BTreeMap;
use std::fmt;

#[derive(Debug)]
struct P {
    a: u32,
}

struct Id(u16);

impl fmt::Debug for Id {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Id(0x{:x?})", self.0)
    }
}

fn main() {
    let v = vec![10u8, 255, 3];
    println!("{:x?}", v);
    println!("{:X?}", v);
    println!("{:02x?}", v);
    println!("{:#x?}", 255u32);
    println!("{:#x?}", v);
    println!("{:X?}", Some(-1i8));
    println!("{:x?}", (1u64, "s", 'c', 2.5f64, -2i64));
    println!("{:x?}", [0xdeadu16, 7]);
    let bytes: &[u8] = &[1, 2, 171];
    println!("bad {:02x?} after", bytes);
    println!("{:0x?}|{:6x?}|{:<6x?}|{:06X?}", 3000usize, 255, 255, 255);
    let mut m = BTreeMap::new();
    m.insert(16u8, vec![17u32]);
    println!("{:x?}", m);
    let r: Result<u8, i32> = Err(-16);
    println!("{:x?} {:x?}", r, Ok::<u128, ()>(u128::MAX));
    println!("{:x?}", 10u128);
    println!("{:?}", Id(255));
    println!("{:?}", P { a: 1 });
    let x = 5;
    let rx = &x;
    let s = format!("{:p}", rx);
    let p = P { a: 2 };
    let a = format!("{:p}", &p);
    let b = format!("{:p}", &p);
    println!("{} {} {}", s.starts_with("0x"), a == b, a.len() > 2);
    println!("[{:>20}]", format!("{:p}", &p).len() <= 20);
}
