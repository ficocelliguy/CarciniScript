pub mod nested;

macro_rules! count {
    () => { 0usize };
    ($head:tt $($tail:tt)*) => { 1usize + count!($($tail)*) };
}

pub fn total() -> usize {
    count!(
        a b c d e f g h i j a b c d e f g h i j a b c d e f g h i j a b c d e f g h i j
        a b c d e f g h i j a b c d e f g h i j a b c d e f g h i j a b c d e f g h i j
        a b c d e f g h i j a b c d e f g h i j a b c d e f g h i j a b c d e f g h i j
        a b c d e f g h i j a b c d e f g h i j
    )
}
