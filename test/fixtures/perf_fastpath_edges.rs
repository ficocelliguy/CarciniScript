// Edge cases around the run-time fast paths: inline BigInt wrapping on parenthesized receivers,
// unit-variant and derived-`PartialEq` compares (`Ordering`, `Option<Dir>`, references),
// `match` on an indexed element, nested grid reads and writes of Copy structs, write-back
// `&mut` arguments to methods, and long ASCII / non-ASCII byte offsets.
use std::cmp::Ordering;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Dir {
    N,
    E,
    S,
    W,
}

#[derive(Debug, Clone, PartialEq)]
enum Op {
    Push(i64),
    Add,
    Jmp(usize),
    Halt,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct P {
    x: i32,
    y: i32,
}

#[derive(Debug, PartialEq)]
struct W<T> {
    v: T,
}

struct Acc {
    total: u64,
    v: Vec<i32>,
}

impl Acc {
    fn add(&mut self, x: &mut i32) {
        self.total += *x as u64;
        *x *= 2;
    }
}

fn inc(x: &mut i32) -> i32 {
    *x += 1;
    if *x > 100 {
        panic!("too big {}", x);
    }
    *x
}

fn main() {
    let a: u64 = 0xffff_ffff_ffff;
    let b: u64 = 12345;
    println!("{}", (a + b).wrapping_mul(0x100000001b3));
    println!("{}", (a ^ b).wrapping_add(u64::MAX));
    println!("{}", a.wrapping_sub(b + 1).wrapping_mul(a));
    let c: i64 = -5;
    println!("{} {}", (c - 1).wrapping_mul(-3), c.wrapping_sub(i64::MIN));
    let k = 7u32;
    println!("{}", a.wrapping_mul(k as u64));
    println!("{:?} {}", 3.cmp(&4), 3.cmp(&4) == Ordering::Less);
    println!("{}", "b".cmp("a") != Ordering::Greater);
    let ds = [Dir::N, Dir::E, Dir::S, Dir::W];
    let mut n = 0;
    for d in ds.iter() {
        if *d == Dir::S || Dir::W == *d {
            n += 1;
        }
        if d != &Dir::N {
            n += 10;
        }
    }
    let od = Some(Dir::E);
    println!("{} {} {}", n, od == Some(Dir::E), od.map(|d| d == Dir::E).unwrap_or(false));
    let prog = vec![Op::Push(3), Op::Push(4), Op::Add, Op::Jmp(5), Op::Halt, Op::Halt];
    let mut pc = 0;
    let mut st: Vec<i64> = vec![];
    loop {
        match &prog[pc] {
            Op::Push(v) => st.push(*v),
            Op::Add => {
                let x = st.pop().unwrap() + st.pop().unwrap();
                st.push(x)
            }
            Op::Jmp(t) => {
                pc = *t;
                continue;
            }
            Op::Halt => break,
        }
        pc += 1;
    }
    println!("{:?} {} {}", st, pc, prog[2] == Op::Add);
    let mut pts = vec![P { x: 1, y: 2 }, P { x: 3, y: 4 }];
    match pts[1] {
        P { x, .. } if x > 2 => pts[0].x = 9,
        _ => {}
    }
    let q = pts[0];
    pts[0].y = 50;
    println!("{:?} {:?} {}", q, pts, q == pts[0]);
    println!("{}", W { v: 1 } == W { v: 1 });
    let mut grid = vec![vec![0i32; 3]; 3];
    for i in 0..3 {
        for j in 0..3 {
            grid[i][j] += (i * 3 + j) as i32;
            grid[i][j] *= 2;
        }
    }
    let s: i32 = grid[1][2] + grid[2][0];
    let mut pg = vec![vec![P { x: 0, y: 0 }; 2]; 2];
    pg[1][0].x = 4;
    pg[1][1] = pg[1][0];
    pg[1][0].y += 3;
    println!("{:?} {} {:?}", grid, s, pg);
    let mut acc = Acc { total: 0, v: vec![1, 2, 3] };
    let mut loc = 5;
    acc.add(&mut loc);
    let mut v2 = vec![1, 2, 3];
    acc.add(&mut v2[1]);
    println!("{} {} {:?}", acc.total, loc, v2);
    let i = 1;
    inc(&mut acc.v[i]);
    println!("{:?}", acc.v);
    let txt = "héllo wörld, this is a longer string past thirty-two units";
    let ascii = "hello world, this is a longer string past thirty-two units";
    println!("{} {} {}", txt.len(), &txt[7..12], txt.is_char_boundary(2));
    println!("{} {} {}", ascii.as_bytes()[4], &ascii[0..5], ascii.is_char_boundary(58));
    println!("{:?} {:?}", txt.find("wörld"), ascii.find("world"));
    let bs = txt.as_bytes();
    println!("{} {}", bs[1], bs[2]);
    let mut grid2 = vec![vec![1u8; 2]; 2];
    grid2[0][1] = 7;
    println!("{:?}", grid2);
}
