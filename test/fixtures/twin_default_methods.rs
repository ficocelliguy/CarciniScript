// Impls of one trait for `[T]` and `Vec<T>` (one type in JS) that differ, where the trait has
// default methods: the `Vec` impl gets its own copies of the defaults it inherits, whose
// `self.m()` calls reach its methods (borsh's `Write` for `Vec<u8>` and `&mut [u8]`), also
// through a type parameter.
pub trait PoolIndex {
    fn head(&self) -> usize;
    fn len2(&self) -> usize {
        self.head() * 2
    }
    fn len4(&self) -> usize {
        self.len2() * 2
    }
}
impl PoolIndex for [usize] {
    fn head(&self) -> usize { self[0] }
}
impl PoolIndex for Vec<usize> {
    fn head(&self) -> usize { self[0] + 1 }
}

pub trait Write {
    fn write(&mut self, buf: &[u8]) -> usize;
    fn write_all(&mut self, buf: &[u8]) -> usize {
        let n = self.write(buf);
        n + self.write(buf)
    }
}
impl Write for Vec<u8> {
    fn write(&mut self, buf: &[u8]) -> usize {
        self.extend_from_slice(buf);
        buf.len()
    }
}
impl Write for [u8] {
    fn write(&mut self, buf: &[u8]) -> usize {
        self[0] = buf[0];
        1
    }
}

fn twice<P: PoolIndex + ?Sized>(p: &P) -> usize { p.len4() }

fn main() {
    let v = vec![5usize, 6];
    let s: &[usize] = &v;
    println!("{} {} {} {}", v.head(), s.head(), v.len2(), s.len2());
    println!("{} {}", v.len4(), s.len4());
    println!("{} {}", twice(&v), twice(s));
    let mut out: Vec<u8> = Vec::new();
    let n = out.write_all(&[1, 2]);
    println!("{} {:?}", n, out);
    let mut buf = [0u8; 2];
    let m = buf[..].write_all(&[9, 8]);
    println!("{} {:?}", m, buf);
}
