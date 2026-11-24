// `std::panic::AssertUnwindSafe`: a closure that borrows state mutably, wrapped for
// `catch_unwind`; the wrapper's `Deref` / `DerefMut`, `.0` and `{:?}`; and a test runner in a
// script: tests listed by a macro, each run under `catch_unwind`.
use std::panic::{self, AssertUnwindSafe};

macro_rules! tests {
    ($($name:ident $body:block)*) => {
        $(fn $name() $body)*
        const TESTS: &[(&str, fn())] = &[$((stringify!($name), $name)),*];
    };
}

fn ways_to_sum(n: usize) -> u64 {
    let mut ways = vec![0u64; n + 1];
    ways[0] = 1;
    for part in 1..n {
        for total in part..=n {
            ways[total] += ways[total - part];
        }
    }
    ways[n]
}

tests! {
    known { assert_eq!(ways_to_sum(5), 6); }
    wrong { assert_eq!(ways_to_sum(5), 7); }
    explodes { let v: Vec<u64> = Vec::new(); let _ = v[3]; }
}

fn message(e: &Box<dyn std::any::Any + Send>) -> String {
    if let Some(s) = e.downcast_ref::<&str>() {
        s.to_string()
    } else if let Some(s) = e.downcast_ref::<String>() {
        s.clone()
    } else {
        "?".to_string()
    }
}

fn main() {
    // A closure that changes `log`: rustc wants the assertion.
    let mut log = vec![1];
    let r = panic::catch_unwind(AssertUnwindSafe(|| {
        log.push(2);
        if log.len() > 1 {
            panic!("boom at {}", log.len());
        }
        7
    }));
    println!("{} {:?}", r.is_err(), log);
    let ok = panic::catch_unwind(AssertUnwindSafe(|| log.len() * 10));
    println!("{:?}", ok.map_err(|e| message(&e)));

    // The wrapper itself: methods through `Deref` / `DerefMut`, the field, `{:?}`.
    let mut w = AssertUnwindSafe(&mut log);
    w.push(3);
    w.0.push(4);
    println!("{} {:?}", w.len(), w);
    let n = AssertUnwindSafe(5);
    println!("{} {:?}", *n + 1, n);
    // (Written out.)
    let m = std::panic::AssertUnwindSafe(2);
    let k = panic::catch_unwind(panic::AssertUnwindSafe(|| *m * 3));
    println!("{:?}", k.is_ok_and(|v| v == 6));

    // Tests under `catch_unwind`, counting the outcomes as they go.
    let mut passed = 0;
    let mut failures = Vec::new();
    for (name, f) in TESTS {
        match panic::catch_unwind(AssertUnwindSafe(|| {
            f();
            passed += 1;
        })) {
            Ok(()) => println!("test {name} ... ok"),
            Err(e) => {
                println!("test {name} ... FAILED");
                failures.push(format!("{name}: {}", message(&e)));
            }
        }
    }
    for f in &failures {
        println!("{f}");
    }
    println!("{passed} passed; {} failed", failures.len());
}
