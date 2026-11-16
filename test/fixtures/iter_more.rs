// More iterator adapters: peekable (peek, next_if, next_if_eq), fuse, by_ref, try_fold,
// try_for_each, next_back, nth_back, rfind, rfold, len, comparisons and is_sorted.
use std::iter::Peekable;
use std::str::Chars;

#[derive(Debug, PartialEq)]
enum Tok {
    Num(i64),
    Ident(String),
    Op(char),
}

fn number(chars: &mut Peekable<Chars>) -> i64 {
    let mut n = 0;
    while let Some(&c) = chars.peek() {
        if let Some(d) = c.to_digit(10) {
            n = n * 10 + d as i64;
            chars.next();
        } else {
            break;
        }
    }
    n
}

fn lex(src: &str) -> Vec<Tok> {
    let mut out = Vec::new();
    let mut chars = src.chars().peekable();
    while let Some(&c) = chars.peek() {
        if c.is_whitespace() {
            chars.next();
        } else if c.is_ascii_digit() {
            out.push(Tok::Num(number(&mut chars)));
        } else if c.is_alphabetic() {
            let mut s = String::new();
            while let Some(c) = chars.next_if(|c| c.is_alphanumeric()) {
                s.push(c);
            }
            out.push(Tok::Ident(s));
        } else {
            chars.next();
            out.push(Tok::Op(c));
        }
    }
    out
}

fn main() {
    println!("{:?}", lex("x1 = 42 + y*7"));

    let mut it = [1, 1, 2, 3, 3, 3].iter().peekable();
    let mut runs = Vec::new();
    while let Some(&x) = it.next() {
        let mut n = 1;
        while it.next_if_eq(&&x).is_some() {
            n += 1;
        }
        runs.push((x, n));
    }
    println!("{:?}", runs);
    let mut p = (1..4).peekable();
    let a = p.peek().copied();
    let b = p.next();
    let c = p.peek().copied();
    println!("{:?} {:?} {:?} {:?}", a, b, c, p.collect::<Vec<_>>());

    // by_ref: take a few, then keep going with the rest
    let mut words = "a b c d e".split_whitespace();
    let first: Vec<&str> = words.by_ref().take(2).collect();
    let rest: Vec<&str> = words.collect();
    println!("{:?} {:?}", first, rest);
    let mut f = vec![1, 2].into_iter().fuse();
    println!(
        "{:?} {:?} {:?} {:?}",
        f.next(),
        f.next(),
        f.next(),
        f.next()
    );

    // try_fold / try_for_each
    let sum: Option<u8> = [100u8, 100, 50]
        .iter()
        .try_fold(0u8, |acc, &x| acc.checked_add(x));
    let sum2: Option<u8> = [100u8, 100, 60]
        .iter()
        .try_fold(0u8, |acc, &x| acc.checked_add(x));
    println!("{:?} {:?}", sum, sum2);
    let parsed: Result<i32, String> = ["1", "2", "3"].iter().try_fold(0, |acc, s| {
        s.parse::<i32>().map(|v| acc + v).map_err(|e| e.to_string())
    });
    let bad: Result<i32, String> = ["1", "x", "3"].iter().try_fold(0, |acc, s| {
        s.parse::<i32>()
            .map(|v| acc + v)
            .map_err(|e| format!("{}: {}", s, e))
    });
    println!("{:?} {:?}", parsed, bad);
    let r = (1..10).try_for_each(|x| if x < 5 { Ok(()) } else { Err(x) });
    println!("{:?}", r);

    // double-ended
    let mut d = vec![1, 2, 3, 4, 5].into_iter();
    println!(
        "{:?} {:?} {:?} {:?}",
        d.next_back(),
        d.next(),
        d.nth_back(1),
        d.collect::<Vec<_>>()
    );
    let mut two = "racecar".chars();
    let mut pal = true;
    while let (Some(a), Some(b)) = (two.next(), two.next_back()) {
        if a != b {
            pal = false;
        }
    }
    println!("{}", pal);
    println!(
        "{:?} {}",
        (1..10).rfind(|x| x % 3 == 0),
        (1..4).rfold(String::new(), |s, x| s + &x.to_string())
    );

    // len, comparisons, is_sorted
    let v = vec![3, 1, 4, 1, 5];
    let mut li = v.iter();
    li.next();
    println!("{} {}", v.iter().len(), li.len());
    println!(
        "{} {}",
        v.iter().eq([3, 1, 4, 1, 5].iter()),
        v.iter().ne(v.iter().rev())
    );
    println!(
        "{:?} {} {}",
        [1, 2].iter().cmp([1, 3].iter()),
        "abc".chars().lt("abd".chars()),
        [2.0].iter().partial_cmp([1.5].iter()) == Some(std::cmp::Ordering::Greater)
    );
    println!(
        "{} {} {}",
        v.iter().is_sorted(),
        [1, 2, 2, 9].iter().is_sorted(),
        ["bb", "a", "ccc"].iter().is_sorted_by_key(|s| s.len())
    );
    println!("{}", [5, 3, 1].iter().is_sorted_by(|a, b| a >= b));
}
