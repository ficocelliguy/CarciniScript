// Two `mod` levels below the crate root, which still sets the limit.
macro_rules! letters {
    (@acc $acc:expr;) => { $acc };
    (@acc $acc:expr; $w:ident $($rest:ident)*) => {
        letters!(@acc $acc * 2 % 1000 + stringify!($w).len(); $($rest)*)
    };
    ($($w:ident)*) => { letters!(@acc 0usize; $($w)*) };
}

pub fn words() -> usize {
    letters!(
        a bb ccc a bb ccc a bb ccc a bb ccc a bb ccc a bb ccc a bb ccc a bb ccc a bb ccc a bb
        a bb ccc a bb ccc a bb ccc a bb ccc a bb ccc a bb ccc a bb ccc a bb ccc a bb ccc a bb
        a bb ccc a bb ccc a bb ccc a bb ccc a bb ccc a bb ccc a bb ccc a bb ccc a bb ccc a bb
        a bb ccc a bb ccc a bb ccc a bb ccc a bb ccc a bb ccc a bb ccc a bb ccc a bb ccc a bb
        a bb ccc a bb ccc a bb ccc a bb ccc a bb ccc a bb ccc a bb ccc a bb ccc a bb ccc a bb
    )
}
