// `io::SeekFrom` and `fmt::Alignment` as values and in patterns (`matches!`, `match`), next to
// `io::ErrorKind`.
use std::fmt::Alignment;
use std::io::{self, SeekFrom};

struct Buf {
    pos: u64,
    len: u64,
}
impl Buf {
    fn seek(&mut self, to: SeekFrom) -> io::Result<u64> {
        let p = match to {
            SeekFrom::Start(n) => n as i64,
            SeekFrom::End(d) => self.len as i64 + d,
            SeekFrom::Current(d) => self.pos as i64 + d,
        };
        if p < 0 {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "before start"));
        }
        self.pos = p as u64;
        Ok(self.pos)
    }
}

fn pad(a: Alignment, s: &str, w: usize) -> String {
    let fill = w.saturating_sub(s.len());
    match a {
        Alignment::Left => format!("{}{}", s, ".".repeat(fill)),
        Alignment::Right => format!("{}{}", ".".repeat(fill), s),
        Alignment::Center => format!("{}{}{}", ".".repeat(fill / 2), s, ".".repeat(fill - fill / 2)),
    }
}

fn main() {
    let mut b = Buf { pos: 0, len: 10 };
    println!("{:?}", b.seek(SeekFrom::Start(3)));
    println!("{:?}", b.seek(SeekFrom::Current(2)));
    println!("{:?}", b.seek(io::SeekFrom::End(-1)));
    let e = b.seek(SeekFrom::Current(-100)).unwrap_err();
    println!("{:?} {}", e.kind(), matches!(e.kind(), io::ErrorKind::InvalidInput | io::ErrorKind::NotFound));
    let s = SeekFrom::Start(3);
    println!("{:?} {} {}", s, matches!(s, SeekFrom::Start(_)), s == SeekFrom::Start(3));
    for a in [Alignment::Left, Alignment::Right, Alignment::Center] {
        println!("{:?} [{}] {}", a, pad(a, "ab", 7), matches!(a, Alignment::Left));
    }
}
