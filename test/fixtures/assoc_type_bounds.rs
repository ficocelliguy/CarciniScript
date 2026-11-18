// Associated type bounds (`I: IntoIterator<Item: Display>`): the same as a `where I::Item:
// Display` clause; in argument position (`impl Iterator<Item: Into<i64>>`), a type parameter.
use std::fmt::{Debug, Display};

fn show_all<I: IntoIterator<Item: Display>>(items: I) -> String {
    items.into_iter().map(|x| format!("[{}]", x)).collect::<Vec<_>>().join("")
}

fn debug_all<I>(items: I) -> Vec<String>
where
    I: Iterator<Item: Debug + Clone>,
{
    items.map(|x| format!("{:?}", x.clone())).collect()
}

fn sum_all(it: impl Iterator<Item: Into<i64>>) -> i64 {
    it.map(|x| x.into()).sum()
}

fn pair_up<A, B>(a: A, b: B) -> Vec<String>
where
    A: IntoIterator<Item: Display>,
    B: IntoIterator<Item: Display>,
{
    a.into_iter().zip(b).map(|(x, y)| format!("{}-{}", x, y)).collect()
}

trait Shape {
    type Unit;
    fn size(&self) -> Self::Unit;
}

struct Sq(f64);

impl Shape for Sq {
    type Unit = f64;
    fn size(&self) -> f64 {
        self.0 * self.0
    }
}

fn describe<S: Shape<Unit: Display>>(s: &S) -> String {
    format!("size {}", s.size())
}

struct Labels<I> {
    items: I,
}

impl<I: Iterator<Item: Display>> Labels<I> {
    fn render(self) -> String {
        self.items.map(|x| x.to_string()).collect::<Vec<_>>().join(",")
    }
}

fn main() {
    println!("{}", show_all(vec![1, 2, 3]));
    println!("{}", show_all(["a", "b"]));
    println!("{:?}", debug_all(vec!["x", "y"].into_iter()));
    println!("{}", sum_all(vec![1i32, 2, 3].into_iter()));
    println!("{}", sum_all([4u8, 5].into_iter()));
    println!("{:?}", pair_up(vec![1, 2], vec!['a', 'b']));
    println!("{}", describe(&Sq(1.5)));
    let l = Labels { items: vec![7, 8, 9].into_iter() };
    println!("{}", l.render());
}
