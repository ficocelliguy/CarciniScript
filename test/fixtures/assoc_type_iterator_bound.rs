// An associated type its trait bounds by `Iterator<Item = X>` (`type Iter: Iterator<Item =
// u32>;`): iterator methods on `I::Iter`, also with an associated-type binding on the bound
// (`I: Source<Error = String>`).
trait Source {
    type Error;
    type Iter: Iterator<Item = u32>;
    fn items(&self) -> Self::Iter;
    fn fail(&self) -> Self::Error;
}
struct V(Vec<u32>);
impl Source for V {
    type Error = String;
    type Iter = std::vec::IntoIter<u32>;
    fn items(&self) -> Self::Iter { self.0.clone().into_iter() }
    fn fail(&self) -> String { "bad".to_string() }
}
struct Up(u32);
impl Source for Up {
    type Error = String;
    type Iter = std::ops::Range<u32>;
    fn items(&self) -> Self::Iter { 0..self.0 }
    fn fail(&self) -> String { format!("up {}", self.0) }
}
fn sum<I: Source<Error = String>>(s: &I) -> u32 { s.items().map(|x| x * 2).sum() }
fn first<I: Source<Error = String>>(s: &I) -> Option<u32> { let mut it = s.items(); it.next() }
fn err<I: Source<Error = String>>(s: &I) -> usize { s.fail().len() }
fn all<I: Source>(s: &I) -> Vec<u32> { let mut out = Vec::new(); for x in s.items() { out.push(x); } out }
fn main() {
    let v = V(vec![1, 2, 3]);
    println!("{} {:?} {} {:?}", sum(&v), first(&v), err(&v), all(&v));
    let u = Up(4);
    println!("{} {:?} {} {:?}", sum(&u), first(&Up(0)), err(&u), all(&u));
}
