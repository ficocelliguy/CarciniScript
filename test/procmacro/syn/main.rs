use mac::{Builder, Describe, VariantNames, timed};

#[derive(Debug, Builder)]
struct Config {
    host: String,
    port: u16,
}

#[derive(VariantNames)]
enum Shape {
    Dot,
    Circle(f64),
    Rect { w: f64, h: f64 },
}

#[derive(Describe)]
#[allow(dead_code)]
struct Pair<T: Clone, U>
where
    U: Default,
{
    a: T,
    b: U,
}

/// Adds.
#[timed("calc")]
fn sum(a: i32, b: i32) -> i32 {
    a + b
}

#[timed("noop")]
pub fn hello() {
    println!("hello");
}

fn main() {
    let c = Config::builder().host("home".to_string()).port(8080).build().unwrap();
    println!("{:?}", c);
    println!("{:?}", Config::builder().port(1).build().map(|c| c.port));
    let shapes = [Shape::Dot, Shape::Circle(1.0), Shape::Rect { w: 1.0, h: 2.0 }];
    for s in &shapes {
        print!("{} ", s.variant_name());
    }
    println!("{}", Shape::variant_count());
    println!("{}", Pair::<i32, u8>::describe());
    println!("{}", sum(2, 3));
    hello();
}
