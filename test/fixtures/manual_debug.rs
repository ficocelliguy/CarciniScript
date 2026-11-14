// A hand-written `impl fmt::Debug` next to an `impl fmt::Display` on the same type (two `fmt`s),
// used by `{:?}` directly, inside `Vec`/`Option`/tuples and derived `Debug` of a containing struct.
use std::fmt;

struct Money(i64);

impl fmt::Display for Money {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "${}.{:02}", self.0 / 100, self.0 % 100)
    }
}

impl fmt::Debug for Money {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Money({} cents)", self.0)
    }
}

#[derive(Debug)]
struct Wallet {
    owner: String,
    cash: Money,
    history: Vec<Money>,
}

fn main() {
    let m = Money(12345);
    println!("{} {:?}", m, m);
    let w = Wallet { owner: "me".into(), cash: Money(500), history: vec![Money(1), Money(250)] };
    println!("{:?}", w);
    println!("{:?} {:?}", Some(Money(7)), (Money(1), 2));
}
