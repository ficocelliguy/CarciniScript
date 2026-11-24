// `RangeBounds` named without `Bound`: a range literal passed for an `R: RangeBounds<K>`.
use std::collections::BTreeMap;
use std::ops::RangeBounds;
fn take<R: RangeBounds<i32>>(m: &BTreeMap<i32, i32>, r: R) -> Vec<i32> { m.range(r).map(|(k, _)| *k).collect() }
fn main() {
    let m: BTreeMap<i32, i32> = (0..10).map(|i| (i, i)).collect();
    println!("{:?}", take(&m, 5..=6));
    println!("{:?}", take(&m, 2..4));
    println!("{:?}", take(&m, ..3));
}
