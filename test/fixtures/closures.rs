// Closures: captures (read and write), `impl Fn` parameters and returns, `dyn Fn`, `fn` pointers,
// block bodies, `?` inside a closure, pattern parameters and passing named functions.
fn apply<F: Fn(i32) -> i32>(f: F, x: i32) -> i32 {
    f(x)
}

fn apply_twice(f: impl Fn(i32) -> i32, x: i32) -> i32 {
    f(f(x))
}

fn make_adder(n: i32) -> impl Fn(i32) -> i32 {
    move |x| x + n
}

fn compose(f: &dyn Fn(i32) -> i32, g: fn(i32) -> i32, x: i32) -> i32 {
    g(f(x))
}

fn double(x: i32) -> i32 {
    x * 2
}

fn call_mut(mut f: impl FnMut()) {
    f();
    f();
}

fn first_even_halved(v: &[i32]) -> Option<i32> {
    let half = |x: i32| -> Option<i32> {
        let e = if x % 2 == 0 { Some(x) } else { None }?;
        Some(e / 2)
    };
    for &x in v {
        if let Some(h) = half(x) {
            return Some(h);
        }
    }
    None
}

fn main() {
    let k = 10;
    let add_k = |x| x + k;
    println!("{}", add_k(5));
    println!("{}", apply(add_k, 1));
    println!("{}", apply(|x| x * x, 7));
    println!("{}", apply_twice(|x| x - 3, 20));
    let add5 = make_adder(5);
    println!("{} {}", add5(1), make_adder(-2)(3));
    println!("{}", compose(&|x| x + 1, double, 4));
    println!("{}", apply(double, 21));

    let mut count = 0;
    let mut bump = || count += 1;
    bump();
    bump();
    call_mut(|| count += 10);
    println!("count {}", count);

    let describe = |n: i32| {
        if n < 0 {
            return "negative";
        }
        match n {
            0 => "zero",
            _ => "positive",
        }
    };
    println!("{} {} {}", describe(-4), describe(0), describe(9));

    let swap = |(a, b): (i32, char)| (b, a);
    println!("{:?}", swap((1, 'x')));

    println!("{:?} {:?}", first_even_halved(&[3, 5, 8, 10]), first_even_halved(&[1, 3]));

    let mut log: Vec<String> = Vec::new();
    let mut record = |s: &str| log.push(s.to_uppercase());
    record("a");
    record("b");
    println!("{:?}", log);
}
