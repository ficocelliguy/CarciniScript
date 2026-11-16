// Const generics: `const N: usize` on functions, structs, impls and methods.
use std::fmt;

const SIZE: usize = 3;

fn sum<const N: usize>(a: [i32; N]) -> i32 {
    let mut t = 0;
    for i in 0..N {
        t += a[i];
    }
    t
}

fn len_of<T, const N: usize>(_a: &[T; N]) -> usize {
    N
}

fn zeros<const N: usize>() -> [f64; N] {
    [0.0; N]
}

fn flag<const B: bool>() -> &'static str {
    if B { "on" } else { "off" }
}

fn letter<const C: char>() -> char {
    C
}

fn is_byte<const B: u8>(x: u8) -> bool {
    x == B
}

fn offset<const D: i32>(x: i32) -> i32 {
    x + D
}

#[derive(Debug, Clone, PartialEq)]
struct Matrix<const R: usize, const C: usize> {
    data: [[f64; C]; R],
}

impl<const R: usize, const C: usize> Matrix<R, C> {
    fn zero() -> Self {
        Matrix {
            data: [[0.0; C]; R],
        }
    }

    fn rows(&self) -> usize {
        R
    }

    fn cols(&self) -> usize {
        C
    }

    fn transpose(&self) -> Matrix<C, R> {
        let mut out = Matrix::<C, R>::zero();
        for i in 0..R {
            for j in 0..C {
                out.data[j][i] = self.data[i][j];
            }
        }
        out
    }

    fn mul<const K: usize>(&self, o: &Matrix<C, K>) -> Matrix<R, K> {
        let mut out = Matrix::<R, K>::zero();
        for i in 0..R {
            for j in 0..K {
                for k in 0..C {
                    out.data[i][j] += self.data[i][k] * o.data[k][j];
                }
            }
        }
        out
    }
}

struct Ring<T, const N: usize> {
    items: Vec<T>,
    head: usize,
}

impl<T: Clone + fmt::Debug, const N: usize> Ring<T, N> {
    fn new() -> Self {
        Ring {
            items: Vec::with_capacity(N),
            head: 0,
        }
    }

    fn capacity(&self) -> usize {
        N
    }

    fn push(&mut self, x: T) {
        if self.items.len() < N {
            self.items.push(x);
        } else {
            self.items[self.head] = x;
            self.head = (self.head + 1) % N;
        }
    }

    fn show(&self) -> String {
        format!("{:?} of {}", self.items, N)
    }
}

impl<T, const N: usize> fmt::Display for Ring<T, N> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Ring<{}>", N)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Tag<const ID: u32>;

impl<const ID: u32> Tag<ID> {
    fn id(&self) -> u32 {
        ID
    }
}

struct Buf<const N: usize = 4> {
    data: Vec<u8>,
}

impl<const N: usize> Buf<N> {
    fn new() -> Self {
        Buf { data: vec![0; N] }
    }
}

fn describe<const R: usize, const C: usize>(m: &Matrix<R, C>) -> String {
    format!("{}x{}", R, C)
}

fn main() {
    println!("{}", sum([1, 2, 3]));
    println!("{}", sum::<2>([10, 20]));
    println!("{}", zeros::<{ 128 / 8 + (1 << 2) }>().len());
    println!("{}", len_of(&["a", "b", "c", "d"]));
    let z: [f64; 4] = zeros::<4>();
    println!("{:?}", z);
    let w = zeros::<SIZE>();
    println!("{}", w.len());
    println!("{} {}", flag::<true>(), flag::<false>());
    println!("{}", letter::<'x'>());
    println!("{} {}", is_byte::<b'A'>(65), is_byte::<b' '>(65));
    println!("{}", offset::<-5>(12));

    let m = Matrix {
        data: [[1.0, 2.0, 3.0], [4.0, 5.0, 6.0]],
    };
    println!("{} {} {}", m.rows(), m.cols(), describe(&m));
    let t = m.transpose();
    println!("{:?} {}", t, describe(&t));
    let p = m.mul(&t);
    println!("{:?}", p);
    let c = p.clone();
    println!("{} {}", c == p, c.rows());
    let z: Matrix<2, 5> = Matrix::zero();
    println!("{} {}", z.rows(), z.cols());
    let id = Matrix::<1, 1>::zero();
    println!("{}", describe(&id));

    let mut r: Ring<i32, 3> = Ring::new();
    for i in 1..=5 {
        r.push(i);
    }
    println!("{} {} {}", r.show(), r.capacity(), r);
    let r2 = Ring::<&str, 2>::new();
    println!("{}", r2.capacity());

    let tag = Tag::<7>;
    println!("{} {:?}", tag.id(), tag);
    let b: Buf = Buf::new();
    println!("{}", b.data.len());
    let b8 = Buf::<8>::new();
    println!("{}", b8.data.len());
}
