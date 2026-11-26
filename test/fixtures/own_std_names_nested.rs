// Types named like std or bundled ones that aren't at the top level: in inline modules (flattened
// to `fmt__Formatter`), in function bodies, two functions' own `Error`s, and std's `Range` next to
// them, which stays std's.
mod fmt {
    #[derive(Debug)]
    pub struct Formatter {
        pub depth: usize,
        pub out: String,
    }
    impl Formatter {
        pub fn new() -> Formatter {
            Formatter { depth: 0, out: String::new() }
        }
        pub fn write(&mut self, s: &str) {
            self.out.push_str(s);
        }
    }
    pub fn render(f: &mut Formatter) {
        f.write("x");
        f.depth += 1;
    }
    impl std::fmt::Display for Formatter {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "F({})", self.out)
        }
    }
    pub mod inner {
        use super::Formatter;
        pub fn make() -> Formatter {
            let mut f = Formatter::new();
            super::render(&mut f);
            f
        }
        pub fn make2() -> super::Formatter {
            super::Formatter { depth: 7, out: "two".to_string() }
        }
    }
    #[derive(Debug, Clone, PartialEq)]
    pub enum Error {
        Bad(u8),
        Worse,
    }
}

mod other {
    use crate::fmt::*;
    pub fn go() -> Formatter {
        let mut f = Formatter::new();
        f.write("o");
        f
    }
    pub fn err() -> Error {
        Error::Worse
    }
}

use fmt::Formatter;

fn ranges() {
    #[derive(Debug, Clone, Copy)]
    struct Range {
        start: usize,
        end: usize,
    }
    impl Range {
        fn len(&self) -> usize {
            self.end - self.start
        }
    }
    struct Formatter<'a> {
        buf: &'a mut String,
    }
    impl<'a> Formatter<'a> {
        fn write_str(&mut self, s: &str) {
            self.buf.push_str(s);
        }
    }
    let r = Range { start: 2, end: 5 };
    let mut s = String::new();
    let mut f = Formatter { buf: &mut s };
    f.write_str("x");
    f.write_str("y");
    let std_range: std::ops::Range<usize> = 1..4;
    println!("{:?} {} {} {:?}", r, r.len(), s, std_range);
}

fn first() -> String {
    #[derive(Debug)]
    enum Error {
        Bad(u8),
    }
    format!("{:?}", Error::Bad(1))
}

fn second() -> String {
    #[derive(Debug)]
    struct Error {
        code: i32,
    }
    format!("{:?}", Error { code: -1 })
}

fn std_range() -> std::ops::Range<u8> {
    let r: Range<u8> = 3..6;
    r
}

use std::ops::Range;

fn main() {
    let mut f = fmt::Formatter::new();
    fmt::render(&mut f);
    let g: Formatter = fmt::inner::make();
    let h = other::go();
    println!("{} {} {} {:?}", f, g.out.len(), h, fmt::Error::Bad(3));
    println!("{:?} {:?}", fmt::inner::make2(), other::err() == fmt::Error::Worse);
    let e: Result<(), std::fmt::Error> = Err(std::fmt::Error);
    println!("{:?}", e);
    ranges();
    println!("{} {} {:?} {}", first(), second(), std_range(), std_range().len());
}
