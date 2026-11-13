// Iterator adapters and consumers: map/filter chains, laziness, sums and products, min/max,
// searching, folding, char ranges, string splitting, windows and chunks, and collecting into
// Vec, String, HashSet and HashMap.
use std::collections::{HashMap, HashSet};

fn main() {
    let v = vec![3, 1, 4, 1, 5, 9, 2, 6];
    let doubled: Vec<i32> = v.iter().map(|x| x * 2).collect();
    println!("{:?}", doubled);
    let evens = v.iter().filter(|&&x| x % 2 == 0).count();
    println!("evens {}", evens);
    let total: i32 = v.iter().sum();
    let prod: i64 = v.iter().map(|&x| x as i64).product();
    println!("{} {}", total, prod);
    println!("{:?} {:?}", v.iter().max(), v.iter().min());
    println!("{:?}", v.iter().position(|&x| x == 5));
    println!("{:?} {:?}", v.iter().find(|&&x| x > 4), v.iter().find(|&&x| x > 100));
    println!("{} {}", v.iter().any(|&x| x == 9), v.iter().all(|&x| x > 0));
    println!("{}", v.iter().fold(0, |acc, x| acc * 10 + x));
    println!("{:?}", v.iter().copied().reduce(|a, b| a.max(b)));
    println!("{:?}", v.iter().skip(2).step_by(2).collect::<Vec<_>>());
    println!("{:?}", v.iter().take_while(|&&x| x < 5).collect::<Vec<_>>());
    println!("{:?}", v.iter().skip_while(|&&x| x < 5).collect::<Vec<_>>());
    println!("{:?} {:?}", v.iter().last(), v.iter().nth(3));
    println!("{:?}", v.iter().rev().take(3).collect::<Vec<&i32>>());

    // Lazy: only as many items as `take` asks for are inspected.
    let mut seen = Vec::new();
    let firsts: Vec<i32> = (1..).inspect(|&x| seen.push(x)).filter(|x| x % 3 == 0).take(2).collect();
    println!("{:?} {:?}", firsts, seen);

    let empty: Vec<f64> = Vec::new();
    let s: f64 = empty.iter().sum();
    println!("{:?} {}", s, (1..=5).map(|x| x as f64 * 0.5).sum::<f64>());
    println!("{}", (1..=10).filter(|n| n % 2 == 1).map(|n| n * n).sum::<i32>());

    for (i, c) in "héllo".chars().enumerate() {
        if i % 2 == 0 {
            print!("{}{} ", i, c);
        }
    }
    println!();
    for c in 'a'..='e' {
        print!("{}", c);
    }
    println!();
    let alphabet: String = ('a'..'g').rev().collect();
    println!("{}", alphabet);
    let reversed: String = "stressed".chars().rev().collect();
    println!("{}", reversed);
    let upper: String = "shout".chars().map(|c| c.to_ascii_uppercase()).collect();
    println!("{}", upper);

    let words: Vec<&str> = "the quick  brown fox".split_whitespace().collect();
    println!("{:?}", words);
    let parts: Vec<String> = "a,b,,c".split(',').map(|s| s.to_string()).collect();
    println!("{:?}", parts);
    let text = "line one\nline two\r\n\nlast\n";
    println!("{:?}", text.lines().collect::<Vec<_>>());
    let lens: Vec<usize> = text.lines().map(|l| l.len()).collect();
    println!("{:?}", lens);

    let nums = vec![1, 2, 3, 4, 5];
    let sums: Vec<i32> = nums.windows(2).map(|w| w[0] + w[1]).collect();
    println!("{:?}", sums);
    for chunk in nums.chunks(2) {
        print!("{:?} ", chunk);
    }
    println!();

    let pairs: Vec<(i32, char)> = nums.iter().copied().zip("abc".chars()).collect();
    println!("{:?}", pairs);
    let joined: Vec<i32> = nums.iter().copied().chain(vec![10, 20]).collect();
    println!("{:?}", joined);
    let nested = vec![vec![1, 2], vec![], vec![3]];
    let flat: Vec<i32> = nested.iter().flatten().copied().collect();
    println!("{:?}", flat);
    let expanded: Vec<i32> = (1..4).flat_map(|n| vec![n; n as usize]).collect();
    println!("{:?}", expanded);
    let first_big = nums.iter().find_map(|&x| if x > 2 { Some(x * 100) } else { None });
    println!("{:?}", first_big);
    let maybe: Vec<Option<i32>> = vec![Some(1), None, Some(3)];
    println!("{:?}", maybe.iter().flatten().collect::<Vec<_>>());

    let set: HashSet<i32> = v.iter().map(|x| x % 3).collect();
    let mut set_items: Vec<_> = set.into_iter().collect();
    set_items.sort();
    println!("{:?}", set_items);
    let index: HashMap<char, usize> = "abc".chars().enumerate().map(|(i, c)| (c, i)).collect();
    let mut entries: Vec<(char, usize)> = index.into_iter().collect();
    entries.sort();
    println!("{:?}", entries);

    let fruit = vec!["pear", "fig", "banana", "kiwi"];
    let longest = fruit.iter().max_by_key(|s| s.len());
    let shortest = fruit.iter().min_by_key(|s| s.len());
    println!("{:?} {:?}", longest, shortest);
    let mut acc = 0;
    (1..=4).for_each(|x| acc += x);
    println!("{}", acc);
    for (a, b) in nums.iter().zip(nums.iter().skip(1)) {
        print!("{}{} ", a, b);
    }
    println!();
}
