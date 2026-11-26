// `impl Read for` a struct named `Reader` (or `ReadBuf`): the file declaring `Reader` doesn't
// declare `Read`, so `io::Read` is still std's. Bounded on a trait of the file's own.
use std::io::{self, Read};
use std::{cmp, fmt};

pub trait Buf {
    fn remaining(&self) -> usize;
    fn take1(&mut self) -> u8;
}

struct Bytes(Vec<u8>);

impl Buf for Bytes {
    fn remaining(&self) -> usize {
        self.0.len()
    }
    fn take1(&mut self) -> u8 {
        self.0.remove(0)
    }
}

pub struct Reader<B> {
    buf: B,
}

impl<B: Buf + Sized> io::Read for Reader<B> {
    fn read(&mut self, dst: &mut [u8]) -> io::Result<usize> {
        let len = cmp::min(self.buf.remaining(), dst.len());
        for d in dst.iter_mut().take(len) {
            *d = self.buf.take1();
        }
        Ok(len)
    }
}

struct ReadBuf {
    n: u8,
}

impl Read for ReadBuf {
    fn read(&mut self, dst: &mut [u8]) -> io::Result<usize> {
        for d in dst.iter_mut() {
            self.n += 1;
            *d = self.n;
        }
        Ok(dst.len())
    }
}

impl fmt::Debug for ReadBuf {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "ReadBuf({})", self.n)
    }
}

fn main() {
    let mut r = Reader { buf: Bytes(b"hello".to_vec()) };
    let mut s = String::new();
    r.read_to_string(&mut s).unwrap();
    println!("{}", s);
    let mut rb = ReadBuf { n: 0 };
    let mut b = [0u8; 3];
    rb.read_exact(&mut b).unwrap();
    println!("{:?} {:?}", b, rb);
}
