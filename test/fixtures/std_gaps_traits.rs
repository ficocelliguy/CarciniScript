// Std traits implemented by hand (PLAN "Std-library gaps"): `ExactSizeIterator` /
// `FusedIterator` (`len()` from `size_hint`), `DoubleEndedIterator` (`.rev()` runs `next_back`),
// `io::Read` / `io::Write` / `BufRead`, `Hasher` / `BuildHasher`, the `UnwindSafe` markers,
// `Display::fmt` delegation (`self.0.fmt(f)`), and `for` over a type's own `iter()`.
use std::collections::HashMap;
use std::fmt::{self, Display};
use std::hash::{BuildHasher, BuildHasherDefault, Hash, Hasher};
use std::io::{self, BufRead, BufReader, Read, Write};
use std::iter::FusedIterator;
use std::panic::{RefUnwindSafe, UnwindSafe};

struct Count {
    n: usize,
    end: usize,
}

// (Before the `Iterator` impl, whose `Item` it names.)
impl DoubleEndedIterator for Count {
    fn next_back(&mut self) -> Option<Self::Item> {
        if self.n < self.end {
            self.end -= 1;
            println!("  next_back -> {}", self.end);
            Some(self.end)
        } else {
            None
        }
    }
}

impl Iterator for Count {
    type Item = usize;
    fn next(&mut self) -> Option<usize> {
        if self.n < self.end {
            self.n += 1;
            Some(self.n - 1)
        } else {
            None
        }
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        let r = self.end - self.n;
        (r, Some(r))
    }
}

impl ExactSizeIterator for Count {}
impl FusedIterator for Count {}

struct Bag {
    items: Vec<u32>,
}

struct BagIter<'a> {
    bag: &'a Bag,
    at: usize,
}

impl<'a> Iterator for BagIter<'a> {
    type Item = u32;
    fn next(&mut self) -> Option<u32> {
        let x = self.bag.items.get(self.at).copied();
        self.at += 1;
        x
    }
}

impl Bag {
    fn iter(&self) -> BagIter<'_> {
        BagIter { bag: self, at: 0 }
    }
}

struct Upper {
    out: Vec<u8>,
}

impl Write for Upper {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        for b in buf {
            self.out.push(b.to_ascii_uppercase());
        }
        Ok(buf.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// Hands out its bytes three at a time.
struct Slow {
    data: Vec<u8>,
    pos: usize,
}

impl Read for Slow {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = buf.len().min(3).min(self.data.len() - self.pos);
        buf[..n].copy_from_slice(&self.data[self.pos..self.pos + n]);
        self.pos += n;
        Ok(n)
    }
}

/// A `BufRead` of its own: the buffer is the rest of the data.
struct Lines {
    data: Vec<u8>,
    pos: usize,
}

impl Read for Lines {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = buf.len().min(self.data.len() - self.pos);
        buf[..n].copy_from_slice(&self.data[self.pos..self.pos + n]);
        self.pos += n;
        Ok(n)
    }
}

impl BufRead for Lines {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        Ok(&self.data[self.pos..])
    }
    fn consume(&mut self, amt: usize) {
        self.pos += amt;
    }
}

fn count_lines<R: BufRead>(r: R) -> usize {
    r.lines().count()
}

#[derive(Default)]
struct Fnv(u32);

impl Hasher for Fnv {
    fn finish(&self) -> u64 {
        self.0 as u64
    }
    fn write(&mut self, bytes: &[u8]) {
        for b in bytes {
            self.0 = (self.0 ^ *b as u32).wrapping_mul(16777619);
        }
    }
}

#[derive(Default, Clone)]
struct FnvBuild;

impl BuildHasher for FnvBuild {
    type Hasher = Fnv;
    fn build_hasher(&self) -> Fnv {
        Fnv(0x811c9dc5)
    }
}

/// `self.0.fmt(f)`: the field's own `Display`, with the caller's width and fill.
struct Score(u32);

impl Display for Score {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.0.fmt(f)
    }
}

struct Label(String);

impl Display for Label {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

struct Shared;
impl UnwindSafe for Shared {}
impl RefUnwindSafe for Shared {}

fn main() -> io::Result<()> {
    // Iterators
    let c = Count { n: 0, end: 5 };
    println!("{} {:?}", c.len(), c.size_hint());
    let back: Vec<usize> = Count { n: 0, end: 4 }.rev().collect();
    println!("{:?}", back);
    let mut c2 = Count { n: 0, end: 4 };
    println!("{:?} {:?} {}", c2.next(), c2.next_back(), c2.len());
    for x in (Count { n: 1, end: 3 }).rev() {
        println!("{}", x);
    }
    let firsts: Vec<usize> = Count { n: 0, end: 10 }.rev().take(2).collect();
    println!("{:?}", firsts);
    println!("{:?} {:?}", vec![1, 2, 3].into_iter().size_hint(), (0..10).size_hint());
    println!("{}", vec![4, 5].iter().len());
    let bag = Bag { items: vec![1, 2, 3] };
    let mut t = 0;
    for x in bag.iter() {
        t += x;
    }
    for (i, x) in bag.iter().enumerate() {
        t += i as u32 * x;
    }
    println!("{}", t);

    // io
    let mut u = Upper { out: Vec::new() };
    write!(u, "hi {}", 5)?;
    u.write_all(b" there")?;
    u.flush()?;
    println!("{}", String::from_utf8(u.out).unwrap());
    let mut s = Slow {
        data: b"hello world".to_vec(),
        pos: 0,
    };
    let mut buf = [0u8; 8];
    let n = s.read(&mut buf)?;
    println!("{} {:?}", n, &buf[..n]);
    let mut rest = String::new();
    s.read_to_string(&mut rest)?;
    println!("{:?}", rest);
    let mut five = [0u8; 5];
    Slow {
        data: b"abcdefg".to_vec(),
        pos: 0,
    }
    .read_exact(&mut five)?;
    println!("{:?}", String::from_utf8_lossy(&five));
    let mut l = Lines {
        data: b"one\ntwo\nthree".to_vec(),
        pos: 0,
    };
    let mut line = String::new();
    l.read_line(&mut line)?;
    println!("{:?}", line);
    let mut upto = Vec::new();
    l.read_until(b'w', &mut upto)?;
    println!("{:?}", upto);
    for rest in l.lines() {
        println!("{:?}", rest?);
    }
    let reader = BufReader::new(Slow {
        data: b"a\nbb\nccc\n".to_vec(),
        pos: 0,
    });
    println!("{}", count_lines(reader));

    // Hasher / BuildHasher
    let mut h = Fnv(0x811c9dc5);
    h.write(b"abc");
    println!("{}", h.finish());
    let mut h2 = FnvBuild.build_hasher();
    42u64.hash(&mut h2);
    String::from("key").hash(&mut h2);
    h2.write_u32(7);
    h2.write_u8(1);
    println!("{}", h2.finish());
    println!("{} {}", FnvBuild.hash_one(5usize), FnvBuild.hash_one("abc"));
    let mut m: HashMap<&str, i32, FnvBuild> = HashMap::with_hasher(FnvBuild);
    m.insert("a", 1);
    println!("{:?}", m.get("a"));
    let mut m2: HashMap<i32, i32, BuildHasherDefault<Fnv>> = HashMap::default();
    m2.insert(1, 2);
    println!("{:?}", m2.get(&1));

    println!("[{}] [{:>6}] [{:*<5}] [{:.2}]", Score(7), Score(42), Label("ab".into()), Label("xyz".into()));
    let _ = Shared;
    Ok(())
}
