// Slice patterns on arrays: the length is known, so `[first, .., last]` covers every value of a
// `[T; 3]` (rustc doesn't ask for `[]`), and `let` / `for` can destructure nested arrays.

const N: usize = 4;

fn ends(a: [i32; 3]) -> (i32, i32) {
    match a {
        [first, .., last] => (first, last),
    }
}

fn head(a: &[u8; N]) -> u8 {
    match a {
        [h, ..] => *h,
    }
}

fn classify(a: [i32; 2]) -> &'static str {
    match a {
        [0, 0] => "origin",
        [0, _] => "y axis",
        [_, 0] => "x axis",
        [_, _] => "other",
    }
}

fn flags(f: [bool; 2]) -> u8 {
    match f {
        [true, true] => 3,
        [true, false] => 2,
        [false, true] => 1,
        [false, false] => 0,
    }
}

fn corners(g: [[u8; 2]; 2]) -> u8 {
    match g {
        [[a, _], [_, d]] => a + d,
    }
}

fn make() -> [char; 3] {
    ['x', 'y', 'z']
}

fn main() {
    println!("{:?}", ends([1, 2, 3]));
    println!("{}", head(&[9, 8, 7, 6]));
    println!(
        "{} {} {} {}",
        classify([0, 0]),
        classify([0, 4]),
        classify([3, 0]),
        classify([1, 1])
    );
    println!("{} {} {} {}", flags([true, true]), flags([true, false]), flags([false, true]), flags([false, false]));
    println!("{}", corners([[1, 2], [3, 4]]));

    let [a, b, c] = [10, 20, 30];
    println!("{} {} {}", a, b, c);
    let [x, rest @ ..] = [1, 2, 3, 4];
    println!("{} {:?}", x, rest);
    match [1, 2, 3] {
        [1, mid @ .., 3] => println!("mid {:?}", mid),
        [..] => println!("any"),
    }
    let grid = [[1, 2], [3, 4]];
    let [[p, _], [_, q]] = grid;
    println!("{} {}", p, q);
    let typed: [[i32; 3]; 2] = [[1, 2, 3], [4, 5, 6]];
    let [[first, ..], [.., last]] = typed;
    println!("{} {}", first, last);
    for [k, v] in [[1, 2], [3, 4]] {
        print!("{}={} ", k, v);
    }
    println!();
    let pairs = [["a", "b"], ["c", "d"]];
    for [l, r] in pairs.iter() {
        print!("{}{} ", l, r);
    }
    println!();
    let [c0, .., c2] = make();
    println!("{} {}", c0, c2);
    let m = match make() {
        ['x', ..] => "starts with x",
        [_, _, _] => "other",
    };
    println!("{}", m);
}
