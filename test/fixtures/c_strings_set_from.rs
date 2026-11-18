// C string literals (`c"..."`, `cr"..."`) as `CStr` values, itertools' `set_from` over
// `iter_mut()`, `iter_mut()` chains over numbers, strings and bools, and `char::escape_debug`
// on combining marks and separators.
use itertools::Itertools;
use std::ffi::CStr;
use std::iter::{self, repeat};

const FILE_PATH: &CStr = c"/dev/urandom";

#[derive(Debug, Clone, PartialEq)]
struct P {
    x: i32,
}

fn describe(s: &CStr) -> String {
    format!("{:?} {} {}", s, s.count_bytes(), s.is_empty())
}

fn negate(r: &mut [u64], a: &[u64]) {
    r.iter_mut().zip(a.iter()).for_each(|(r, &a)| {
        *r = !a;
    });
}

fn fill(result: &mut [u32], input: Vec<u32>) {
    result
        .iter_mut()
        .zip(input.into_iter().chain(iter::repeat(0)))
        .for_each(|(r, i)| *r = i);
}

fn halve(v: &mut [i64]) -> Result<(), String> {
    v.iter_mut().try_for_each(|item| {
        if *item < 0 {
            return Err(format!("negative {}", item));
        }
        *item /= 2;
        Ok(())
    })
}

fn main() {
    // C strings
    println!("{}", describe(FILE_PATH));
    println!("{:?} {}", FILE_PATH.to_bytes(), FILE_PATH.to_bytes_with_nul().len());
    println!("{}", FILE_PATH.to_str().unwrap());
    let raw = cr"a\nb";
    let esc = c"tab\there \"q\" 'x' \u{e9} \x7f";
    println!("{:?} {:?}", raw, esc);
    println!("{}", esc.to_string_lossy());
    println!("{}", c"" == c"" && c"a" < c"b");
    println!("{:?}", CStr::from_bytes_with_nul(b"hi\0"));
    println!("{:?}", CStr::from_bytes_with_nul(b"h\0i\0"));
    println!("{:?}", CStr::from_bytes_with_nul(b"hi"));
    println!("{:?}", CStr::from_bytes_until_nul(b"ab\0cd"));
    println!("{:?}", CStr::from_bytes_until_nul(b"abcd"));
    if let Err(e) = CStr::from_bytes_with_nul(b"x\0y") {
        println!("{}", e);
    }
    let odd = CStr::from_bytes_until_nul(&[0xff, b'a', 0xe2, 0x82, b'z', 0xc3, 0xa9, 0]).unwrap();
    println!("{:?} {}", odd, odd.to_str().is_err());
    println!("{:?}", c"e\u{301} \u{1F600}");
    for n in [c"one", c"two"].iter() {
        print!("{:?} ", n);
    }
    println!();

    // `char::escape_debug`
    for c in ['\u{301}', '\u{a0}', '\u{200d}', '\u{2028}', ' ', 'é', '\u{7f}', '\u{ad}'] {
        print!("{} ", c.escape_debug());
    }
    println!();

    // `set_from`
    let mut plot = vec!['.'; 6];
    let n = plot.iter_mut().set_from(repeat(' '));
    println!("{:?} {}", plot, n);
    let n = plot.iter_mut().skip(2).set_from("ab".chars());
    println!("{:?} {}", plot, n);
    let mut v = vec![1u64, 2, 3];
    let n = v.iter_mut().rev().set_from(vec![7, 8, 9, 10]);
    println!("{:?} {}", v, n);
    let mut ps = vec![P { x: 1 }, P { x: 2 }];
    let n = ps.iter_mut().set_from([P { x: 5 }]);
    println!("{:?} {}", ps, n);
    let mut words = vec![String::new(); 3];
    words.iter_mut().set_from(["a", "b"].iter().map(|s| s.to_string()));
    println!("{:?}", words);

    // `iter_mut()` chains over numbers, strings and bools
    let mut r = [0u64; 3];
    negate(&mut r, &[1, 2, 3]);
    println!("{:?}", r);
    let mut res = [9u32; 5];
    fill(&mut res, vec![1, 2]);
    println!("{:?}", res);
    let mut w = vec![1, 2, 3, 4];
    if let Some(x) = w.iter_mut().find(|x| **x > 2) {
        *x += 100;
    }
    println!("{:?}", w);
    let mut h = vec![4, 8, 10];
    println!("{:?} {:?}", halve(&mut h), h);
    let mut h2 = vec![4, -8, 10];
    println!("{:?} {:?}", halve(&mut h2), h2);
    let mut names = vec!["a".to_string(), "b".to_string()];
    names.iter_mut().zip(1..).for_each(|(n, i)| n.push_str(&i.to_string()));
    println!("{:?}", names);
    let mut flags = vec![false; 3];
    flags.iter_mut().zip([true, false, true]).for_each(|(f, x)| *f = x);
    println!("{:?}", flags);
}
