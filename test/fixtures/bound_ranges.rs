// `BTreeMap::range` / `BTreeSet::range` over `Bound` pairs and range values, and
// `RangeBounds::start_bound` / `end_bound`.
use std::collections::{BTreeMap, BTreeSet};
use std::ops::{Bound, RangeBounds};

fn describe<R: RangeBounds<i32>>(r: &R) -> String {
    format!("{:?} {:?}", r.start_bound(), r.end_bound())
}

fn between<R: RangeBounds<i32>>(m: &BTreeMap<i32, i32>, r: R) -> Vec<i32> {
    m.range(r).map(|(k, _)| *k).collect()
}

fn by_value<R: RangeBounds<i32>>(r: R) -> String {
    describe(&r)
}

fn unbounded_start(r: impl RangeBounds<u8>) -> bool {
    matches!(r.start_bound(), Bound::Unbounded)
}

fn main() {
    let mut m = BTreeMap::new();
    for i in 0..10 {
        m.insert(i, i * 2);
    }
    let v: Vec<_> = m.range((Bound::Excluded(2), Bound::Included(5))).collect();
    println!("{:?}", v);
    let v: Vec<_> = m.range((Bound::Included(7), Bound::Unbounded)).collect();
    println!("{:?}", v);
    let v: Vec<_> = m.range((Bound::Unbounded, Bound::Excluded(2))).collect();
    println!("{:?}", v);
    let pair = (Bound::Excluded(3), Bound::Excluded(4));
    println!("{}", m.range(pair).count());
    let lo = 6;
    let pair: (Bound<i32>, Bound<i32>) = (Bound::Excluded(lo), Bound::Excluded(lo + 3));
    let keys: Vec<i32> = m.range(pair).map(|(k, _)| *k).collect();
    println!("{:?}", keys);
    let r = 2..4;
    println!("{:?}", m.range(r).collect::<Vec<_>>());
    let mid = 5..=6;
    println!("{:?}", between(&m, mid));
    println!("{:?}", between(&m, (Bound::Excluded(8), Bound::Unbounded)));

    let s: BTreeSet<u32> = [5, 10, 15, 20].into_iter().collect();
    let some: Vec<_> = s.range((Bound::Excluded(5), Bound::Included(15))).collect();
    println!("{:?}", some);
    let names: BTreeSet<String> = ["ant", "bee", "cat"].iter().map(|s| s.to_string()).collect();
    let after = (Bound::Excluded("ant".to_string()), Bound::Unbounded);
    println!("{:?}", names.range(after).collect::<Vec<_>>());

    let r = 3..7;
    println!("{:?} {:?} {}", r.start_bound(), r.end_bound(), r.contains(&4));
    println!("{:?} {:?}", (1..=4).start_bound(), (1..=4).end_bound());
    println!("{:?} {:?}", (2..).start_bound(), (..5).end_bound());
    let x: Bound<&i32> = (..=5).end_bound();
    println!("{:?}", x);
    println!("{}", describe(&(10..20)));
    println!("{}", describe(&(Bound::Excluded(1), Bound::Unbounded)));
    // A range literal passed for a `RangeBounds` parameter, `..` too.
    println!("{:?} {:?} {:?}", between(&m, 2..4), between(&m, 8..), between(&m, ..=1));
    println!("{} {}", between(&m, ..).len(), m.range(..).count());
    println!("{} {}", by_value(..), by_value(1..2));
    println!("{}", unbounded_start(..3) && !unbounded_start(0..=1));
    match r.end_bound() {
        Bound::Excluded(e) => println!("ends before {}", e),
        _ => println!("other"),
    }
}
