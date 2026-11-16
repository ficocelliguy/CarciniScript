// Irrefutable array patterns: `let [a, b] = arr;` when the array's length is known (its
// annotation, an array literal, `<[T; N]>::try_from(..)`), also with `..`; and
// `<[T; N]>::try_from` from a `Vec` (its `Err` is the `Vec`) or a slice (`TryFromSliceError`).
fn position() -> [f64; 4] {
    [10.0, 1.5, 0.0, 0.0]
}

fn dims(v: Vec<i64>) -> Option<i64> {
    let [r, c] = <[i64; 2]>::try_from(v).ok()?;
    Some(r * c)
}

fn main() {
    let [long, avg, short, short_avg]: [f64; 4] = position();
    println!("{long} {avg} {short} {short_avg}");
    let [first, .., last] = [1, 2, 3, 4, 5];
    let [x, y] = [[1u8, 2], [3, 4]];
    println!("{first} {last} {x:?} {y:?}");
    let [head, rest @ ..]: [char; 3] = ['a', 'b', 'c'];
    println!("{head} {rest:?}");
    let [(a, b), _] = [(1, 'x'), (2, 'y')];
    println!("{a} {b}");
    println!("{:?} {:?}", dims(vec![3, 4]), dims(vec![1, 2, 3]));
    let v = vec![7, 8];
    println!("{:?}", <[i32; 2]>::try_from(v));
    println!("{:?}", <[i32; 3]>::try_from(vec![1]));
    let s = [1, 2, 3, 4];
    let pair = <[i32; 2]>::try_from(&s[1..3]);
    let bad = <[i32; 2]>::try_from(&s[..]);
    println!("{pair:?} {bad:?}");
    if let Err(e) = bad {
        println!("{e}");
    }
}
