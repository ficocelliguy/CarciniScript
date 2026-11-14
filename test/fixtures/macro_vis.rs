// `macro_rules!` with `$v:vis` (also empty) and `$(#[$m:meta])*` fragments: macros that stamp out
// functions and structs with their visibility and attributes.
macro_rules! getter {
    ($v:vis $name:ident : $t:ty = $val:expr) => {
        $v fn $name() -> $t {
            $val
        }
    };
}

macro_rules! tagged {
    ($(#[$m:meta])* $v:vis struct $n:ident { $($f:ident : $ft:ty),* }) => {
        $(#[$m])*
        $v struct $n { $($f: $ft),* }
    };
}

getter!(pub answer: i32 = 42);
getter!(greeting: &'static str = "hi");

tagged! {
    #[derive(Debug, Clone)]
    pub struct Point { x: i32, y: i32 }
}

fn main() {
    let p = Point { x: answer(), y: 1 };
    println!("{} {:?} {:?}", greeting(), p, p.clone().x);
}
