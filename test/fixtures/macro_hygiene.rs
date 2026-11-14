// `macro_rules!` hygiene: names a macro's body binds (`let tmp`, `let total: i32`, `for i`, `|x|`) don't
// clash with the caller's `tmp`, `total`, `i` or `x`; a name passed in (`$name:ident`) is the caller's.
macro_rules! swap {
    ($a:expr, $b:expr) => {{
        let tmp = $a;
        $a = $b;
        $b = tmp;
    }};
}

macro_rules! double_sum {
    ($v:expr) => {{
        let total: i32 = $v.iter().map(|x| x * 2).sum();
        total
    }};
}

macro_rules! count_to {
    ($n:expr) => {{
        let mut acc = Vec::new();
        for i in 0..$n {
            acc.push(i);
        }
        acc
    }};
}

macro_rules! make_var {
    ($name:ident, $val:expr) => {
        let $name = $val;
    };
}

struct P {
    tmp: i32,
}

fn main() {
    let mut tmp = 1;
    let mut other = 2;
    swap!(tmp, other);
    let total = 100;
    let x = 5;
    let d = double_sum!(vec![x, total]);
    let i = 42;
    let c = count_to!(3);
    make_var!(made, i + 1);
    let p = P { tmp: 9 };
    println!("{} {} {} {} {} {:?} {} {}", tmp, other, total, d, i, c, made, p.tmp);
}
