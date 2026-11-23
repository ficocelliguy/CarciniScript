// Items nested in different functions with the same names: each function sees its own.
#[derive(Debug)]
struct Acc {
    total: i32,
}

fn one() -> i32 {
    #[derive(Debug, Clone, Copy)]
    enum State {
        A,
        B(i32),
    }
    struct Acc(i32);
    const LIMIT: i32 = 10;
    let s = State::B(3);
    println!("{:?} {:?}", s, State::A);
    assert!(matches!(s, State::B(_)));
    let a = Acc(LIMIT);
    match s {
        State::A => a.0,
        State::B(n) => n + a.0,
    }
}

fn two() -> String {
    #[derive(Debug, PartialEq)]
    enum State {
        X,
        Y,
    }
    #[derive(Debug)]
    struct Acc {
        v: String,
    }
    const LIMIT: usize = 2;
    let a = Acc { v: "hi".repeat(LIMIT) };
    let s: State = State::Y;
    println!("{:?} {:?}", a, s);
    match s {
        State::X => String::new(),
        State::Y => a.v,
    }
}

fn main() {
    println!("{} {}", one(), two());
    println!("{:?}", Acc { total: 3 });
}
