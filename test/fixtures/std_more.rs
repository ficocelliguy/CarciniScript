// More of std: float bits and math (`to_bits`, `hypot`, `div_euclid`, ..), integer `ilog`/`isqrt`,
// `scan` / `unzip` / `map_while`, `std::iter::{repeat, once, successors, from_fn, ..}`, nested
// `concat`, `BTreeMap::range`, `Weak`, and formatting widths.
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::fmt::Write as _;
use std::rc::{Rc, Weak};
use std::cell::{Cell, RefCell};

fn main() {
    let f = 1.5f64;
    println!("{} {}", (0.5f64).to_bits() >> 52, f64::from_bits(4609434218613702656));
    println!("{} {}", (2.5f32).to_bits(), f32::from_bits(0x40200000));
    println!("{} {}", (-7i32).rem_euclid(3), (-7i32).div_euclid(3));
    println!("{} {}", f.clamp(0.0, 1.0), 5i32.clamp(1, 3));
    println!("{} {}", (3.0f64).hypot(4.0), (1.0f64).atan2(1.0));
    println!("{} {} {}", 40u32.leading_zeros(), 40u32.trailing_zeros(), 40u32.count_ones());
    println!("{} {}", (-5i32).signum(), 90f64.to_radians());
    let s = "hello world foo";
    for (i, c) in s.char_indices().filter(|(_, c)| *c == 'o') {
        print!("{i}{c} ");
    }
    println!();
    let words: Vec<&str> = s.split_whitespace().rev().collect();
    println!("{:?}", words);
    let v = vec![5, 3, 8, 1, 9, 2];
    let pairs: Vec<(i32, i32)> = v.iter().zip(v.iter().skip(1)).map(|(a, b)| (*a, *b)).collect();
    println!("{:?}", pairs);
    for w in v.windows(2) {
        print!("{} ", w[0] + w[1]);
    }
    println!();
    for c in v.chunks(4) {
        print!("{:?} ", c);
    }
    println!();
    let mut sorted = v.clone();
    sorted.sort_by_key(|x| std::cmp::Reverse(*x));
    println!("{:?}", sorted);
    let mut d = vec![1, 1, 2, 2, 3, 1];
    d.dedup();
    println!("{:?}", d);
    let s2 = vec![1, 3, 5, 7];
    println!("{:?} {:?}", s2.binary_search(&5), s2.binary_search(&4));
    let total = v.iter().fold(0, |a, b| a + b);
    let running: Vec<i32> = v.iter().scan(0, |acc, &x| { *acc += x; Some(*acc) }).collect();
    println!("{total} {:?}", running);
    let tw: Vec<_> = v.iter().take_while(|x| **x != 1).collect();
    let sw: Vec<_> = v.iter().skip_while(|x| **x != 1).collect();
    println!("{:?} {:?}", tw, sw);
    let fm: Vec<char> = vec!["ab", "cd"].iter().flat_map(|s| s.chars()).collect();
    println!("{:?}", fm);
    let st: Vec<i32> = (0..20).step_by(5).collect();
    println!("{:?} {}", st, "ab".repeat(3));
    let (even, odd): (Vec<i32>, Vec<i32>) = v.iter().partition(|x| *x % 2 == 0);
    println!("{:?} {:?}", even, odd);
    let (a, b): (Vec<i32>, Vec<char>) = vec![(1, 'a'), (2, 'b')].into_iter().unzip();
    println!("{:?} {:?}", a, b);
    let ch: Vec<i32> = v.iter().chain(s2.iter()).copied().collect();
    println!("{:?}", ch);
    let cy: Vec<i32> = [1, 2].iter().cycle().take(5).cloned().collect();
    println!("{:?}", cy);
    let rp: Vec<i32> = std::iter::repeat(7).take(3).collect();
    let on: Vec<i32> = std::iter::once(1).chain(std::iter::once(2)).collect();
    let sc: Vec<u32> = std::iter::successors(Some(1u32), |x| if *x < 100 { Some(x * 3) } else { None }).collect();
    let mut n = 0;
    let ff: Vec<i32> = std::iter::from_fn(|| { n += 1; if n < 4 { Some(n) } else { None } }).collect();
    println!("{:?} {:?} {:?} {:?}", rp, on, sc, ff);
    println!("{:?} {:?}", v.iter().max_by_key(|x| (*x % 5, **x)), v.iter().min_by(|a, b| a.cmp(b)));
    println!("{:?} {:?} {:?}", v.iter().nth(2), v.iter().last(), v.iter().position(|x| *x == 8));
    let mut out = String::new();
    write!(out, "{}-{}", 1, 2).unwrap();
    writeln!(out, "!").unwrap();
    print!("{out}");
    let mut r = vec![1, 2, 3, 4, 5];
    r.rotate_left(2);
    r.swap(0, 4);
    let tail = r.split_off(3);
    r.truncate(2);
    let dr: Vec<i32> = r.drain(..1).collect();
    r.extend_from_slice(&[9, 9]);
    println!("{:?} {:?} {:?}", r, tail, dr);
    println!("{} {}", vec![vec![1], vec![2, 3]].concat().len(), vec!["a", "b"].join("/"));
    println!("{:?} {:?}", char::from_u32(65), '7'.to_digit(10));
    println!("{} {}", 'a'.to_ascii_uppercase(), "MiXed".to_ascii_lowercase());
    let mut dq: VecDeque<i32> = VecDeque::new();
    dq.push_front(1);
    dq.push_back(2);
    println!("{:?} {}", dq, dq.contains(&2));
    let mut bt = BTreeMap::new();
    bt.insert("b", 2);
    bt.insert("a", 1);
    println!("{:?} {:?}", bt, bt.range("a".."b").collect::<Vec<_>>());
    let hs: HashSet<i32> = [1, 2, 3].into_iter().collect();
    let hs2: HashSet<i32> = [2, 3, 4].into_iter().collect();
    let mut inter: Vec<_> = hs.intersection(&hs2).collect();
    inter.sort();
    println!("{:?}", inter);
    let mut hm: HashMap<String, Vec<i32>> = HashMap::with_capacity(4);
    hm.entry("x".to_string()).or_default().push(1);
    println!("{:?}", hm.get("x"));
    let shared = Rc::new(RefCell::new(vec![1]));
    let weak: Weak<RefCell<Vec<i32>>> = Rc::downgrade(&shared);
    if let Some(s) = weak.upgrade() {
        s.borrow_mut().push(2);
    }
    println!("{:?} {}", shared.borrow(), Rc::ptr_eq(&shared, &shared));
    let c = Cell::new(5);
    c.set(c.get() + 1);
    println!("{}", c.get());
    println!("{:>8.2}|{:<5}|{:^7}|{:+}|{:08.3}|{:e}", 3.14159, "ab", "mid", 5, -2.5, 1234.5);
    println!("{}", i64::MAX.checked_add(1).is_none());
    println!("{:?}", "a=1;b=2".split(';').map(|kv| kv.split_once('=').unwrap()).collect::<Vec<_>>());
    println!("{}", "abc".chars().rev().collect::<String>());
    println!("{:?}", "  trim me ".trim_start().strip_suffix(' '));
    println!("{}", 2u64.pow(40));
    println!("{:?}", "x,y,,z".split(',').filter(|s| !s.is_empty()).collect::<Vec<_>>());
    let mut arr = [0u8; 4];
    arr[1] = 3;
    println!("{:?} {}", arr, arr.len());
    println!("{}", std::f64::consts::PI.floor());
    println!("{:?}", (1..=4).map(|x| x * x).sum::<i32>());
    println!("{:?}", v.iter().copied().reduce(|a, b| a.max(b)));
    println!("{}", v.iter().any(|&x| x > 8) && v.iter().all(|&x| x > 0));
    println!("{:?}", v.chunks_exact(4).map(|c| c.len()).collect::<Vec<_>>());
    println!("{:?}", v.iter().rev().enumerate().map(|(i, x)| i as i32 * x).collect::<Vec<_>>());
    let mut sv = v.clone();
    sv.sort_unstable_by(|a, b| b.cmp(a));
    sv.retain(|x| *x > 2);
    println!("{:?}", sv);
    let mut cnt = 0;
    let rw: Vec<i32> = std::iter::repeat_with(|| { cnt += 2; cnt }).take(3).collect();
    let none: Vec<i32> = std::iter::empty().collect();
    println!("{:?} {:?}", rw, none);
    let words = vec!["a".to_string(), "bb".to_string()];
    let lens: Vec<String> = words.iter().scan(String::new(), |acc, w| { acc.push_str(w); Some(acc.clone()) }).collect();
    let mw: Vec<i32> = [1, 2, -1, 3].iter().map_while(|x| if *x > 0 { Some(x * 10) } else { None }).collect();
    println!("{:?} {:?}", lens, mw);
    println!("{} {} {} {}", 1000u32.ilog10(), 1025u32.ilog2(), 99u64.isqrt(), (-7i32).is_negative());
    println!("{} {} {:.3} {:.4}", (2.75f64).fract(), (-0.0f64).signum(), (4.0f64).recip(), (1.0f64).exp_m1());
    println!("{} {} {}", (-0.0f64).is_sign_negative(), f64::INFINITY.is_infinite(), (8.0f64).log(2.0));
    println!("{} {} {}", (-7.5f64).rem_euclid(2.0), (-7.5f64).div_euclid(2.0), (3.0f64).copysign(-1.0));
    println!("{:.4} {:.4}", (1.0f64).to_degrees(), (0.5f64).tanh());
    let mut bs = std::collections::BTreeSet::new();
    for x in [5, 1, 9, 3, 7] {
        bs.insert(x);
    }
    println!("{:?} {:?}", bs.range(3..=7).collect::<Vec<_>>(), bs.range(..4).collect::<Vec<_>>());
    let mut scores = BTreeMap::new();
    for (k, v) in [(10, "x"), (20, "y"), (30, "z")] {
        scores.insert(k, v);
    }
    for (k, v) in scores.range(15..) {
        print!("{k}={v} ");
    }
    println!();
    let lonely: Weak<i32> = Weak::new();
    println!("{:?}", lonely.upgrade());
    let parts = vec![vec!["a".to_string()], vec!["b".to_string(), "c".to_string()]];
    println!("{:?}", parts.concat());
}
