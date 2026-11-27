// Slice and `Vec` methods: `split_at_checked` / `split_at_mut_checked`, `partition_point`,
// `is_sorted_by` / `is_sorted_by_key`, `dedup_by`, `split(pred)` / `split_inclusive(pred)`,
// `split_last_mut`, `try_reserve_exact`, `first_chunk` / `first_chunk_mut`,
// `chunks_exact(..).remainder()` and `core::slice::from_ref`.
fn halves(v: &[u8]) -> Option<(&[u8], &[u8])> {
    v.split_at_checked(2)
}

fn bump_tail(v: &mut [u32]) {
    if let Some((a, b)) = v.split_at_mut_checked(1) {
        a[0] += 100;
        b[0] += 1;
    }
}

#[derive(Debug)]
struct P {
    x: i32,
}

fn bump_last(v: &mut [P]) {
    if let Some((last, rest)) = v.split_last_mut() {
        last.x *= 2;
        rest[0].x += 5;
    }
    if let Some((first, _)) = v.split_first_mut() {
        first.x += 1;
    }
}

fn head(v: &mut [u8]) -> Option<&mut [u8; 2]> {
    v.first_chunk_mut::<2>()
}

fn main() {
    let v: Vec<u8> = vec![1, 2, 3, 0, 4, 5, 0, 6];
    println!("{:?} {:?}", halves(&v), halves(&v[..1]));
    println!("{:?}", v.split_at_checked(9));
    let mut w = vec![1u32, 2, 3];
    bump_tail(&mut w);
    println!("{:?}", w);
    let mut ps = vec![P { x: 1 }, P { x: 2 }, P { x: 3 }];
    bump_last(&mut ps);
    println!("{:?}", ps);
    let mut e: Vec<u32> = vec![];
    println!("{}", e.split_at_mut_checked(1).is_none());
    e.push(1);

    let sorted = [1, 2, 4, 4, 7, 9];
    println!("{} {}", sorted.partition_point(|x| *x < 4), sorted.partition_point(|&x| x < 100));
    println!("{} {}", sorted.is_sorted_by(|a, b| a <= b), sorted.is_sorted_by(|a, b| a < b));
    println!("{} {}", v.is_sorted_by_key(|x| *x), ["a", "bb", "ccc"].is_sorted_by_key(|s| s.len()));

    let mut words = vec!["foo", "FOO", "bar", "Bar", "baz"];
    words.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
    println!("{:?}", words);

    let parts: Vec<&[u8]> = v.split(|b| *b == 0).collect();
    println!("{:?} {}", parts, v.split(|b| *b == 9).count());
    let incl: Vec<Vec<u8>> = v.split_inclusive(|b| *b == 0).map(|s| s.to_vec()).collect();
    println!("{:?}", incl);

    let mut r: Vec<u8> = Vec::new();
    r.try_reserve_exact(10).unwrap();
    println!("{} {}", r.is_empty(), r.try_reserve(4).is_ok());

    let mut m = vec![9u8, 8, 7];
    if let Some(h) = head(&mut m) {
        h[1] = 0;
    }
    println!("{:?} {:?} {:?}", m, m.first_chunk::<2>(), m.first_chunk::<4>());

    let data = [1u8, 2, 3, 4, 5, 6, 7];
    let ch = data.chunks_exact(3);
    println!("{:?}", ch.remainder());
    let sums: Vec<u32> = data.chunks_exact(2).map(|c| c.iter().map(|&x| x as u32).sum()).collect();
    println!("{:?} {:?}", sums, data.chunks_exact(2).remainder());

    let one = 42u16;
    let s: &[u16] = core::slice::from_ref(&one);
    println!("{:?} {}", s, std::slice::from_ref(&'x').len());
}
