// A user trait implemented for built-in types and a struct, called through a generic `T: Trait`
// and through `Box<dyn Trait>`: the runtime finds the impl for numbers (integer vs float),
// strings and Vecs (`κσ.$dcall`).
trait Describe {
    fn describe(&self) -> String;
}
impl Describe for f64 {
    fn describe(&self) -> String { format!("float {self:.2}") }
}
impl Describe for i32 {
    fn describe(&self) -> String { format!("int {self}") }
}
impl Describe for String {
    fn describe(&self) -> String { format!("text {:?}", self) }
}
impl Describe for Vec<i32> {
    fn describe(&self) -> String { format!("{} ints", self.len()) }
}
#[derive(Debug)]
struct Point(i32, i32);
impl Describe for Point {
    fn describe(&self) -> String { format!("point {:?}", self) }
}
fn show<T: Describe>(x: T) -> String { x.describe() }
fn show_all(items: &[Box<dyn Describe>]) -> Vec<String> { items.iter().map(|d| d.describe()).collect() }
fn main() {
    println!("{} | {} | {} | {} | {}", show(2.5f64), show(7i32), show("hi".to_string()), show(vec![1, 2, 3]), show(Point(1, 2)));
    let boxed: Vec<Box<dyn Describe>> = vec![Box::new(Point(3, 4)), Box::new(Point(0, 0))];
    println!("{:?}", show_all(&boxed));
}
