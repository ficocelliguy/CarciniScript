// Iterator adapter types written out: `iter::Cycle<iter::Chain<IntoIter<i32>, IntoIter<i32>>>`.
use std::iter;
use std::vec::IntoIter;

fn combine_vecs_explicit_return_type(
    v: Vec<i32>,
    u: Vec<i32>,
) -> iter::Cycle<iter::Chain<IntoIter<i32>, IntoIter<i32>>> {
    v.into_iter().chain(u.into_iter()).cycle()
}

fn pairs(a: Vec<i32>, b: Vec<char>) -> iter::Zip<IntoIter<i32>, IntoIter<char>> {
    a.into_iter().zip(b)
}

fn main() {
    let v1 = vec![1, 2, 3];
    let v3 = vec![4, 5];
    let mut v = combine_vecs_explicit_return_type(v1, v3);
    assert_eq!(Some(1), v.next());
    assert_eq!(Some(2), v.next());
    assert_eq!(Some(3), v.next());
    assert_eq!(Some(4), v.next());
    assert_eq!(Some(5), v.next());
    assert_eq!(Some(1), v.next());
    assert_eq!(Some(2), v.next());
    let taken: Vec<i32> = v.take(4).collect();
    println!("{:?}", taken);
    for (n, c) in pairs(vec![1, 2, 3], vec!['a', 'b']) {
        println!("{} {}", n, c);
    }
    println!("all done");
}
