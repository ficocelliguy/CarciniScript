// In-place slice methods on a range of a Vec change the Vec: `sort` (and `_by`, `_by_key`,
// `_unstable`), `reverse`, `fill`, `swap`, `rotate_left`/`right` and `copy_from_slice`, on locals
// and on struct fields, with open-ended ranges and computed bounds.
struct Board {
    cells: Vec<char>,
}

fn main() {
    let mut v = vec![3, 9, 2, 1, 5, 8];
    v[1..4].sort();
    println!("{:?}", v);
    v[..2].reverse();
    println!("{:?}", v);
    v[4..].fill(0);
    println!("{:?}", v);
    v[1..3].swap(0, 1);
    println!("{:?}", v);
    let mid = v.len() / 2;
    v[mid..].rotate_left(1);
    v[..mid].rotate_right(1);
    println!("{:?}", v);
    v[..3].sort_by(|a, b| b.cmp(a));
    println!("{:?}", v);
    v[2..].sort_by_key(|x| (*x as i32 - 4).abs());
    println!("{:?}", v);
    v[0..2].copy_from_slice(&[7, 7]);
    v[..].sort_unstable();
    println!("{:?}", v);

    let mut b = Board { cells: "dcbahgfe".chars().collect() };
    b.cells[..4].sort();
    b.cells[4..].sort();
    println!("{}", b.cells.iter().collect::<String>());

    let mut words = vec!["pear", "fig", "apple", "kiwi"];
    words[1..].sort_by_key(|w| w.len());
    println!("{:?}", words);
}
