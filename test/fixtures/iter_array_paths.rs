// Iterators over a Vec work by index: `rev`, `step_by`, `skip`, `take`, `last`, `count`, `nth`
// and `collect` don't copy or step through every item, and `by_ref()` adapters still consume
// the same iterator. `windows` / `chunks` are class iterators.
fn main() {
    let v: Vec<i32> = (1..=10).collect();
    println!("{:?}", v.iter().rev().collect::<Vec<_>>());
    println!("{:?}", v.iter().rev().rev().take(3).collect::<Vec<_>>());
    println!("{:?}", v.iter().step_by(3).collect::<Vec<_>>());
    println!("{:?}", v.iter().step_by(1).count());
    println!("{:?}", v.iter().step_by(100).collect::<Vec<_>>());
    println!("{:?}", v.iter().skip(7).collect::<Vec<_>>());
    println!("{:?}", v.iter().skip(70).collect::<Vec<_>>());
    println!("{:?}", v.iter().take(0).collect::<Vec<_>>());
    println!("{:?}", v.iter().take(70).count());
    println!("{:?} {:?}", v.iter().last(), Vec::<i32>::new().iter().last());
    println!("{} {}", v.iter().count(), v.iter().skip(4).count());
    println!("{:?} {:?} {:?}", v.iter().nth(0), v.iter().nth(9), v.iter().nth(10));
    println!("{:?}", v.iter().rev().take(3).map(|x| x * 2).collect::<Vec<_>>());
    println!("{:?}", v.iter().skip(2).rev().step_by(2).collect::<Vec<_>>());
    println!("{:?}", v.iter().rev().skip(1).step_by(4).collect::<Vec<_>>());
    println!("{:?}", v.iter().enumerate().rev().take(2).collect::<Vec<_>>());
    println!("{:?}", v.iter().rev().enumerate().skip(8).collect::<Vec<_>>());
    println!("{:?}", v.iter().skip(3).take(4).rev().collect::<Vec<_>>());
    println!("{:?}", v.iter().rev().last());
    println!("{:?} {}", v.iter().rev().nth(2), v.iter().rev().count());
    println!("{}", v.iter().rev().take(1000).map(|&x| x as i64).sum::<i64>());
    let mut it = v.iter();
    println!("{:?} {:?}", it.next(), it.nth(1));
    println!("{:?}", it.by_ref().take(2).collect::<Vec<_>>());
    println!("{:?}", it.next());
    println!("{:?}", it.by_ref().rev().take(2).collect::<Vec<_>>());
    println!("{:?} {:?}", it.next(), it.clone().count());
    println!("{:?}", it.by_ref().step_by(2).collect::<Vec<_>>());
    println!("{:?} {}", it.next(), it.count());
    let mut it2 = v.iter();
    let skipped = it2.by_ref().skip(3);
    drop(skipped);
    println!("{:?}", it2.next());
    let mut it3 = v.iter();
    println!("{:?} {:?} {:?}", it3.next_back(), it3.next(), it3.last());
    let mut it4 = v.iter();
    println!("{} {:?}", it4.by_ref().count(), it4.next());
    let w: Vec<String> = ["a", "b", "c", "d"].iter().map(|s| s.to_string()).collect();
    println!("{:?} {:?}", w.iter().rev().collect::<Vec<_>>(), w.iter().rev().last());
    println!("{:?}", w.into_iter().rev().step_by(2).collect::<Vec<_>>());
    println!("{:?}", v.windows(3).map(|w| w.iter().sum::<i32>()).collect::<Vec<_>>());
    println!("{:?}", v.windows(10).count());
    println!("{:?}", v.windows(11).count());
    println!("{:?}", v.chunks(4).collect::<Vec<_>>());
    println!("{:?}", v.chunks(10).count());
    println!("{:?}", v.chunks(11).map(|c| c.len()).collect::<Vec<_>>());
    println!("{:?}", v.chunks_exact(4).collect::<Vec<_>>());
    println!("{:?}", v.chunks_exact(11).count());
    let kept: Vec<&[i32]> = v.windows(2).filter(|w| w[0] % 2 == 1).collect();
    println!("{:?}", kept);
    let mut wi = v.windows(4);
    println!("{:?} {:?}", wi.next(), wi.next());
    let empty: Vec<i32> = Vec::new();
    println!("{:?} {:?} {:?}", empty.windows(1).count(), empty.chunks(2).count(), empty.iter().rev().next());
    let big: Vec<i64> = (0..100_000).collect();
    let mut total = 0i64;
    for x in big.iter().rev() {
        total += *x;
    }
    for x in big.iter().step_by(7) {
        total += *x;
    }
    for x in big.iter().skip(99_990) {
        total += *x;
    }
    println!("{} {:?} {}", total, big.iter().rev().nth(1), big.iter().skip(5).step_by(1000).count());
}
