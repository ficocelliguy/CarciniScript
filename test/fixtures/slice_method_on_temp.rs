// `copy_from_slice` / `clone_from_slice` and other in-place methods on a range of a Vec that
// isn't a place (`self.write_buf()[..n].copy_from_slice(..)`): the Vec is evaluated once.
struct Buf { data: Vec<u8>, calls: u32 }
impl Buf {
    fn write_buf(&mut self) -> &mut Vec<u8> { self.calls += 1; &mut self.data }
    fn slice(&mut self) -> &mut [u8] { self.calls += 1; &mut self.data[..] }
    fn put(&mut self, src: &[u8]) {
        let n = src.len();
        self.write_buf()[..n].copy_from_slice(src);
        self.slice()[n..n + 1].clone_from_slice(&[9]);
        self.write_buf()[1..][..2].reverse();
        self.slice()[2..].sort();
    }
}
fn main() {
    let mut b = Buf { data: vec![0; 6], calls: 0 };
    b.put(&[1, 2, 3]);
    println!("{:?} {}", b.data, b.calls);
}
