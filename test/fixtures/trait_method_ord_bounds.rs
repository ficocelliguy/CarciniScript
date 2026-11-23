// `Ord` / `Eq` bounds on a trait method's own type parameters imply `PartialOrd` / `PartialEq`
// in its default body, inline or in a `where` clause.
trait Pick {
    fn pick<B: Ord>(&self, a: B, b: B) -> B {
        if a > b { a } else { b }
    }
    fn same<B>(&self, a: B, b: B) -> bool
    where
        B: Eq,
    {
        a == b
    }
}

struct P;
impl Pick for P {}

fn main() {
    println!("{} {} {} {}", P.pick(3, 9), P.pick("b", "a"), P.same("a", "a"), P.same(1, 2));
}
