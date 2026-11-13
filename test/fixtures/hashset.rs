// HashSet: membership, tuples as keys (a grid flood fill), set relations, cloning.
// Iteration order is unspecified in Rust, so anything printed is sorted or order-independent.
use std::collections::HashSet;

fn flood(grid: &Vec<Vec<char>>, start: (usize, usize)) -> usize {
    let mut seen: HashSet<(usize, usize)> = HashSet::new();
    let mut stack = vec![start];
    while let Some((r, c)) = stack.pop() {
        if grid[r][c] == '#' || !seen.insert((r, c)) {
            continue;
        }
        if r > 0 {
            stack.push((r - 1, c));
        }
        if r + 1 < grid.len() {
            stack.push((r + 1, c));
        }
        if c > 0 {
            stack.push((r, c - 1));
        }
        if c + 1 < grid[r].len() {
            stack.push((r, c + 1));
        }
    }
    seen.len()
}

fn main() {
    let mut words = HashSet::new();
    println!("{} {} {}", words.insert("apple"), words.insert("pear"), words.insert("apple"));
    println!("{} {} {}", words.len(), words.contains(&"pear"), words.contains(&"fig"));
    println!("{} {}", words.remove(&"pear"), words.remove(&"pear"));
    println!("{:?} {}", words, words.is_empty());

    let rows = ["..#..", ".##..", "....#", "###.."];
    let grid: Vec<Vec<char>> = vec![
        vec!['.', '.', '#', '.', '.'],
        vec!['.', '#', '#', '.', '.'],
        vec!['.', '.', '.', '.', '#'],
        vec!['#', '#', '#', '.', '.'],
    ];
    let _ = rows;
    println!("flood from (0,0): {}", flood(&grid, (0, 0)));
    println!("flood from (3,4): {}", flood(&grid, (3, 4)));

    let evens: HashSet<u32> = HashSet::from([2, 4, 6, 8]);
    let small: HashSet<u32> = HashSet::from([2, 4]);
    let odds: HashSet<u32> = HashSet::from([1, 3]);
    println!("{} {} {} {}", small.is_subset(&evens), evens.is_superset(&small), evens.is_disjoint(&odds), small.is_subset(&odds));

    let mut copy = evens.clone();
    copy.insert(10);
    copy.remove(&2);
    println!("{} {} {}", evens.len(), copy.len(), evens == copy);
    println!("{}", HashSet::from([1, 2, 3]) == HashSet::from([3, 2, 1]));

    let mut sum = 0;
    for x in &evens {
        sum += *x;
    }
    let mut sorted = Vec::new();
    for x in copy.iter() {
        sorted.push(*x);
    }
    sorted.sort();
    println!("sum {sum}, sorted {:?}", sorted);

    let mut letters: HashSet<char> = HashSet::new();
    for c in ['m', 'i', 's', 's', 'i', 's', 's', 'i', 'p', 'p', 'i'] {
        letters.insert(c);
    }
    println!("{} distinct letters", letters.len());
    letters.clear();
    let single: HashSet<(i32, String)> = HashSet::from([(1, String::from("one"))]);
    println!("{} {:?}", letters.len(), single);
}
