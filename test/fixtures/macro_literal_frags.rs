// A literal `$x:expr` in a transcriber attribute (serde's `target_has_atomic = $size`),
// `concat!(stringify!($p), ..)` (itoa) and macro-generated inline modules.
macro_rules! sized {
    ($name:ident $size:expr) => {
        #[cfg(any(no_target_has_atomic, target_has_atomic = $size))]
        fn $name() -> &'static str { $size }
    };
}
sized!(eight "8");
macro_rules! named {
    ($p:ident, $w:expr) => {
        const NAME: &str = concat!(stringify!($p), " ", $w);
    };
}
named!(foo, "bar");
macro_rules! modded {
    ($m:ident, $t:ident, $v:expr) => {
        mod $m {
            pub struct $t(pub i32);
            impl $t { pub fn get(&self) -> i32 { self.0 + $v } }
            pub fn make() -> $t { $t($v) }
        }
    };
}
modded!(a, A, 1);
modded!(b, B, 2);
fn main() {
    println!("{}", eight());
    println!("{}", NAME);
    println!("{} {}", a::make().get(), b::B(5).get());
}
