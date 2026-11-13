// Vec<T>: construction, indexing, mutation, iteration, formatting, equality, cloning, and
// element types decided by inference from later statements.

fn sum(values: &[i64]) -> i64 {
    let mut total = 0;
    for v in values {
        total += *v;
    }
    total
}

fn double_all(values: &mut Vec<u32>) {
    for v in values.iter_mut() {
        *v *= 2;
    }
}

fn primes_below(limit: usize) -> Vec<usize> {
    let mut sieve = vec![true; limit];
    let mut primes = Vec::new();
    for n in 2..limit {
        if sieve[n] {
            primes.push(n);
            let mut m = n * n;
            while m < limit {
                sieve[m] = false;
                m += n;
            }
        }
    }
    primes
}

fn main() {
    // Element type comes from a later push: this is a Vec<u8>, so += wraps at 256.
    let mut bytes = Vec::new();
    bytes.push(250u8);
    bytes.push(3);
    bytes[0] += 10;
    println!("{:?} len={}", bytes, bytes.len());

    let mut nums: Vec<u32> = vec![3, 1, 4, 1, 5, 9, 2, 6];
    nums.sort();
    println!("sorted {:?}", nums);
    nums.reverse();
    nums.insert(2, 100);
    let removed = nums.remove(0);
    nums.swap(0, 1);
    println!("{:?} removed={} contains 5: {} contains 7: {}", nums, removed, nums.contains(&5), nums.contains(&7));
    double_all(&mut nums);
    println!("doubled {:?}", nums);
    nums.truncate(3);
    println!("truncated {:?} empty={}", nums, nums.is_empty());
    nums.clear();
    println!("cleared {:?} empty={}", nums, nums.is_empty());

    let signed: Vec<i64> = vec![-5, 10, 2_000_000_000, 2_000_000_000];
    println!("sum = {}", sum(&signed));

    let primes = primes_below(60);
    println!("{} primes below 60: {:?}", primes.len(), primes);

    // Nested Vecs: `vec![row; n]` must clone the row, not share it.
    let mut grid = vec![vec![0i32; 4]; 3];
    for r in 0..3 {
        for c in 0..4 {
            grid[r][c] = (r * 4 + c) as i32;
        }
    }
    grid[1][2] = -1;
    println!("{:?}", grid);
    let copy = grid.clone();
    grid[0][0] = 99;
    println!("copy unchanged: {:?} equal now: {}", copy[0], copy == grid);

    let mut words = vec![String::from("pear"), "apple".to_string(), "fig".to_string()];
    words.sort();
    words.push("kiwi".to_string());
    println!("{:?} -> {}", words, words.join(", "));
    let mut total_len = 0;
    for w in &words {
        total_len += w.len();
    }
    println!("total length {}", total_len);

    let floats = vec![1.0, 2.5, -0.0];
    println!("{:?} {:?}", floats, vec!['a', '\n']);

    let mut a = vec![1, 2, 3];
    let b = [1, 2, 3];
    println!("a == b: {} a != b: {}", a == b.to_vec(), a != b.to_vec());
    a.extend_from_slice(&b);
    assert_eq!(a, vec![1, 2, 3, 1, 2, 3]);
    let mut squares = [0u64; 5];
    for i in 0..squares.len() {
        squares[i] = (i * i) as u64;
    }
    println!("{:?} {}", squares, squares.len());

    let mut counts = vec![0; 3];
    for x in [0usize, 2, 2, 1, 2] {
        counts[x] += 1;
    }
    println!("counts {:?}", counts);

    let idx = counts.len() + 2;
    println!("about to index {} of {}", idx, counts.len());
    println!("{}", counts[idx]);
}
