// `str::from_utf8(..)` gives a `&str` (passed straight to `push_str`), and
// `String::from_utf8_lossy(..)` a `Cow<str>`: `Borrowed` when the bytes were valid UTF-8,
// `Owned` when a byte was replaced. A leading BOM is kept, as std keeps it.
use std::borrow::Cow;
use core::str;

fn decode(input: Cow<'_, [u8]>) -> Cow<'_, str> {
    match input {
        Cow::Borrowed(bytes) => String::from_utf8_lossy(bytes),
        Cow::Owned(bytes) => match String::from_utf8_lossy(&bytes) {
            Cow::Borrowed(_) => Cow::Owned(String::from_utf8(bytes).unwrap()),
            Cow::Owned(s) => Cow::Owned(s),
        },
    }
}

fn kind(b: &[u8]) -> String {
    match String::from_utf8_lossy(b) {
        Cow::Borrowed(s) => format!("borrowed {} {}", s, s.len()),
        Cow::Owned(s) => format!("owned {} {}", s, s.len()),
    }
}

struct Sink<'a> {
    out: &'a mut String,
}

impl<'a> Sink<'a> {
    fn write(&mut self, s: &[u8]) {
        self.out.push_str(str::from_utf8(s).unwrap());
    }
}

fn main() {
    let mut out = String::new();
    out.push_str(std::str::from_utf8(b"ab").unwrap());
    let mut sink = Sink { out: &mut out };
    sink.write("cé".as_bytes());
    println!("{}", out);

    println!("{}", kind(b"plain"));
    println!("{}", kind(&[104, 0xff, 105]));
    let simple: Cow<str> = String::from_utf8_lossy(b"xyz");
    println!("{} {}", simple, simple.len());
    println!("{}", decode(Cow::Borrowed(b"one")));
    println!("{}", decode(Cow::Owned(vec![116, 119, 111])));
    println!("{}", decode(Cow::Owned(vec![0xc3, 0x28])));
    let owned: String = String::from_utf8_lossy(b"own").into_owned();
    println!("{}", owned);

    let bom = [0xef, 0xbb, 0xbf, b'A'];
    println!("{} {}", str::from_utf8(&bom).unwrap().len(), String::from_utf8_lossy(&bom).len());
    println!("{}", String::from_utf8(bom.to_vec()).unwrap().chars().count());
}
