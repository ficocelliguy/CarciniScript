// Impls of one trait for arrays of one element type and different lengths (`[u8; 1]`, `[u8; 2]`)
// that behave differently, one class in JS: each method picks the impl by the array's length at
// run time, also through a type parameter and in a trait's default method; another element
// type (`[u16; 2]`) is told apart by its elements as before.
pub trait Bump {
    fn bump(&mut self);
    fn describe(&self, tag: &str) -> String;
    fn twice(&mut self) {
        self.bump();
        self.bump();
    }
}
impl Bump for [u8; 1] {
    fn bump(&mut self) {
        self[0] += 1;
    }
    fn describe(&self, tag: &str) -> String {
        format!("{tag}: one {}", self[0])
    }
}
impl Bump for [u8; 2] {
    fn bump(&mut self) {
        self[0] += 2;
        self[1] = self[0] * 10;
    }
    fn describe(&self, tag: &str) -> String {
        format!("{tag}: two {:?}", self)
    }
}
impl Bump for [u16; 2] {
    fn bump(&mut self) {
        self[1] = 300;
    }
    fn describe(&self, tag: &str) -> String {
        format!("{tag}: wide {:?}", self)
    }
}
macro_rules! sized {
    ($($n:literal => $v:literal),*) => {
        pub trait Size { fn size(&self) -> u32; }
        $(impl Size for [bool; $n] { fn size(&self) -> u32 { $v } })*
    };
}
sized!(1 => 10, 2 => 20, 3 => 30);
fn go<F: Bump>(f: &mut F) -> String {
    f.twice();
    f.describe("generic")
}
fn main() {
    let mut a = [0u8; 1];
    let mut b = [0u8; 2];
    let mut c = [0u16; 2];
    a.bump();
    b.bump();
    c.bump();
    println!("{} | {} | {}", a.describe("a"), b.describe("b"), c.describe("c"));
    b.twice();
    println!("{}", b.describe("b"));
    println!("{} | {}", go(&mut a), go(&mut b));
    println!("{} {} {}", [true].size(), [true, false].size(), [false; 3].size());
}
