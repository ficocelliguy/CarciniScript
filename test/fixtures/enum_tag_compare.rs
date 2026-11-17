// `==` / `!=` against unit variants (a tag compare), between enum values, on structs with a
// derived `PartialEq` (their own `$eq`) and a hand-written one; `Self::Variant` in impls;
// `match` over an indexed enum element with bindings, and through `&mut`.
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
    Jnz(usize),
    Label { name: String },
    Halt,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct P {
    x: i32,
    y: i32,
}

#[derive(Debug, Clone, Copy)]
struct Approx(f64);

impl PartialEq for Approx {
    fn eq(&self, o: &Approx) -> bool {
        (self.0 - o.0).abs() < 0.5
    }
}

impl Dir {
    fn turn(self) -> Dir {
        if self == Self::W {
            Dir::N
        } else if Self::N == self {
            Dir::E
        } else if self != Dir::E {
            Dir::W
        } else {
            Dir::S
        }
    }
}

fn run(prog: &[Op]) -> i64 {
    let mut stack: Vec<i64> = Vec::new();
    let mut pc = 0;
    let mut acc = 0;
    loop {
        match &prog[pc] {
            Op::Push(v) => stack.push(*v),
            Op::Add => {
                let a = stack.pop().unwrap();
                let b = stack.pop().unwrap();
                stack.push(a + b);
            }
            Op::Jnz(t) => {
                let top = stack.pop().unwrap();
                acc += top;
                if top != 0 {
                    stack.push(top - 1);
                    pc = *t;
                    continue;
                }
            }
            Op::Label { name } if name.starts_with('x') => acc += 1000,
            Op::Label { .. } => acc += 1,
            Op::Halt => break,
        }
        pc += 1;
    }
    acc
}

fn main() {
    let mut d = Dir::N;
    let mut s = 0;
    for _ in 0..10 {
        d = d.turn();
        if d == Dir::S {
            s += 1;
        }
        if Dir::W != d {
            s += 10;
        }
    }
    println!("{} {:?} {}", s, d, (d == Dir::N) as u8);
    let prog = vec![Op::Push(3), Op::Label { name: "x".into() }, Op::Label { name: "y".into() }, Op::Jnz(1), Op::Push(1), Op::Push(2), Op::Add, Op::Halt];
    println!("{}", run(&prog));
    println!("{} {} {}", prog[1] == Op::Add, prog[6] == Op::Halt, prog[0] != Op::Push(3));
    println!("{} {}", prog[0] == prog[3], prog[1] == Op::Label { name: "x".into() });
    let dirs = [Dir::N, Dir::S, Dir::S];
    println!("{} {}", dirs[1] == Dir::S, dirs.iter().filter(|&&x| x == Dir::S).count());
    let mut p = P { x: 1, y: 2 };
    let q = p;
    p.x += 1;
    println!("{} {} {}", p == q, p != q, P { x: 2, y: 2 } == p);
    println!("{} {}", Approx(1.0) == Approx(1.2), Approx(1.0) != Approx(2.0));
    // A match through `&mut` on an element: bindings write the element in place.
    let mut ops = vec![Op::Push(1), Op::Jnz(0), Op::Halt];
    for i in 0..ops.len() {
        match &mut ops[i] {
            Op::Push(v) => *v += 10,
            Op::Jnz(t) => *t += 2,
            _ => {}
        }
    }
    println!("{:?}", ops);
    let mut grid = vec![vec![Dir::N; 2]; 2];
    match grid[1][0] {
        Dir::N => grid[1][0] = Dir::S,
        _ => {}
    }
    println!("{:?} {}", grid, grid[1][0] == Dir::S);
    // Options of enums keep the generic compare.
    let maybe: Option<Dir> = Some(Dir::E);
    println!("{} {}", maybe == Some(Dir::E), maybe == None);
}
