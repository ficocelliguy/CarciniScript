// Std `From` / `TryFrom` impls: `Option<T>: From<T>`, `String` / `Vec<u8>` / `PathBuf` from a
// `Cow`, `Cow<Path>` from a `PathBuf`, `Cell<T>: From<T>`, `char: From<u8>`, `u32: From<char>`,
// `String: From<char>`, `u8: TryFrom<char>`, `Vec<T>` from a `VecDeque<T>`, and
// `u32::try_from(u32)` with `Infallible` for its error.
use std::borrow::Cow;
use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::convert::Infallible;
use std::path::{Path, PathBuf};

struct Flags {
    first: Cell<bool>,
}

fn opt(s: String) -> Option<String> {
    s.into()
}

fn lower(s: &str) -> Cow<'_, str> {
    if s.chars().any(|c| c.is_uppercase()) {
        Cow::Owned(s.to_lowercase())
    } else {
        Cow::Borrowed(s)
    }
}

fn same(i: u32) -> Result<u32, Infallible> {
    u32::try_from(i)
}

fn main() {
    println!("{:?} {:?}", opt("a".into()), Option::from(3u8));
    let n: Option<u64> = 7u64.into();
    println!("{:?}", n);

    let s: String = lower("ABC").into();
    let t = String::from(lower("xyz"));
    println!("{} {}", s, t);
    let bytes: Cow<'_, [u8]> = Cow::Borrowed(&[1u8, 2]);
    let v: Vec<u8> = bytes.into();
    println!("{:?}", v);

    let p = PathBuf::from("/tmp/x");
    let c: Cow<'static, Path> = p.into();
    let back: PathBuf = c.into();
    println!("{}", back.display());

    let f = Flags { first: false.into() };
    f.first.set(true);
    let r: RefCell<u8> = 5u8.into();
    *r.borrow_mut() += 1;
    println!("{} {} {}", f.first.get(), Cell::from(9u8).get(), r.borrow());

    let ch: char = 65u8.into();
    let code: u32 = 'é'.into();
    let big: u64 = 'z'.into();
    let st: String = 'q'.into();
    println!("{} {} {} {}", ch, code, big, st);
    println!("{:?} {:?}", u8::try_from('a'), u8::try_from('€'));
    match u8::try_from('€') {
        Ok(_) => {}
        Err(e) => println!("{}", e),
    }

    let d: VecDeque<(u8, u16)> = VecDeque::from(vec![(1, 2), (3, 4)]);
    let back: Vec<(u8, u16)> = d.into();
    println!("{:?}", back);
    println!("{:?}", same(4));
}
