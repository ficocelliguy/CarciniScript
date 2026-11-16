// A `const` item as a const argument in a struct field's type (`History<WINDOW>`), next to a
// literal one.
const WINDOW: usize = 3;

#[derive(Debug, Default)]
struct History<const N: usize> {
    values: Vec<f64>,
}

impl<const N: usize> History<N> {
    fn push(&mut self, x: f64) {
        self.values.push(x);
        if self.values.len() > N {
            self.values.remove(0);
        }
    }
    fn is_full(&self) -> bool {
        self.values.len() == N
    }
}

struct Market {
    short: History<WINDOW>,
    long: History<5>,
}

fn main() {
    let mut m = Market { short: History::default(), long: History::default() };
    for i in 0..4 {
        m.short.push(i as f64);
        m.long.push(i as f64 * 2.0);
    }
    println!("{:?} {} {:?} {}", m.short, m.short.is_full(), m.long.values, m.long.is_full());
}
