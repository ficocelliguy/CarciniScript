// `T: AsRef<[u8]>` on an impl's type parameter: a value of the struct holds whatever it was built
// from (a `Vec<u8>`, a `&str`, an array), and `as_ref()` reads it as bytes.
struct Hasher<T> { key: T }
impl<T: AsRef<[u8]>> Hasher<T> {
    fn sum(&self) -> u32 { self.key.as_ref().iter().map(|&b| b as u32).sum() }
    fn len(&self) -> usize { self.key.as_ref().len() }
}
struct Named<S> { name: S }
impl<S> Named<S> where S: AsRef<[u8]> {
    fn first(&self) -> Option<u8> { self.name.as_ref().first().copied() }
}
fn total<T: AsRef<[u8]>>(x: T) -> usize { x.as_ref().len() }
fn main() {
    let h = Hasher { key: vec![1u8, 2, 3] };
    let s = Hasher { key: "abc" };
    let a = Hasher { key: [9u8; 4] };
    let u = Hasher { key: String::from("héllo") };
    println!("{} {} {} {} {}", h.sum(), h.len(), s.sum(), a.sum(), total("hello") + total(vec![1u8]));
    println!("{} {}", u.len(), u.sum());
    println!("{:?} {:?}", Named { name: "xyz" }.first(), Named { name: Vec::<u8>::new() }.first());
}
