// `NonZero<char>` (a `char` here) and `NonZero::new` / `NonZero::<T>::new` for any of them.
use std::num::{NonZero, NonZeroU32};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
struct Key {
    c: NonZero<char>,
    n: NonZero<u8>,
}

fn first_nonzero(s: &str) -> Option<NonZero<char>> {
    s.chars().find_map(NonZero::<char>::new)
}

fn main() {
    let k = Key { c: NonZero::new('q').unwrap(), n: NonZero::<u8>::new(3).unwrap() };
    let j = k;
    println!("{:?} {:?} {} {}", k, j, k == j, k <= j);
    println!("{} {}", k.c.get(), k.n.get());

    let none: Option<NonZero<u8>> = NonZero::new(0);
    let z: Option<NonZero<char>> = NonZero::new('\0');
    let big = NonZero::<u128>::new(0);
    let wide: Option<NonZero<i64>> = NonZero::new(-4);
    println!("{:?} {:?} {:?} {:?}", none, z, big, wide);
    println!("{:?} {:?}", NonZero::<char>::new('0'), NonZero::new(0u16));

    let u = NonZeroU32::new(9).unwrap();
    let w: NonZero<u32> = u;
    println!("{} {}", w, u.get() + w.get());
    println!("{:?} {:?}", first_nonzero("\0\0x"), first_nonzero("\0"));
}
