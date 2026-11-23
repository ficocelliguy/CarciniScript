// `OnceCell::from(x)` / `x.into()`, and `Cow<[T]>` from a `Vec<T>`.
use std::borrow::Cow;
use std::cell::OnceCell;
use std::sync::OnceLock;

fn main() {
    let c: OnceCell<u8> = OnceCell::from(4);
    let d: OnceCell<String> = String::from("five").into();
    let l = OnceLock::from(6);
    println!("{:?} {:?} {:?}", c.get(), d.get(), l.get());
    println!("{:?}", c.set(9));
    let w: Cow<[u8]> = Cow::from(vec![1u8, 2]);
    let x: Cow<[u8]> = vec![3u8].into();
    println!("{:?} {:?} {}", w, x, w.len() + x.len());
    println!("{:?}", w.into_owned());
}
