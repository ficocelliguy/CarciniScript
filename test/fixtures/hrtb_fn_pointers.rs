// `for<'a> fn(..)` pointer types (a higher-ranked lifetime binder on a function pointer) in struct
// fields, `Option` fields, parameters, return types, aliases and derived structs.
type Pick = for<'a> fn(&'a str, &'a str) -> &'a str;

#[derive(Clone, Copy, Debug)]
struct Ops {
    split: for<'a> fn(&'a [u8], usize) -> (&'a [u8], &'a [u8]),
    pick: Pick,
    hook: Option<for<'a> fn(&'a mut Vec<u8>)>,
}

fn halve<'a>(s: &'a [u8], n: usize) -> (&'a [u8], &'a [u8]) {
    s.split_at(n)
}

fn longer<'a>(a: &'a str, b: &'a str) -> &'a str {
    if a.len() >= b.len() { a } else { b }
}

fn push_nine(v: &mut Vec<u8>) {
    v.push(9);
}

fn apply(f: for<'a> fn(&'a str) -> &'a str, s: &str) -> String {
    f(s).to_string()
}

fn trimmer() -> for<'a> fn(&'a str) -> &'a str {
    str::trim
}

fn main() {
    let ops = Ops { split: halve, pick: longer, hook: Some(push_nine) };
    let copy = ops;
    let data = [1u8, 2, 3, 4];
    let (a, b) = (copy.split)(&data, 1);
    println!("{:?} {:?}", a, b);
    println!("{}", (ops.pick)("ab", "abc"));
    let mut v = vec![1u8];
    if let Some(h) = ops.hook {
        h(&mut v);
    }
    println!("{:?}", v);
    println!("[{}]", apply(trimmer(), "  hi  "));
    let fs: Vec<for<'a> fn(&'a str) -> &'a str> = vec![str::trim_start, str::trim_end];
    for f in &fs {
        println!("[{}]", f(" x "));
    }
}
