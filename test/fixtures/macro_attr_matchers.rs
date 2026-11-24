// A `macro_rules!` whose matcher takes attributes itself: literal `#[cfg(..)]` tokens with a
// fragment inside (itoa's `impl_Integer_size!`), `$(#[$attr:meta])*` (serde's `forwarded_impl!`)
// and `$(, #[$m:meta])?` (writeable). The invocation keeps its attributes for the matcher; the
// expansion's `#[cfg]`s apply where they land.
trait Width { fn width() -> String; }
macro_rules! impl_size {
    ($t:ty as $p:ident #[cfg(target_pointer_width = $w:literal)]) => {
        #[cfg(target_pointer_width = $w)]
        impl Width for $t {
            fn width() -> String { format!("{} {}", stringify!($p), $w) }
        }
    };
}
impl_size!(isize as i32 #[cfg(target_pointer_width = "32")]);
impl_size!(isize as i64 #[cfg(target_pointer_width = "64")]);
macro_rules! forwarded {
    ($(#[$attr:meta])* $name:ident, $v:expr) => {
        $(#[$attr])*
        fn $name() -> i32 { $v }
    };
}
forwarded! { #[cfg(windows)] pick, 1 }
forwarded! { #[cfg(not(windows))] pick, 1 }
forwarded! { #[inline] #[allow(dead_code)] other, 2 }
macro_rules! with_opt {
    ($name:ident $(, #[$m:meta])?) => {
        $(#[$m])?
        fn $name() -> &'static str { stringify!($name) }
    };
}
with_opt!(plain);
with_opt!(gated, #[cfg(not(feature = "nope"))]);
fn main() {
    println!("{} {} {}", isize::width(), pick(), other());
    println!("{} {}", plain(), gated());
}
