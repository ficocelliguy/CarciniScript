// Impls of one trait for types that are one value in JS: a concrete `&T` impl next to `T`'s
// (num-traits' `Inv for &f32`, form_urlencoded's `Target for &mut String`), picked per call by
// whether the receiver is written as a reference and through a type parameter by the reference's
// class; and a `[u8]` impl next to a generic `[T]` one (wasm-encoder), picked by the element type.
trait Inv { fn inv(self) -> f32; fn name(&self) -> String; }
impl Inv for f32 { fn inv(self) -> f32 { 1.0 / self } fn name(&self) -> String { "f32".into() } }
impl Inv for &f32 { fn inv(self) -> f32 { 2.0 / *self } fn name(&self) -> String { "&f32".into() } }
fn through<T: Inv>(t: T) -> f32 { t.inv() }
fn by_ref(r: &f32) -> f32 { r.inv() }
fn main_refs() {
    let x = 4.0f32;
    let r = &x;
    println!("{} {} {} {}", x.inv(), (&x).inv(), r.inv(), by_ref(&x));
    println!("{} {}", x.name(), r.name());
    println!("{} {}", through(x), through(&x));
}
trait Target { fn finish(self) -> String; fn put(&mut self, s: &str); }
impl Target for String {
    fn finish(self) -> String { self }
    fn put(&mut self, s: &str) { self.push_str(s); }
}
impl<'a> Target for &'a mut String {
    fn finish(self) -> String { format!("[{}]", self) }
    fn put(&mut self, s: &str) { self.push('<'); self.push_str(s); self.push('>'); }
}
trait Half { fn half(self) -> f32; }
impl Half for &f32 { fn half(self) -> f32 { *self / 4.0 } }
impl Half for f32 { fn half(self) -> f32 { self / 2.0 } }
fn main_order() {
    let x = 4.0f32;
    println!("{} {}", x.half(), (&x).half());
    let mut s = String::new();
    s.put("a");
    let r = &mut s;
    let out = r.finish();
    println!("{out}");
    println!("{}", s.finish());
}
trait Enc { fn enc(&self) -> String; }
impl Enc for i32 { fn enc(&self) -> String { format!("i{self}") } }
impl Enc for String { fn enc(&self) -> String { format!("s{self}") } }
impl<T: Enc> Enc for [T] { fn enc(&self) -> String { self.iter().map(|x| x.enc()).collect::<Vec<_>>().join(",") } }
impl Enc for [u8] { fn enc(&self) -> String { format!("bytes{}", self.len()) } }
fn put<E: Enc + ?Sized>(e: &E) -> String { e.enc() }
fn main_slices() {
    let v = vec![1i32, 2];
    let b = vec![1u8, 2, 3];
    println!("{} {}", v[..].enc(), b[..].enc());
    println!("{} {}", v.as_slice().enc(), b.as_slice().enc());
    let s: &[u8] = &b;
    println!("{} {}", s.enc(), put(s));
    println!("{} {}", put(&v[..]), put(&b[..]));
    let w = vec!["a".to_string()];
    println!("{}", put(&w[..]));
}
trait K { fn k(&self) -> String; }
impl K for [u8] { fn k(&self) -> String { format!("u8 {}", self.len()) } }
impl K for [u16] { fn k(&self) -> String { format!("u16 {}", self.len()) } }
fn g<T: K + ?Sized>(t: &T) -> String { t.k() }
fn main_widths() {
    let a = vec![1u8];
    let b = vec![1u16, 2];
    println!("{} {} {} {}", a[..].k(), b.as_slice().k(), g(&a[..]), g(&b[..]));
}
fn main() {
    main_refs();
    main_order();
    main_slices();
    main_widths();
}
