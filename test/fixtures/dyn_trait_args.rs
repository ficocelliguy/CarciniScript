// A generic trait implemented several times for one type where the impls differ only by number
// type (`Scale<f64>` / `Scale<i32>`) or only by return type (`Conv<u8>` / `Conv<u16>`), called
// through `dyn Scale<f64>`: the trait arguments written on the `dyn` pick the impl, also in
// boxes, collections, generic functions over `dyn Scale<R>` and qualified calls.
trait Scale<R> { fn scale(&self, r: R) -> String; }
struct M(i32);
struct N;
impl Scale<f64> for M { fn scale(&self, r: f64) -> String { format!("M f64 {}", self.0 as f64 * r) } }
impl Scale<i32> for M { fn scale(&self, r: i32) -> String { format!("M i32 {}", self.0 * r) } }
impl Scale<f64> for N { fn scale(&self, r: f64) -> String { format!("N {r}") } }

trait Conv<T> { fn conv(&self) -> T; }
impl Conv<u8> for M { fn conv(&self) -> u8 { self.0 as u8 } }
impl Conv<u16> for M { fn conv(&self) -> u16 { (self.0 * 1000) as u16 } }

fn by_dyn(s: &dyn Scale<f64>) -> String { s.scale(2.0) }
fn by_dyn_i(s: &dyn Scale<i32>) -> String { s.scale(2) }
fn conv_dyn(c: &dyn Conv<u16>) -> u16 { c.conv() }
fn mk(b: bool) -> Box<dyn Scale<f64>> { if b { Box::new(M(2)) } else { Box::new(N) } }
fn through<R>(s: &dyn Scale<R>, r: R) -> String { s.scale(r) }

fn main() {
    let m = M(3);
    println!("{} | {}", by_dyn(&m), by_dyn_i(&m));
    println!("{}", conv_dyn(&m));
    let v: Vec<Box<dyn Scale<f64>>> = vec![mk(true), mk(false)];
    for s in &v {
        println!("{}", s.scale(3.0));
    }
    let w: Vec<&dyn Scale<i32>> = vec![&m, &M(4)];
    println!("{:?}", w.iter().map(|s| s.scale(2)).collect::<Vec<_>>());
    let cs: Vec<Box<dyn Conv<u8>>> = vec![Box::new(M(300))];
    for c in &cs {
        println!("{}", c.conv());
    }
    println!("{} {}", through(&m, 2.5), through::<i32>(&m, 2));
    let d: &dyn Conv<u8> = &m;
    println!("{} {}", Conv::conv(d), <M as Conv<u16>>::conv(&m));
    println!("{} {}", Conv::<u8>::conv(&m), Scale::<f64>::scale(&m, 0.5));
}
