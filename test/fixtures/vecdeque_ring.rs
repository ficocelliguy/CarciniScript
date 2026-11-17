// `VecDeque` as a ring buffer: everything a `Vec` shares with it still works after the head has
// wrapped around the backing array.
use std::collections::{HashMap, VecDeque};

#[derive(Clone, Debug, PartialEq, Default)]
struct Line {
    q: VecDeque<i32>,
    name: String,
}

fn bump(x: &mut i32) {
    *x += 100;
}

fn total(q: &VecDeque<i32>) -> i32 {
    q.iter().sum()
}

fn main() {
    let mut q: VecDeque<i32> = VecDeque::new();
    // Push and pop so the head wraps around many times.
    for i in 0..20 {
        q.push_back(i);
        q.push_front(-i);
        if i % 3 == 0 {
            q.pop_front();
        }
        if i % 4 == 0 {
            q.pop_back();
        }
    }
    println!("{} {:?}", q.len(), q);
    println!("{:?} {:?} {:?} {:?}", q.front(), q.back(), q.get(3), q.get(100));
    println!("{} {} {}", q[0], q[q.len() - 1], q[5]);
    q[2] = 77;
    q[3] += 1;
    bump(&mut q[4]);
    println!("{:?}", q);
    for x in q.iter_mut() {
        *x *= 2;
    }
    for (i, x) in q.iter_mut().enumerate() {
        if i % 2 == 0 {
            *x += 1;
        }
    }
    println!("{:?} {}", q, total(&q));
    let mut seen = Vec::new();
    for (i, x) in q.iter().enumerate() {
        if i < 3 {
            seen.push(*x);
        }
    }
    println!("{:?} {:?}", seen, q.iter().rev().take(3).collect::<Vec<_>>());
    println!("{} {}", q.contains(&77), q.contains(&155));
    q.retain(|x| *x % 2 != 0);
    println!("{:?}", q);
    q.insert(1, 500);
    println!("{:?} {:?}", q.remove(0), q.remove(99));
    q.swap(0, 1);
    println!("{:?}", q);
    let drained: Vec<i32> = q.drain(1..3).collect();
    println!("{:?} {:?}", drained, q);
    q.extend(vec![9, 8, 7]);
    q.extend([6, 5].iter().copied());
    let mut other: VecDeque<i32> = (1..=3).collect();
    q.append(&mut other);
    println!("{:?} {:?} {}", q, other, other.is_empty());
    q.truncate(6);
    println!("{:?}", q);
    let c = q.clone();
    q.make_contiguous().sort();
    println!("{:?} {:?} {}", q, c, q == c);
    q.make_contiguous().reverse();
    println!("{:?} {}", q, q == VecDeque::from(vec![500, 9, 8, 7, 6, 5]));
    println!("{:?}", q.make_contiguous().binary_search(&8));
    q.make_contiguous().rotate_left(2);
    println!("{:?}", q);
    q.make_contiguous().rotate_right(1);
    println!("{:?}", q);
    let v: Vec<i32> = q.iter().copied().collect();
    let v2: Vec<i32> = q.iter().rev().copied().collect();
    let back: VecDeque<i32> = v.clone().into();
    println!("{:?} {:?} {:?} {}", v, v2, back, back == q);
    let w: VecDeque<i32> = std::mem::take(&mut q);
    println!("{:?} {:?} {}", w, q, q.is_empty());
    q.push_back(1);
    let mut r = VecDeque::from([2, 3]);
    std::mem::swap(&mut q, &mut r);
    println!("{:?} {:?}", q, r);
    q.clear();
    println!("{:?} {}", q, q.len());
    println!("{:?} {:?}", q.pop_front(), q.pop_back());

    // Many pops after many pushes: the backing array wraps and grows.
    let mut big: VecDeque<u32> = VecDeque::with_capacity(4);
    let mut acc: u64 = 0;
    for i in 0..5000u32 {
        big.push_back(i);
        if i % 2 == 0 {
            big.push_front(i);
        }
        if i % 3 == 0 {
            acc += big.pop_front().unwrap() as u64;
        }
    }
    while big.len() > 10 {
        acc += big.pop_back().unwrap() as u64;
    }
    println!("{} {:?} {}", acc, big, big[9]);

    // Nested and in structs.
    let mut grid: Vec<VecDeque<i32>> = vec![VecDeque::new(); 3];
    grid[1].push_back(5);
    grid[1].push_front(4);
    grid[1][0] += 10;
    let mid = grid[1][1];
    grid[2].push_back(mid);
    println!("{:?} {}", grid, grid[1].len());
    let mut by_name: HashMap<String, VecDeque<i32>> = HashMap::new();
    by_name.entry("a".to_string()).or_default().push_back(1);
    by_name.entry("a".to_string()).or_default().push_front(0);
    println!("{:?}", by_name["a"]);
    let mut line = Line::default();
    line.q.push_back(3);
    line.name.push('x');
    let copy = line.clone();
    line.q.push_front(2);
    println!("{:?} {:?} {}", line, copy, line == copy);
    let strs: VecDeque<String> = ["b", "a"].iter().map(|s| s.to_string()).collect();
    let mut sorted = strs.clone();
    sorted.make_contiguous().sort();
    println!("{:?} {:?} {}", strs, sorted, strs.iter().map(|s| s.len()).sum::<usize>());
    let mut it = strs.iter();
    while let Some(s) = it.next() {
        print!("{} ", s);
    }
    println!();
    let mut dq: VecDeque<(i32, char)> = VecDeque::new();
    dq.push_back((1, 'a'));
    dq.push_front((0, 'b'));
    let (n, c) = dq[1];
    println!("{} {} {:?}", n, c, dq.iter().map(|(n, _)| n).max());
    let mut empty: VecDeque<i32> = VecDeque::new();
    println!("{:?} {:?} {:?}", empty.pop_front(), empty.front(), empty.iter().next());
    empty.push_front(1);
    println!("{:?}", empty);
}
