// Deep macro expansions: a crate's `#![recursion_limit]` raises rustc's 128 levels, and a
// tt-muncher's `@internal` rules aren't taken by an earlier `$e:expr` rule (no expression starts
// with `@`), which used to recurse until the limit.
#![recursion_limit = "256"]

use std::fmt::Write;

// 150 levels: one per token.
macro_rules! count {
    () => { 0usize };
    ($head:tt $($tail:tt)*) => { 1usize + count!($($tail)*) };
}

// writeable's `assert_writeable_eq!`: each rule hands on to the next by its own path.
#[macro_export]
macro_rules! assert_shown_eq {
    ($actual:expr, $expected:expr $(,)?) => {
        $crate::assert_shown_eq!($actual, $expected, "")
    };
    ($actual:expr, $expected:expr, $($arg:tt)+) => {{
        $crate::assert_shown_eq!(@internal, $actual, $expected, $($arg)*)
    }};
    (@internal, $actual:expr, $expected:expr, $($arg:tt)+) => {{
        let shown = $actual.to_string();
        assert_eq!(shown, $expected, $($arg)*);
        shown.len()
    }};
}

// Rules told apart by a leading punctuation token an expression can't start with.
macro_rules! op {
    ($e:expr) => { format!("value {}", $e) };
    (=> $name:ident) => { format!("arrow {}", stringify!($name)) };
    (, $e:expr) => { format!("comma {}", $e) };
    (+ $a:expr, $b:expr) => { format!("plus {}", $a + $b) };
    (% $t:ty) => { format!("type {}", stringify!($t)) };
}

// A tt-muncher that builds a list in an accumulator.
macro_rules! rev {
    (@acc [$($acc:expr),*]) => { vec![$($acc),*] };
    (@acc [$($acc:expr),*] $head:expr $(, $tail:expr)*) => { rev!(@acc [$head $(, $acc)*] $($tail),*) };
    ($($e:expr),*) => { rev!(@acc [] $($e),*) };
}

fn main() {
    let n = count!(
        a b c d e f g h i j a b c d e f g h i j a b c d e f g h i j a b c d e f g h i j
        a b c d e f g h i j a b c d e f g h i j a b c d e f g h i j a b c d e f g h i j
        a b c d e f g h i j a b c d e f g h i j a b c d e f g h i j a b c d e f g h i j
        a b c d e f g h i j a b c d e f g h i j a b c d e f g h i j
    );
    println!("count {n}");

    let len = assert_shown_eq!(44, "44");
    println!("shown {len}");
    let len = assert_shown_eq!(1.5, "1.5", "with a message {}", 1);
    println!("shown {len}");

    println!("{}", op!(3 * 4));
    println!("{}", op!(=> home));
    println!("{}", op!(, 7));
    println!("{}", op!(+ 2, 5));
    println!("{}", op!(% u64));

    let v: Vec<i32> = rev!(1, 2, 3, 4, 5);
    println!("{v:?}");

    let mut s = String::new();
    write!(s, "{}", count!(x y z)).unwrap();
    println!("{s}");
}
