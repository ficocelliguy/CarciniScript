// `type` aliases inside functions and blocks, scoped like rustc: an inner `type Id` shadows the
// file's, a block's alias ends with the block, and `self::Num` names a file-level one.
type Id = u32;

fn tally(words: &[&str]) -> usize {
    type Counts = std::collections::HashMap<String, usize>;
    let mut c = Counts::new();
    for w in words {
        *c.entry(w.to_string()).or_insert(0) += 1;
    }
    c.len()
}

fn main() {
    type Id = (u8, u8);
    let a: Id = (1, 2);
    {
        type Id = String;
        let b: Id = "inner".to_string();
        println!("{}", b);
    }
    let n: self::Num = 3;
    println!("{:?} {} {}", a, tally(&["x", "y", "x"]), n + first());
}

type Num = i64;

fn first() -> Num {
    let x: Id = 7;
    x as Num
}
