// Tuples: construction, fields, destructuring, patterns, ordering, copies, enumerate.

fn min_max(values: &[i32]) -> (i32, i32) {
    let mut lo = values[0];
    let mut hi = values[0];
    for v in values {
        if *v < lo {
            lo = *v;
        }
        if *v > hi {
            hi = *v;
        }
    }
    (lo, hi)
}

fn describe(pair: (bool, bool)) -> &'static str {
    // Exhaustive without a `_` arm.
    match pair {
        (true, true) => "both",
        (true, false) => "first",
        (false, true) => "second",
        (false, false) => "neither",
    }
}

fn fizz(n: u32) -> String {
    match (n % 3, n % 5) {
        (0, 0) => String::from("FizzBuzz"),
        (0, _) => String::from("Fizz"),
        (_, 0) => String::from("Buzz"),
        _ => n.to_string(),
    }
}

fn main() {
    let (lo, hi) = min_max(&[4, -2, 9, 3]);
    println!("{lo} {hi}");

    let mut point = (3, -4);
    point.0 += 10;
    point.1 *= 2;
    println!("{:?} {} {}", point, point.0, point.1);

    // Tuples are values: changing a copy leaves the original alone.
    let original = (1, String::from("a"));
    let mut copy = original.clone();
    copy.0 = 99;
    let mut moved_copy = point;
    moved_copy.1 = 0;
    println!("{:?} {:?} {:?} {:?}", original, copy, point, moved_copy);

    let mut a = 1;
    let mut b = 2;
    (a, b) = (b, a);
    println!("swapped {a} {b}");

    for pair in [(true, true), (true, false), (false, true), (false, false)] {
        print!("{} ", describe(pair));
    }
    println!();
    for n in 9..=15 {
        print!("{} ", fizz(n));
    }
    println!();

    let mut scores: Vec<(u32, String)> = vec![(3, String::from("c")), (1, String::from("z")), (3, String::from("a"))];
    scores.sort();
    println!("{:?}", scores);
    println!("{} {}", (1, 2) < (1, 3), (2, 0) > (1, 9));

    let mut grid = vec![(0, 0); 3];
    for (i, cell) in grid.iter_mut().enumerate() {
        cell.0 = i * 10;
        cell.1 = i + 1;
    }
    let mut total = 0;
    for (i, (x, y)) in grid.iter().enumerate() {
        total += i * (*x + *y);
    }
    println!("{:?} total {total}", grid);
    let mut stored = Vec::new();
    let mut p = (1, 1);
    stored.push(p);
    p.0 = 5;
    println!("{:?} {:?}", stored, p);

    let nested = ((1, 'x'), [2u8, 3], Some((4.5, "y")));
    let ((n, c), arr, opt) = nested;
    println!("{n} {c} {:?} {:?} {:?}", arr, opt, (7,));
    if let Some((f, s)) = opt {
        println!("{f} {s}");
    }
    let (first, .., last) = (1, 2, 3, 4);
    println!("{first} {last}");
    returned_tuples_are_copies();
}

fn first(pairs: &Vec<(i32, i32)>) -> (i32, i32) {
    pairs[0]
}

fn returned_tuples_are_copies() {
    let pairs = vec![(1, 2), (3, 4)];
    let mut p = first(&pairs);
    p.0 = 100;
    let q = if p.0 > 50 { pairs[1] } else { pairs[0] };
    println!("{:?} {:?} {:?}", pairs, p, q);
}
