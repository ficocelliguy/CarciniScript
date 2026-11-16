// The bundled `smallvec`: `SmallVec<[T; N]>` is a `Vec<T>` (the inline buffer is an allocation
// detail), in types, turbofish and struct fields, nested, with `smallvec![..]`, `from_vec`,
// `from_slice`, `from_buf`, `into_vec`, and `Vec`'s methods.
use smallvec::{smallvec, SmallVec};

#[derive(Debug, Clone)]
struct Route {
    hops: SmallVec<[&'static str; 4]>,
    cost: u32,
}

fn neighbors(n: u32) -> SmallVec<[u32; 8]> {
    let mut out = SmallVec::new();
    for d in [1, 2, 3] {
        if n % d == 0 {
            out.push(n / d);
        }
    }
    out
}

fn main() {
    let mut v: SmallVec<[i32; 4]> = smallvec![3, 1, 2];
    v.push(5);
    v.extend([9, 7]);
    v.sort();
    println!("{:?} {} {:?}", v, v.len(), v.iter().max());

    let r = Route { hops: smallvec!["home", "n00dles"], cost: 4 };
    let mut r2 = r.clone();
    r2.hops.push("foodnstuff");
    r2.cost += 1;
    println!("{:?} {:?}", r, r2.hops);

    println!("{:?}", neighbors(12));
    let a = SmallVec::<[u8; 2]>::from_vec(vec![1, 2, 3]);
    let b: SmallVec<[u8; 2]> = SmallVec::from_slice(&[4, 5]);
    let c: SmallVec<[u8; 3]> = SmallVec::from_buf([6, 7, 8]);
    let all: Vec<u8> = a.into_iter().chain(b).chain(c.into_vec()).collect();
    println!("{all:?}");

    let grid: SmallVec<[SmallVec<[char; 3]>; 2]> = smallvec![smallvec!['a', 'b'], smallvec!['c']];
    println!("{:?} {}", grid, grid.iter().map(|row| row.len()).sum::<usize>());
    let zeros: smallvec::SmallVec<[f64; 2]> = smallvec![0.5; 3];
    println!("{:?}", zeros);
}
