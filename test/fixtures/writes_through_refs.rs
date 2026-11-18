// Writes through references into containers and cells: in-place methods on a range of a `Vec`
// (`buf[a..b].copy_from_slice(..)`, `v[1..5].sort()`, `.fill`, `.rotate_left`, `.swap`),
// `v.get(1..3)` / `dest.get_mut(n..)` re-slicing, `*output = vec![..]` through a `&mut Vec`
// parameter, and closures taking `&mut` to a number or string (`F: FnMut(&mut String)`,
// `|f| write!(f, "-")`, `|v: &mut i32|`, a function item as one, boxed `dyn Fn(&mut i64)`,
// `F: FnMut(&mut T)` over structs and numbers).
use std::fmt::{self, Write};

#[derive(Debug)]
struct Full;

fn fill_header(buf: &mut [u8], tag: &[u8]) {
    buf[2..2 + tag.len()].copy_from_slice(tag);
    buf[..2].copy_from_slice(&[0xAA, 0xBB]);
}

fn put(mut dest: &mut [u8], chunks: &[&[u8]]) -> Result<usize, Full> {
    let start = dest.len();
    for c in chunks {
        if c.len() > dest.len() {
            return Err(Full);
        }
        dest[..c.len()].copy_from_slice(c);
        dest = dest.get_mut(c.len()..).ok_or(Full)?;
    }
    Ok(start - dest.len())
}

fn put_indexed(mut dest: &mut [u8], chunks: &[&[u8]]) -> usize {
    let start = dest.len();
    for c in chunks {
        dest[..c.len()].copy_from_slice(c);
        dest = &mut dest[c.len()..];
    }
    start - dest.len()
}

fn split_fill(buf: &mut [u8]) {
    let (head, tail) = buf.split_at_mut(2);
    head[0] = 1;
    head[1] = 2;
    tail.fill(9);
}

fn replace_all(output: &mut Vec<i32>) {
    *output = vec![7, 7];
}

fn first_mut(v: &mut [i32]) {
    if let Some(f) = v.first_mut() {
        *f += 100;
    }
    let last = v.last_mut().unwrap();
    *last = -1;
}

fn join_with<F>(items: &[i32], mut sep: F) -> String
where
    F: FnMut(&mut String) -> fmt::Result,
{
    let mut out = String::new();
    for (i, x) in items.iter().enumerate() {
        if i > 0 {
            sep(&mut out).unwrap();
        }
        write!(out, "{x}").unwrap();
    }
    out
}

fn inc(x: &mut i32) {
    *x += 1;
}

fn apply<F: FnMut(&mut i32)>(x: &mut i32, mut f: F) {
    f(x);
    f(x);
}

fn with_buf<F: Fn(&mut String)>(f: F) -> String {
    let mut s = String::from("[");
    f(&mut s);
    s.push(']');
    s
}

struct Hooks {
    on_line: Box<dyn Fn(&mut String)>,
    steps: Vec<Box<dyn Fn(&mut i64)>>,
}

#[derive(Debug, Default)]
struct Acc {
    n: i32,
}

fn each<T, F: FnMut(&mut T)>(items: &mut [T], mut f: F) {
    for it in items.iter_mut() {
        f(it);
    }
}

fn twice<T>(v: &mut T, f: impl Fn(&mut T)) {
    f(v);
    f(v);
}

fn main() {
    let mut buf = vec![0u8; 8];
    fill_header(&mut buf, b"xyz");
    println!("{:?}", buf);
    let mut a = [1, 2, 3, 4, 5, 6];
    a[1..4].reverse();
    println!("{:?}", a);
    let mut v = vec![5, 3, 9, 1, 7, 2];
    v[1..5].sort();
    v[..3].fill(0);
    v[2..].rotate_left(1);
    v[1..4].swap(0, 2);
    println!("{:?}", v);
    let mut s = vec!["d".to_string(), "b".into(), "c".into(), "a".into()];
    s[1..].sort();
    let mut flags = vec![false; 5];
    flags[1..3].fill(true);
    println!("{:?} {:?}", s, flags);

    let nums = vec![1, 2, 3, 4];
    println!("{:?} {:?} {:?}", nums.get(1..3), nums.get(..2), nums.get(3..9));
    let mut w = vec![1, 2, 3, 4, 5];
    if let Some(part) = w.get_mut(1..3) {
        part[0] = 20;
        part.reverse();
    }
    if let Some(tail) = w.get_mut(3..) {
        tail.fill(0);
    }
    let none = w.get_mut(9..).is_none();
    println!("{:?} {}", w, none);
    let mut out = [0u8; 8];
    println!("{:?} {:?}", put(&mut out, &[b"ab", b"cde"]), out);
    println!("{:?}", put(&mut out[..3], &[b"ab", b"cde"]).is_err());
    let mut out2 = [0u8; 6];
    println!("{} {:?}", put_indexed(&mut out2, &[b"x", b"yz"]), out2);
    let mut b2 = vec![0u8; 5];
    split_fill(&mut b2);
    println!("{:?}", b2);

    let mut o = vec![0, 0, 0];
    replace_all(&mut o);
    println!("{:?}", o);
    let mut m = vec![1, 2, 3];
    first_mut(&mut m);
    first_mut(&mut m[1..]);
    println!("{:?}", m);

    println!("{}", join_with(&[1, 2, 3], |f| write!(f, "-")));
    println!("{}", join_with(&[4, 5], |f| f.write_str(", ")));
    let mut n = 0;
    apply(&mut n, inc);
    apply(&mut n, |v| *v += 10);
    let g = inc;
    g(&mut n);
    let bump = |v: &mut i32| *v *= 3;
    bump(&mut n);
    println!("{n}");
    println!("{} {}", with_buf(|s| s.push_str("ab")), with_buf(|s| write!(s, "{}", 42).unwrap()));
    let h = Hooks {
        on_line: Box::new(|s| s.push_str(" <eol>")),
        steps: vec![Box::new(|x| *x *= 2), Box::new(|x| *x += 3)],
    };
    let mut line = String::from("text");
    (h.on_line)(&mut line);
    let mut k = 5i64;
    for step in &h.steps {
        step(&mut k);
    }
    println!("{line} {k}");
    let mut accs = vec![Acc::default(), Acc { n: 4 }];
    each(&mut accs, |a| a.n += 10);
    let mut sevens = vec![1, 2, 3];
    each(&mut sevens, |x| *x *= 7);
    println!("{:?} {:?}", accs, sevens);
    let mut text = String::new();
    twice(&mut text, |s| write!(s, "ab").unwrap());
    let mut small = 1u8;
    twice(&mut small, |k| *k += 1);
    let mut grow = |t: &mut String| t.push('y');
    grow(&mut text);
    println!("{text} {small}");
}
