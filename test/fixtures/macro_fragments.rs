// edition: 2018
// (2018: `|$it:pat|` below, which 2021's `pat` can't be followed by.)
// `macro_rules!` fragments as rustc parses them: one `expr` / `ty` / `pat` per capture, joined
// punctuation as one `tt`, and expansions that have to stay valid Rust once written out.
#![allow(semicolon_in_expressions_from_macros)]

// `$($n:expr)*`: expressions with no separator, each one as long as it can be.
macro_rules! sum_each {
    ($($n:expr)*) => {{
        let mut v = Vec::new();
        $( v.push($n); )*
        v
    }};
}

// A qualified path's commas (`<Vec<(u8, u8)>>::new()`) belong to the expression.
macro_rules! all_empty {
    ($($e:expr),* $(,)?) => {
        [$($e.is_empty()),*]
    };
}

// A type before `<` / `<<` stays a type (`as u32 << 4` isn't `u32<..>`).
macro_rules! shifts {
    ($t:ty, $bits:expr) => {
        fn shifted(x: u8) -> $t {
            x as $t << $bits
        }
        fn less(x: u8, y: $t) -> bool {
            x as $t < y
        }
    };
}
shifts!(u32, 4);

// A block `expr` in statement position, followed by more statements.
macro_rules! probe_loop {
    ($label:tt: $i:ident < $len:expr, $body:expr) => {
        $label: loop {
            if $i < $len {
                $body
                $i += 1;
            } else {
                break;
            }
        }
    };
}

// In expression position, the expansion's trailing `;` is dropped.
macro_rules! bail {
    ($reason:expr, $cause:expr) => {
        return Err(format!("{}: {}", $reason, $cause));
    };
}

fn first_even(v: &[i32]) -> Result<i32, String> {
    let n = match v.iter().find(|x| *x % 2 == 0) {
        Some(n) => *n,
        None => bail!("no even number", v.len()),
    };
    Ok(n)
}

// A macro named `safe` (a contextual keyword) at item level.
macro_rules! safe {
    ($($t:ident => $w:ident);+ $(;)?) => { $(
        #[derive(Debug, Default)]
        struct $w {
            inner: $t,
        }
        impl $w {
            fn get(&self) -> $t {
                self.inner
            }
        }
    )+ };
}
safe! {
    u8 => SafeU8;
    u16 => SafeU16;
}

// `=>` and `==` are one `tt` each.
macro_rules! table {
    ($($k:ident $op:tt $v:expr,)*) => {{
        let mut out = Vec::new();
        $( out.push(format!("{} {} {}", stringify!($k), stringify!($op), $v)); )*
        out
    }};
}

// A `$` with nothing after it is the token `$`.
macro_rules! token_name {
    [$] => { "dollar" };
    [#] => { "pound" };
}

// A string literal munched one token at a time stays a string, not `"x"as`.
macro_rules! snapshot {
    ($($args:tt)*) => {
        snapshot_impl!(() $($args)*)
    };
}
macro_rules! snapshot_impl {
    (($($expr:tt)*) as $t:ty, @$expected:literal) => {{
        let parsed: $t = $($expr)*.parse().unwrap();
        (parsed, $expected)
    }};
    (($($expr:tt)*) $next:tt $($rest:tt)*) => {
        snapshot_impl!(($($expr)* $next) $($rest)*)
    };
}

// A rule marker (`@closure`) isn't renamed with the `let closure` in the same expansion.
macro_rules! closures {
    (@closure $x:expr) => {
        $x * 10
    };
    ($x:expr) => {{
        let closure = |y: i32| y + 1;
        closures!(@closure closure($x))
    }};
}

// `pat` and `pat_param` fragments.
macro_rules! is_any {
    ($x:expr, $($p:pat_param)|+) => {
        match $x {
            $($p)|+ => true,
            _ => false,
        }
    };
}

macro_rules! first_pat {
    ($x:expr, $p:pat => $e:expr) => {
        match $x {
            $p => $e,
            #[allow(unreachable_patterns)]
            _ => -1,
        }
    };
}

// A `pat` followed by `|` in the rule (itertools' `|$it:pat| $closure:expr`).
macro_rules! apply {
    ($src:expr, |$it:pat| $body:expr) => {{
        let $it = $src;
        $body
    }};
}

// `//!` docs in an inline module that follows other items.
pub mod read {
    //! Reader helpers.
    //!
    //! More about them.
    pub fn one() -> i32 {
        1
    }
}

fn main() {
    let w = [1, 2, 3];
    // (`2 * 4 -1` is one expression, as rustc parses it.)
    println!("{:?}", sum_each!(0 1 2 "seven".len() 2 * 4 -1 w.len() w[1]));
    println!(
        "{:?}",
        all_empty!(
            <Vec<(u8, u8)>>::new(),
            <Vec<(u8, u8)>>::from([(1, 2)]),
        )
    );
    println!("{} {} {}", shifted(3), less(2, 3), less(5, 3));

    let v = vec![1, 2, 3, 40, 50];
    let mut i = 0;
    let mut total = 0;
    probe_loop!('probe: i < v.len(), {
        total += v[i];
        if total > 10 {
            break 'probe;
        }
    });
    println!("{} {}", i, total);

    println!("{:?} {:?}", first_even(&[1, 3, 4]), first_even(&[1, 3]));
    let a = SafeU8 { inner: 7 };
    println!("{} {} {:?}", a.get(), SafeU16::default().get(), a);

    for line in table!(a => 1, b == 2, c <= 3,) {
        println!("{}", line);
    }
    println!("{} {}", token_name![$], token_name![#]);
    println!("{:?}", snapshot!("42" as i32, @"answer"));
    println!("{:?}", snapshot!("2.5" as f64, @r#"half of "5""#));
    println!("{}", closures!(4));
    println!("{} {}", is_any!(3, 1 | 3 | 5), is_any!(4, 1 | 3 | 5));
    println!(
        "{} {}",
        first_pat!(Some(2), Some(n @ 1..=5) => n * 3),
        first_pat!(None::<i32>, Some(n) => n)
    );
    println!("{} {}", apply!(5, |x| x * x), apply!((1, 2), |(a, b)| a + b));
    println!("{}", read::one());
}
