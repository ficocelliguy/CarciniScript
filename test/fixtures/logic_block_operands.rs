// `{ .. }` blocks (and `match` / `if`) on the right of `&&` / `||`: their statements run only when
// the left side doesn't decide, in `let`s, `if` / `else if` and `while` conditions. Also maps and
// sets with a `S: BuildHasher` parameter (the hasher is accepted and ignored).
use std::collections::hash_map::RandomState;
use std::collections::{HashMap, HashSet};
use std::hash::{BuildHasher, Hash};

fn size<K, V, S: BuildHasher>(m: &HashMap<K, V, S>) -> usize {
    m.len()
}

fn get_or<K: Hash + Eq, S>(m: &HashMap<K, i32, S>, k: &K) -> i32
where
    S: BuildHasher,
{
    *m.get(k).unwrap_or(&-1)
}

fn uniq<S: BuildHasher>(s: &HashSet<i32, S>) -> usize {
    s.len()
}

fn build<S: BuildHasher + Default>() -> HashMap<String, u8, S> {
    let mut m = HashMap::with_hasher(S::default());
    m.insert("a".to_string(), 1);
    m
}

fn check(n: i32, log: &mut Vec<String>) -> bool {
    log.push(format!("check {}", n));
    n > 1
}

fn classify(v: &[i32]) -> &'static str {
    if !v.is_empty() && {
        let s: i32 = v.iter().sum();
        s > 10
    } {
        "big"
    } else if v.len() == 1
        || match v.first() {
            Some(x) => *x < 0,
            None => false,
        }
    {
        "odd"
    } else {
        "small"
    }
}

fn main() {
    let mut m: HashMap<i32, i32, RandomState> = HashMap::default();
    m.insert(1, 2);
    println!("{} {} {}", size(&m), get_or(&m, &1), get_or(&m, &7));
    let mut s: HashSet<i32, RandomState> = HashSet::with_hasher(RandomState::new());
    s.insert(3);
    s.insert(3);
    println!("{}", uniq(&s));
    let b: HashMap<String, u8, RandomState> = build();
    println!("{:?}", b);

    let mut log = Vec::new();
    let a = true && {
        let y = 3;
        check(y, &mut log)
    };
    let b = false || {
        let z = 0;
        check(z, &mut log)
    };
    let c = false && {
        let z = 5;
        check(z, &mut log)
    };
    let d = true || { check(9, &mut log) };
    let e = check(2, &mut log) && check(3, &mut log) && {
        let w = 1;
        check(w, &mut log)
    };
    println!("{} {} {} {} {} {:?}", a, b, c, d, e, log);
    println!(
        "{} {} {} {}",
        classify(&[5, 9]),
        classify(&[1, 2]),
        classify(&[-3, 1]),
        classify(&[4])
    );
    let mut i = 0;
    let mut seen = Vec::new();
    while i < 10 && {
        i += 1;
        i % 4 != 0
    } {
        seen.push(i);
    }
    println!("{:?}", seen);
    let r: Option<i32> = Some(3);
    let ok = r.is_some() && {
        if let Some(v) = r { v > 2 } else { false }
    };
    let none: Option<i32> = None;
    let found = none.is_none() || {
        let v = none.unwrap();
        v > 0
    };
    println!("{} {}", ok, found);

    // In match guards, closures and nested chains.
    let v = vec![Some(3), None, Some(-2), Some(12)];
    for o in &v {
        let s = match o {
            Some(x)
                if *x > 0 && {
                    let y = x * 2;
                    y < 10
                } =>
            {
                "small"
            }
            Some(x)
                if *x < 0 || {
                    let z = *x;
                    z > 10
                } =>
            {
                "edge"
            }
            Some(_) => "other",
            None => "none",
        };
        println!("{}", s);
    }
    let f = |x: i32| {
        x > 0 && {
            let h = x / 2;
            h > 1
        }
    };
    println!("{} {}", f(3), f(5));
    let g: Vec<i32> = (0..8)
        .filter(|x| {
            *x > 2 && {
                let m = x % 3;
                m == 0
            }
        })
        .collect();
    println!("{:?}", g);
    let mut n = 0;
    loop {
        n += 1;
        if n > 3 || {
            let q = n * n;
            q > 5
        } {
            break;
        }
    }
    println!("{}", n);
    let both = true && {
        let r = 1;
        r == 1
    } && false
        || {
            let s = 2;
            s == 2
        };
    println!("{}", both);
}
