// One trait for tuples and `Vec`s, as borsh has it: both are JS arrays, so a call through a
// generic `T: Enc` goes by `T`'s class, and a tuple impl's methods get their elements' classes.
trait Enc {
    fn enc(&self, out: &mut Vec<String>);
}

impl Enc for u8 {
    fn enc(&self, out: &mut Vec<String>) {
        out.push(format!("u8 {self}"));
    }
}

impl Enc for String {
    fn enc(&self, out: &mut Vec<String>) {
        out.push(format!("str {self:?}"));
    }
}

impl<A: Enc, B: Enc> Enc for (A, B) {
    fn enc(&self, out: &mut Vec<String>) {
        out.push("(".to_string());
        self.0.enc(out);
        self.1.enc(out);
        out.push(")".to_string());
    }
}

impl<A: Enc, B: Enc, C: Enc> Enc for (A, B, C) {
    fn enc(&self, out: &mut Vec<String>) {
        out.push("3(".to_string());
        self.0.enc(out);
        self.1.enc(out);
        self.2.enc(out);
        out.push(")".to_string());
    }
}

impl<T: Enc> Enc for Vec<T> {
    fn enc(&self, out: &mut Vec<String>) {
        out.push(format!("vec {}", self.len()));
        for x in self {
            x.enc(out);
        }
    }
}

fn write<T: Enc>(x: &T) -> String {
    let mut out = Vec::new();
    x.enc(&mut out);
    out.join(" ")
}

// Hands its `T` on to `write`.
fn both<T: Enc>(a: T, b: T) -> String {
    format!("{} | {}", write(&a), write(&b))
}

// `&mut self` methods, through the value's cell (`$dcallMutT`).
trait Bump {
    fn bump(&mut self);
}

impl Bump for u8 {
    fn bump(&mut self) {
        *self += 1;
    }
}

impl<A: Bump, B: Bump> Bump for (A, B) {
    fn bump(&mut self) {
        self.0.bump();
        self.1.bump();
        self.1.bump();
    }
}

impl<T: Bump> Bump for Vec<T> {
    fn bump(&mut self) {
        for x in self.iter_mut() {
            x.bump();
        }
        self.reverse();
    }
}

fn bump_twice<T: Bump>(x: &mut T) {
    x.bump();
    x.bump();
}

// An impl on a slice is a `Vec`'s.
trait Count {
    fn count(&self) -> usize;
}

impl Count for (u8, u8) {
    fn count(&self) -> usize {
        2
    }
}

impl<T> Count for [T] {
    fn count(&self) -> usize {
        100 + self.len()
    }
}

fn count_of<T: Count + ?Sized>(x: &T) -> usize {
    x.count()
}

fn main() {
    println!("{}", write(&(1u8, 2u8)));
    println!("{}", write(&vec![1u8, 2u8]));
    println!("{}", write(&(1u8, "a".to_string(), 3u8)));
    println!("{}", write(&vec![1u8, 2u8, 3u8]));
    println!("{}", write(&vec![(1u8, 2u8), (3u8, 4u8)]));
    println!("{}", write(&(vec![5u8, 6u8], 7u8)));
    println!("{}", write(&((1u8, 2u8), vec![(3u8, 4u8)])));
    println!("{}", both(vec![8u8, 9u8], vec![]));
    println!("{}", both((8u8, 9u8), (0u8, 1u8)));
    let mut out = Vec::new();
    (1u8, 2u8).enc(&mut out);
    vec![1u8, 2u8].enc(&mut out);
    println!("{}", out.join(" "));
    let mut t = (1u8, 2u8);
    bump_twice(&mut t);
    let mut v = vec![1u8, 2u8];
    bump_twice(&mut v);
    let mut vt = vec![(1u8, 1u8), (5u8, 5u8)];
    bump_twice(&mut vt);
    println!("{t:?} {v:?} {vt:?}");
    println!("{} {}", count_of(&(1u8, 2u8)), count_of(&[1u8, 2u8][..]));
    println!("{}", count_of(vec![3u8, 4u8].as_slice()));
}
