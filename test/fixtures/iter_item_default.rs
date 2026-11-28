// `Default::default()` / `unwrap_or_default()` / `I::Item::default()` and operators on the items
// of an `I: Iterator` / `I: IntoIterator` bounded `where I::Item: Default + Add` (itertools' `concat`):
// `I::Item`'s class is handed over like a type parameter's.
use std::ops::Add;
#[derive(Debug, Default, Clone, Copy, PartialEq)]
struct V(i32, i32);
impl Add for V {
    type Output = V;
    fn add(self, o: V) -> V { V(self.0 + o.0, self.1 + o.1) }
}
fn concat<I>(mut it: I) -> I::Item
where
    I: Iterator,
    I::Item: Default + Add<Output = I::Item>,
{
    let first = it.next().unwrap_or_default();
    it.fold(first, |a, b| a + b)
}
fn total<I>(it: I) -> I::Item
where
    I: Iterator,
    I::Item: Default + Add<Output = I::Item>,
{
    let mut acc: I::Item = Default::default();
    for x in it {
        acc = acc + x;
    }
    acc
}
fn sum_all<I>(xs: I) -> I::Item
where
    I: IntoIterator,
    I::Item: Default + Add<Output = I::Item>,
{
    let mut acc = I::Item::default();
    for x in xs {
        acc = acc + x;
    }
    acc
}
fn first_or_default<I: Iterator>(mut it: I) -> I::Item
where
    I::Item: Default,
{
    it.next().unwrap_or_default()
}
fn forward<T: Default + Add<Output = T>>(v: Vec<T>) -> T {
    sum_all(v)
}
fn main() {
    println!("{}", concat(vec![1, 2, 3].into_iter()));
    println!("{}", concat(Vec::<i32>::new().into_iter()));
    println!("{:?}", concat(vec![V(1, 2), V(3, 4)].into_iter()));
    println!("{:?}", concat(Vec::<V>::new().into_iter()));
    println!("{}", total(vec![1.5, 2.0].into_iter()));
    println!("{}", total(Vec::<u8>::new().into_iter()));
    println!("{:?}", total(Vec::<V>::new().into_iter()));
    println!("{:?}", sum_all(vec![V(1, 1), V(2, 3)]));
    println!("{:?}", sum_all(Vec::<V>::new()));
    println!("{}", sum_all([1u8, 2, 3]));
    println!("{:?}", first_or_default(Vec::<V>::new().into_iter()));
    println!("{:?}", first_or_default(std::iter::empty::<String>()));
    println!("{:?}", forward(vec![V(5, 5)]));
    println!("{:?}", forward(Vec::<V>::new()));
    println!("{}", forward(Vec::<f64>::new()));
}
