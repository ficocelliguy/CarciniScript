// Const generic parameters on traits (`trait Buf<const N: usize>`, `impl Buf<4> for X`,
// `impl<const M: usize> Buf<M> for Arr<M>`, `B: Buf<4>` bounds), and a trait implemented for
// arrays and slices (`impl<const N: usize> Tr for [u8; N]`, `impl Tr for [i32]`).
trait Buf<const N: usize> {
    fn size(&self) -> usize;
    fn bytes(&self) -> [u8; N];
    fn twice(&self) -> usize {
        self.size() * 2
    }
}

struct X;

impl Buf<4> for X {
    fn size(&self) -> usize {
        4
    }
    fn bytes(&self) -> [u8; 4] {
        [1, 2, 3, 4]
    }
}

struct Arr<const M: usize> {
    v: [i32; M],
}

impl<const M: usize> Buf<M> for Arr<M> {
    fn size(&self) -> usize {
        M + self.v.len()
    }
    fn bytes(&self) -> [u8; M] {
        [7; M]
    }
}

trait Conv<T, const K: usize> {
    fn conv(&self) -> T;
}

impl Conv<String, 3> for X {
    fn conv(&self) -> String {
        "x".repeat(3)
    }
}

fn total<B: Buf<4>>(b: &B) -> usize {
    b.size() + b.bytes().len()
}

trait Sum3 {
    fn total(&self) -> u32;
}

impl<const N: usize> Sum3 for [u8; N] {
    fn total(&self) -> u32 {
        self.iter().map(|&b| b as u32).sum::<u32>() + N as u32
    }
}

trait First {
    fn first_or(&self, d: i32) -> i32;
}

impl First for [i32] {
    fn first_or(&self, d: i32) -> i32 {
        if self.is_empty() { d } else { self[0] }
    }
}

fn main() {
    println!("{} {} {:?}", X.size(), X.twice(), X.bytes());
    let a = Arr { v: [1, 2] };
    println!("{} {:?}", a.twice(), a.bytes());
    println!("{}", total(&X));
    println!("{}", X.conv());
    let b = [1u8, 2, 3];
    println!("{}", b.total());
    println!("{}", [9u8; 5].total());
    let v = vec![4, 5];
    println!("{} {}", v.first_or(0), v[..0].first_or(-1));
}
