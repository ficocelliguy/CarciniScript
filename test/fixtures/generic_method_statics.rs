// `T::make()`, `T::CONST` and `T::default()` on a method's own type parameter (`fn get<T>`),
// with turbofish and inferred type arguments, and handed on to other generic functions.
trait Unit {
    const NAME: &'static str;
    fn make(n: i32) -> Self;
    fn describe(&self) -> String;
}

#[derive(Debug, Default)]
struct Meters(i32);

#[derive(Debug, Default)]
struct Feet(i32);

impl Unit for Meters {
    const NAME: &'static str = "m";
    fn make(n: i32) -> Self {
        Meters(n)
    }
    fn describe(&self) -> String {
        format!("{}m", self.0)
    }
}

impl Unit for Feet {
    const NAME: &'static str = "ft";
    fn make(n: i32) -> Self {
        Feet(n * 3)
    }
    fn describe(&self) -> String {
        format!("{}ft", self.0)
    }
}

fn make_free<T: Unit>(n: i32) -> T {
    T::make(n)
}

struct Store {
    values: Vec<i32>,
}

impl Store {
    fn get<T: Unit>(&self, i: usize) -> T {
        T::make(self.values[i])
    }

    fn label<T: Unit>(&self) -> String {
        format!("{} values in {}", self.values.len(), T::NAME)
    }

    /// Hands `T` on to a free function and to another method.
    fn both<T: Unit>(&self) -> (T, T) {
        (make_free::<T>(self.values[0]), self.get::<T>(1))
    }

    fn empty<T: Default + std::fmt::Debug>() -> String {
        format!("{:?}", T::default())
    }
}

/// A generic `impl`'s associated functions: `T::make()` for the impl's own `T`.
struct Pairs<T> {
    items: Vec<T>,
}

impl<T: Unit> Pairs<T> {
    fn from_counts(counts: &[i32]) -> Pairs<T> {
        Pairs { items: counts.iter().map(|n| T::make(*n)).collect() }
    }

    fn unit_name() -> &'static str {
        T::NAME
    }
}

fn apply(f: fn(&[i32]) -> Pairs<Feet>, counts: &[i32]) -> usize {
    f(counts).items.len()
}

fn main() {
    let s = Store { values: vec![2, 5] };
    let a: Meters = s.get(0);
    let b = s.get::<Feet>(1);
    println!("{} {}", a.describe(), b.describe());
    println!("{} / {}", s.label::<Meters>(), s.label::<Feet>());
    let (x, y) = s.both::<Feet>();
    println!("{} {}", x.describe(), y.describe());
    println!("{} {}", Store::empty::<Meters>(), Store::empty::<Feet>());
    let p = Pairs::<Feet>::from_counts(&[1, 2]);
    let q: Pairs<Meters> = Pairs::from_counts(&[4]);
    let shown: Vec<String> = p.items.iter().map(|x| x.describe()).collect();
    println!("{:?} {} {} {}", shown, q.items[0].describe(), Pairs::<Meters>::unit_name(), Pairs::<Feet>::unit_name());
    println!("{}", apply(Pairs::<Feet>::from_counts, &[1, 2, 3]));
}
