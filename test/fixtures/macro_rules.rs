// `macro_rules!`: expression macros (captures are one expression: `square!(1 + 2)` is 9), several
// rules picked by their tokens, `$(..),*` repetitions with a trailing-comma rule, nested
// repetition in the expansion, recursion, `ident` captures that name functions and variables,
// macros expanding to statements and to items, `tt` and `literal` fragments, and a macro used
// inside another macro's arguments.
macro_rules! square {
    ($x:expr) => {
        $x * $x
    };
}

macro_rules! max_of {
    ($x:expr) => { $x };
    ($x:expr, $($rest:expr),+) => {{
        let a = $x;
        let b = max_of!($($rest),+);
        if a > b { a } else { b }
    }};
}

macro_rules! hashmap {
    ($($k:expr => $v:expr),* $(,)?) => {{
        let mut m = std::collections::BTreeMap::new();
        $( m.insert($k, $v); )*
        m
    }};
}

macro_rules! getter {
    ($name:ident, $field:ident, $t:ty) => {
        fn $name(s: &Server) -> $t {
            s.$field.clone()
        }
    };
}

macro_rules! log {
    (warn $msg:literal) => { println!("[warn] {}", $msg) };
    (info $($arg:tt)*) => { println!("[info] {}", format!($($arg)*)) };
}

macro_rules! swap_vars {
    ($a:ident, $b:ident) => {
        let tmp = $a;
        $a = $b;
        $b = tmp;
    };
}

#[derive(Debug, Clone)]
struct Server {
    name: String,
    ram: u32,
}

getter!(server_name, name, String);
getter!(server_ram, ram, u32);

fn main() {
    println!("{} {} {}", square!(7), square!(1 + 2), square!(2.5));
    println!("{} {}", max_of!(3), max_of!(4, 9, 2, square!(3)));
    let m = hashmap! { "b" => 2, "a" => 1, "c" => 3, };
    println!("{:?}", m);
    let empty: std::collections::BTreeMap<&str, i32> = hashmap!();
    println!("{:?}", empty);
    let s = Server { name: "n00dles".into(), ram: 4 };
    println!("{} {}", server_name(&s), server_ram(&s));
    log!(warn "low ram");
    log!(info "{} has {}GB", s.name, s.ram);
    let mut x = 1;
    let mut y = 2;
    swap_vars!(x, y);
    println!("{x} {y}");
}
