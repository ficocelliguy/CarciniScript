// `macro_rules!` from module files: `#[macro_export]` macros of a `mod` file, and every macro of
// one loaded with `#[macro_use]`.
mod helpers;
#[macro_use]
mod shapes;

fn main() {
    let v = square_all![1, 2, 3];
    println!("{:?} {}", v, helpers::total(&v));
    let r = rect!(2, 5);
    println!("{} {}", r.0 * r.1, maxed!(3, 9, 4));
}
