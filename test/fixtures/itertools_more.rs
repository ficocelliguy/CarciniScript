// itertools' `izip!` / `iproduct!` macros, `minmax` (`_by`, `_by_key`) into `MinMaxResult`,
// `exactly_one`, `fold_while` with `FoldWhile`, and `kmerge` / `kmerge_by`.
use itertools::FoldWhile::{Continue, Done};
use itertools::MinMaxResult::{MinMax, NoElements, OneElement};
use itertools::{iproduct, izip, Itertools, MinMaxResult};

fn main() {
    let a = vec![1, 2, 3];
    let b = vec!["x", "y", "z", "w"];
    let c = [1.5, 2.5, 3.5];
    for (n, s, f) in izip!(&a, &b, &c) {
        println!("{} {} {}", n, s, f);
    }
    let sums: Vec<i32> = izip!(a.iter(), a.iter().rev()).map(|(x, y)| x + y).collect();
    println!("{:?}", sums);
    let four: Vec<(i32, i32, i32, i32)> = izip!(vec![1, 2], vec![3, 4], vec![5, 6], vec![7, 8]).collect();
    println!("{:?}", four);
    let pairs: Vec<(i32, char)> = iproduct!(0..2, ['a', 'b']).collect();
    println!("{:?}", pairs);
    let triples = iproduct!(0..2, 0..2, 0..2).filter(|(x, y, z)| x + y + z == 2).count();
    println!("{}", triples);
    let single: Vec<(i32,)> = iproduct!(vec![7, 8]).collect();
    println!("{:?}", single);
    for (i, j) in itertools::iproduct!(1..3, 1..3) {
        print!("{} ", i * j);
    }
    println!();

    let v = vec![4, 1, 9, 1, 9, 3];
    match v.iter().minmax() {
        NoElements => println!("none"),
        OneElement(x) => println!("one {}", x),
        MinMax(lo, hi) => println!("{} {}", lo, hi),
    }
    let empty: Vec<i32> = vec![];
    println!("{:?}", empty.iter().minmax());
    println!("{:?}", vec![5].into_iter().minmax());
    println!("{:?}", v.iter().minmax().into_option());
    let words = vec!["pear", "fig", "banana", "kiwi", "plum"];
    if let MinMaxResult::MinMax(s, l) = words.iter().minmax_by_key(|w| w.len()) {
        println!("{} {}", s, l);
    }
    let r = words.iter().minmax_by(|x, y| x.cmp(y));
    println!("{:?}", r);
    let fl = vec![2.5, -1.0, 7.25];
    println!("{:?}", fl.iter().copied().minmax().into_option());

    println!("{:?}", vec![42].into_iter().exactly_one());
    let none = empty.iter().exactly_one();
    println!("{}", none.is_err());
    match v.iter().filter(|x| **x > 3).exactly_one() {
        Ok(x) => println!("just {}", x),
        Err(e) => {
            println!("{}", e);
            println!("{:?}", e.collect::<Vec<_>>());
        }
    }
    if let Err(e) = (0..0).exactly_one() {
        println!("{}", e);
    }
    let found = v.iter().filter(|x| **x == 3).exactly_one().ok();
    println!("{:?}", found);

    let total = v
        .iter()
        .fold_while(0, |acc, x| {
            if acc + x > 10 {
                Done(acc)
            } else {
                Continue(acc + x)
            }
        })
        .into_inner();
    println!("{}", total);
    let all = a.iter().fold_while(0, |acc, x| Continue(acc + x));
    println!("{} {}", all.is_done(), all.into_inner());
    let mut it = v.iter();
    let first_big = it.fold_while(None, |_, x| if *x > 5 { Done(Some(*x)) } else { Continue(None) });
    println!("{:?} {:?}", first_big, it.next());

    let merged: Vec<i32> = vec![vec![1, 4, 7], vec![2, 5, 8], vec![0, 3, 6, 9]].into_iter().kmerge().collect();
    println!("{:?}", merged);
    let tagged = vec![vec![(1, 'a'), (3, 'a')], vec![(1, 'b'), (2, 'b')], vec![(1, 'c')]];
    let by: Vec<(i32, char)> = tagged.into_iter().kmerge_by(|x, y| x.0 < y.0).collect();
    println!("{:?}", by);
    let desc: Vec<i32> = vec![vec![9, 5, 1], vec![8, 2]].into_iter().kmerge_by(|x, y| x > y).collect();
    println!("{:?}", desc);
}
