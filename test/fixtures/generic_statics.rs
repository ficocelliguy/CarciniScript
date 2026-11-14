// Static trait functions and consts on a generic `T` (`T::unit()`, `T::SIDES`, `T::default()`, also for
// `i32` and `String`), passed on to another generic function, and a default trait function without
// `self` that reads `Self::SIDES`.
trait Shape {
    const SIDES: u32;
    fn unit() -> Self;
    fn area(&self) -> f64;
    fn name() -> String {
        format!("{}-gon", Self::SIDES)
    }
}

#[derive(Debug, Default, Clone)]
struct Square(f64);

#[derive(Debug, Default, Clone)]
struct Tri(f64);

impl Shape for Square {
    const SIDES: u32 = 4;
    fn unit() -> Self {
        Square(1.0)
    }
    fn area(&self) -> f64 {
        self.0 * self.0
    }
}

impl Shape for Tri {
    const SIDES: u32 = 3;
    fn unit() -> Self {
        Tri(1.0)
    }
    fn area(&self) -> f64 {
        self.0 * self.0 * 0.433
    }
}

fn units<T: Shape>(n: usize) -> Vec<T> {
    (0..n).map(|_| T::unit()).collect()
}

fn describe<T: Shape>() -> String {
    format!("{} with {} sides, unit area {:.3}", T::name(), T::SIDES, T::unit().area())
}

fn fresh<T: Default + std::fmt::Debug>() -> String {
    format!("{:?}", T::default())
}

fn twice<T: Shape>() -> (String, usize) {
    (describe::<T>(), units::<T>(2).len())
}

fn main() {
    let sq: Vec<Square> = units(3);
    println!("{:?}", sq);
    println!("{}", describe::<Tri>());
    println!("{} {} {}", fresh::<Square>(), fresh::<i32>(), fresh::<String>());
    println!("{:?}", twice::<Square>());
}
