// A struct's type argument inferred from the one impl of the bound it declares on it
// (`impl RawStream for Vec<u8>`, so `Auto::always(Vec::new())` is an `Auto<Vec<u8>>`).
trait RawStream {
    fn put(&mut self, b: u8);
}

impl RawStream for Vec<u8> {
    fn put(&mut self, b: u8) {
        self.push(b);
    }
}

struct Auto<S: RawStream> {
    s: S,
}

impl<S: RawStream> Auto<S> {
    fn always(s: S) -> Self {
        Auto { s }
    }
    fn put(&mut self, b: u8) {
        self.s.put(b);
    }
}

trait Store {
    fn keep(&mut self, k: u16);
    fn total(&self) -> u32;
}

impl Store for Vec<u16> {
    fn keep(&mut self, k: u16) {
        self.push(k);
    }
    fn total(&self) -> u32 {
        self.iter().map(|&k| k as u32).sum()
    }
}

// Through a `where` clause, and built with a struct literal.
struct Shelf<T>
where
    T: Store,
{
    items: T,
    name: &'static str,
}

fn main() {
    let mut a = Auto::always(Vec::new());
    a.put(3);
    a.put(250);
    println!("{:?}", a.s);
    let mut s = Shelf { items: Vec::new(), name: "top" };
    s.items.keep(40000);
    s.items.keep(30000);
    println!("{} {} {:?}", s.name, s.items.total(), s.items);
}
