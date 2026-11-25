// A trait's methods need the trait in scope, which these all have: its own module, `use`,
// `use .. as _`, `*`, a block's `use`, a type parameter's bound, `dyn`, and its own impl.
mod money {
    pub trait Money {
        fn money(&self) -> String;
        fn twice(&self) -> String {
            format!("{} {}", self.money(), self.money())
        }
    }

    impl Money for f64 {
        fn money(&self) -> String {
            format!("${:.2}", self)
        }
    }

    pub fn here(x: f64) -> String {
        x.money()
    }

    pub mod nested {
        use super::Money;

        pub fn there(x: f64) -> String {
            x.twice()
        }
    }
}

mod shapes {
    pub trait Area {
        fn area(&self) -> f64;
    }

    pub struct Square(pub f64);

    impl Area for Square {
        fn area(&self) -> f64 {
            self.0 * self.0
        }
    }

    pub struct Rect {
        pub w: f64,
        pub h: f64,
    }

    impl Area for Rect {
        fn area(&self) -> f64 {
            self.w * self.h
        }
    }

    impl Rect {
        // An inherent method of the same name as nothing in scope: fine anywhere.
        pub fn half(&self) -> f64 {
            self.w * self.h / 2.0
        }
    }
}

mod anon {
    use crate::money::Money as _;

    pub fn show(x: f64) -> String {
        x.money()
    }
}

mod glob {
    use crate::shapes::*;

    pub fn total(a: &Square, b: &Rect) -> f64 {
        a.area() + b.area()
    }
}

trait Shout {
    fn shout(&self) -> String;
}

impl Shout for str {
    fn shout(&self) -> String {
        self.to_uppercase()
    }
}

fn bounded<T: shapes::Area>(t: &T) -> f64 {
    t.area()
}

fn dynamic(t: &dyn shapes::Area) -> f64 {
    t.area()
}

fn main() {
    println!("{}", money::here(1.5));
    println!("{}", money::nested::there(2.0));
    println!("{}", anon::show(3.25));
    let sq = shapes::Square(2.0);
    let r = shapes::Rect { w: 2.0, h: 3.0 };
    println!("{}", glob::total(&sq, &r));
    println!("{} {}", bounded(&sq), dynamic(&r));
    println!("{}", r.half());
    {
        use money::Money;
        println!("{}", 4.0f64.money());
    }
    println!("{}", "hi".shout());
}
