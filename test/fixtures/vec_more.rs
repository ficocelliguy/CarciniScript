// More of std: Vec searching and rearranging (binary search, split_at, drain, split_off, append,
// fill, rotate, swap_remove), std::mem::{swap, replace, take}, integer bit counts and
// checked/saturating/wrapping arithmetic, splitn/rsplitn/rsplit, f64::total_cmp.
use std::mem;

#[derive(Debug, Default)]
struct Queue {
    items: Vec<u32>,
    name: String,
}

fn rotate_names(a: &mut Vec<String>, b: &mut Vec<String>) {
    mem::swap(a, b);
}

fn main() {
    let v = vec![1, 3, 5, 7, 9, 11];
    println!("{:?} {:?} {:?}", v.binary_search(&7), v.binary_search(&4), v.binary_search(&100));
    println!("{:?}", v.binary_search_by(|x| x.cmp(&1)));
    let pairs = vec![(1, 'a'), (4, 'b'), (9, 'c')];
    println!("{:?}", pairs.binary_search_by_key(&4, |&(k, _)| k));
    let (l, r) = v.split_at(2);
    println!("{:?} {:?} {} {}", l, r, v.starts_with(&[1, 3]), v.ends_with(&[11]));
    let mut w = v.clone();
    let drained: Vec<i32> = w.drain(1..3).collect();
    println!("{:?} {:?}", drained, w);
    let tail = w.split_off(2);
    println!("{:?} {:?}", w, tail);
    let mut other = vec![100, 200];
    w.append(&mut other);
    println!("{:?} {:?}", w, other);
    w.rotate_left(1);
    println!("{:?}", w);
    w.rotate_right(2);
    println!("{:?}", w);
    let gone = w.swap_remove(0);
    println!("{} {:?}", gone, w);
    let mut z = vec![0; 3];
    z.fill(7);
    println!("{:?} {:?} {}", z, [1, 2].repeat(2), v.is_sorted());

    let mut a = 1;
    let mut b = 2;
    mem::swap(&mut a, &mut b);
    let old = mem::replace(&mut a, 50);
    println!("{} {} {}", a, b, old);
    let mut q = Queue { items: vec![1, 2], name: "jobs".to_string() };
    let taken = mem::take(&mut q.items);
    println!("{:?} {:?}", taken, q);
    let mut names1 = vec!["x".to_string()];
    let mut names2 = vec!["y".to_string(), "z".to_string()];
    rotate_names(&mut names1, &mut names2);
    println!("{:?} {:?}", names1, names2);

    let x: u8 = 0b1011_0000;
    println!("{} {} {} {}", x.count_ones(), x.leading_zeros(), x.trailing_zeros(), 64u32.is_power_of_two());
    println!("{} {}", (-1i32).count_ones(), 0u16.leading_zeros());
    println!("{:?} {:?} {:?}", 100i8.checked_add(27), 100i8.checked_add(28), 10u32.checked_div(0));
    println!("{} {} {}", 200u8.saturating_mul(2), (-100i8).saturating_sub(100), 3u8.wrapping_sub(5));
    println!("{} {}", 2_000_000_000i32.wrapping_mul(3), 7u32.abs_diff(10));
    println!("{:?}", "a,b,c,d".splitn(3, ',').collect::<Vec<_>>());
    println!("{:?}", "a,b,c,d".rsplitn(2, ',').collect::<Vec<_>>());
    println!("{:?}", "a b c".rsplit(' ').collect::<Vec<_>>());
    let mut fs = vec![2.5, -1.0, f64::NAN, 0.0];
    fs.sort_by(|a, b| a.total_cmp(b));
    println!("{:?}", fs);
    println!("{:?} {:?}", char::from_u32(0x41), std::char::from_digit(11, 16));
    println!("{}", u64::from(7u32) + 1);
}
