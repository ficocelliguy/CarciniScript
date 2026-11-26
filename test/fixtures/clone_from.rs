// `a.clone_from(&b)`: `*a = b.clone()` on numbers, strings, `Option`s, tuples, `Vec`s and derived
// `Clone` structs (in place, so a `&mut` to it sees the copy), through `S: Clone`, and your own
// `clone_from` when an `impl Clone` has one.
#[derive(Debug, Clone)]
struct Inv {
    items: Vec<String>,
    gold: u32,
}
#[derive(Debug)]
struct Wrap<T> {
    inner: T,
    copies: u32,
}
impl<T: Clone> Clone for Wrap<T> {
    fn clone(&self) -> Self {
        Wrap { inner: self.inner.clone(), copies: self.copies + 1 }
    }
    fn clone_from(&mut self, o: &Self) {
        self.inner.clone_from(&o.inner);
        self.copies += 100;
    }
}
fn reset<S: Clone>(a: &mut S, b: &S) {
    a.clone_from(b);
}
fn main() {
    let base = Inv { items: vec!["sword".to_string()], gold: 5 };
    let mut cur = Inv { items: vec![], gold: 0 };
    {
        let r = &mut cur;
        r.clone_from(&base);
        r.gold += 1;
    }
    println!("{:?} {:?}", cur, base);
    let mut w = Wrap { inner: 1.5, copies: 0 };
    w.clone_from(&Wrap { inner: 2.5, copies: 7 });
    let mut wv = Wrap { inner: vec![1], copies: 0 };
    wv.clone_from(&Wrap { inner: vec![2, 3], copies: 0 });
    println!("{:?} {:?} {:?}", w, wv, wv.clone());
    let mut n = 3u64;
    reset(&mut n, &9);
    let mut inv = Inv { items: vec![], gold: 1 };
    reset(&mut inv, &base);
    let mut t = (1, "a".to_string());
    reset(&mut t, &(2, "b".to_string()));
    let mut o: Option<u8> = None;
    o.clone_from(&Some(4));
    println!("{} {:?} {:?} {:?}", n, inv, t, o);
    let mut s = String::from("x");
    s.clone_from(&"yy".to_string());
    let mut k = 1;
    k.clone_from(&5);
    let mut v = vec![1];
    v.clone_from(&vec![2, 3]);
    println!("{} {} {:?}", s, k, v);
}
