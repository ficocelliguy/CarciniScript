// Assigning to an index of a call's `Vec` or slice (`b.slice_mut()[0] = 1`): the call runs once.
struct B { v: Vec<u8>, calls: u32 }
impl B {
    fn slice_mut(&mut self) -> &mut [u8] { self.calls += 1; &mut self.v }
    fn inner(&mut self) -> &mut Vec<u8> { self.calls += 1; &mut self.v }
}
fn rows(g: &mut Vec<Vec<i32>>) -> &mut Vec<i32> { &mut g[1] }
fn main() {
    let mut b = B { v: vec![0, 0, 0], calls: 0 };
    b.slice_mut()[0] = 1;
    b.inner()[1] = 2;
    b.inner()[2] += 5;
    println!("{:?} {}", b.v, b.calls);
    let mut g = vec![vec![1, 2], vec![3, 4]];
    rows(&mut g)[0] *= 10;
    println!("{:?}", g);
}
