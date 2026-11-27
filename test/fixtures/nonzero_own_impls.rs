// One trait implemented for both an integer and its `NonZero*` (`impl Kind for u8` next to
// `impl Kind for NonZeroU8`): the `NonZero*` types are types of their own then, so calls pick
// the impl directly, through `T: Kind` and through `dyn Kind`; the rest of their API holds.
use std::collections::HashMap;
use std::num::{NonZero, NonZeroI32, NonZeroU8, NonZeroUsize};

trait Kind { fn kind(&self) -> String; }
impl Kind for u8 { fn kind(&self) -> String { format!("u8 {self}") } }
impl Kind for NonZero<u8> { fn kind(&self) -> String { format!("nz {}", self.get()) } }
impl Kind for i32 { fn kind(&self) -> String { format!("i32 {self}") } }
impl Kind for NonZeroI32 { fn kind(&self) -> String { format!("nzi32 {self}") } }

struct Cfg { workers: NonZeroUsize, id: std::num::NonZeroU32 }

fn show<T: Kind>(t: &T) -> String { t.kind() }
fn kinds(v: &[&dyn Kind]) -> Vec<String> { v.iter().map(|k| k.kind()).collect() }

fn main() {
    let a = NonZeroU8::new(7).unwrap();
    println!("{} {} {} {}", a.kind(), 7u8.kind(), show(&a), show(&5u8));
    let n = NonZeroI32::new(-4).unwrap();
    println!("{:?}", kinds(&[&a, &5u8, &n, &3i32]));
    println!("{:?}", [NonZeroU8::new(1).unwrap(), a].iter().map(|x| x.kind()).collect::<Vec<_>>());
    println!("[{:>4}] [{:<3}] {:?} {}", a, a, a, a.get() as u32 * 100);
    println!("{:?} {:?}", NonZeroU8::new(0), NonZeroU8::new(3));
    println!("{} {} {}", NonZeroU8::MIN, NonZeroU8::MAX, NonZeroU8::BITS);
    let mut v: Vec<NonZeroU8> = [5u8, 1, 9, 3].iter().filter_map(|&x| NonZeroU8::new(x)).collect();
    v.sort();
    println!("{:?} max={:?}", v, v.iter().max());
    let mut m: HashMap<NonZeroU8, &str> = HashMap::new();
    m.insert(a, "seven");
    println!("{:?}", m.get(&NonZeroU8::new(7).unwrap()));
    let x: u8 = a.into();
    println!("{} {} {}", x, u8::from(a), a == NonZero::<u8>::new(7).unwrap());
    println!("{:?} {:?} {}", a.checked_add(250), a.checked_mul(a), a.is_power_of_two());
    println!("{} {} {} {}", n, -n, n.abs(), n.is_negative());
    let c = Cfg { workers: NonZeroUsize::new(4).unwrap(), id: std::num::NonZeroU32::new(12).unwrap() };
    println!("{} {}", c.workers.get() * 2, c.id);
    let o: Option<NonZeroU8> = None;
    println!("{} {}", o.map_or(0, |n| n.get()), (a | NonZeroU8::new(8).unwrap()).get());
}
