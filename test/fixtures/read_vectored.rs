// `read_vectored` over `IoSliceMut`s: std's default (the first non-empty buffer) for your own
// reader, buffer after buffer for `Cursor` and `BufReader`; the buffers are the caller's bytes.
use std::io::{self, BufReader, IoSliceMut, Read};
fn fill<R: Read>(r: &mut R) -> io::Result<usize> {
    let mut a = [0u8; 3];
    let mut b = [0u8; 4];
    let mut bufs = [IoSliceMut::new(&mut a), IoSliceMut::new(&mut b)];
    let n = r.read_vectored(&mut bufs)?;
    println!("{:?} {:?}", a, b);
    Ok(n)
}
struct Ones(usize);
impl Read for Ones {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = buf.len().min(self.0);
        for i in 0..n { buf[i] = 1; }
        self.0 -= n;
        Ok(n)
    }
}
fn main() {
    let mut c = io::Cursor::new(vec![1u8, 2, 3, 4, 5]);
    println!("{:?}", fill(&mut c).unwrap());
    println!("{:?}", fill(&mut c).unwrap());
    let mut o = Ones(10);
    println!("{:?}", fill(&mut o).unwrap());
    let mut br = BufReader::new(Ones(5));
    println!("{:?}", fill(&mut br).unwrap());
    let mut x = [0u8; 2];
    let mut s = IoSliceMut::new(&mut x);
    println!("{}", s.len());
    s[0] = 9;
    println!("{:?}", &*s);
    println!("{:?}", x);
}
