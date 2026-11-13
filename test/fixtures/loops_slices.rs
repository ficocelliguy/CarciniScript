// `break value` from `loop` (plain, labeled, nested, as a function's tail and in `let`), and slice
// patterns: exact lengths, `..` in any position, `rest @ ..` bindings, nested and guarded, with
// exhaustiveness over lengths.
fn first_square_over(limit: u32) -> u32 {
    let mut n = 1;
    loop {
        if n * n > limit {
            break n * n;
        }
        n += 1;
    }
}

fn find_pair(target: i32) -> Option<(i32, i32)> {
    let found = 'outer: loop {
        for a in 1..10 {
            for b in a..10 {
                if a * b == target {
                    break 'outer Some((a, b));
                }
            }
        }
        break None;
    };
    found
}

fn describe(v: &[i32]) -> String {
    match v {
        [] => "empty".to_string(),
        [x] => format!("one: {}", x),
        [first, .., last] if first == last => format!("bookends {}", first),
        [first, second, rest @ ..] => format!("{} then {} then {} more", first, second, rest.len()),
    }
}

fn sum_ends(v: &[i32]) -> i32 {
    match v {
        [.., a, b] => a + b,
        [x] => *x,
        [] => 0,
    }
}

fn main() {
    println!("{} {}", first_square_over(50), first_square_over(1));
    println!("{:?} {:?}", find_pair(12), find_pair(97));
    let mut tries = 0;
    let got = loop {
        tries += 1;
        if tries == 4 {
            break tries * 100;
        }
    };
    println!("{} {}", got, tries);
    let label = loop {
        break "done";
    };
    println!("{}", label);
    for v in [vec![], vec![7], vec![3, 9, 3], vec![1, 2, 3, 4, 5], vec![4, 6]] {
        println!("{} | {}", describe(&v), sum_ends(&v));
    }
    let words = vec!["go", "north", "fast"];
    if let [cmd, args @ ..] = words.as_slice() {
        println!("{} {:?}", cmd, args);
    }
    let pairs = vec![(1, 'a'), (2, 'b')];
    match pairs.as_slice() {
        [(n, c), ..] => println!("{} {}", n, c),
        [] => println!("none"),
    }
}
