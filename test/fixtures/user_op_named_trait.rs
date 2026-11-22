// A trait of the file's own named like an operator trait is that trait, not `std::ops`'s: its
// methods take any signature and are called through the type, the trait or as a method.
trait Add {
    fn add(&self, n: i32) -> i32;
    fn twice(&self, n: i32) -> i32 {
        self.add(self.add(n))
    }
}

trait Neg {
    fn neg(&self) -> String;
}

struct C(i32);

impl Add for C {
    fn add(&self, n: i32) -> i32 {
        self.0 + n
    }
}

impl Neg for C {
    fn neg(&self) -> String {
        format!("-{}", self.0)
    }
}

impl Add for i32 {
    fn add(&self, n: i32) -> i32 {
        self * 10 + n
    }
}

fn total<T: Add>(t: &T, n: i32) -> i32 {
    t.add(n) + t.twice(n)
}

fn main() {
    let c = C(5);
    println!("{}", C::add(&c, 2));
    println!("{}", Add::add(&c, 3));
    println!("{}", <C as Add>::add(&c, 4));
    println!("{}", c.add(1));
    println!("{}", c.twice(1));
    println!("{}", total(&c, 2));
    println!("{} {}", c.neg(), Neg::neg(&c));
    println!("{} {}", 3i32.add(4), total(&2, 1));
}
