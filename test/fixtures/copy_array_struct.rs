// `#[derive(Copy)]` on structs with `[T; N]` fields (named and tuple): copies are independent.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
struct Threads([u32; 4]);

#[derive(Debug, Clone, Copy)]
struct Batch {
    threads: Threads,
    delays: [f64; 2],
}

fn main() {
    let mut a = Threads([1, 2, 3, 4]);
    let b = a;
    a.0[0] = 9;
    println!("{a:?} {b:?} {}", a == b);
    let x = Batch { threads: b, delays: [0.5, 1.5] };
    let mut y = x;
    y.delays[1] = 7.0;
    y.threads.0[3] = 0;
    println!("{x:?}");
    println!("{y:?}");
    let total: u32 = x.threads.0.iter().sum();
    println!("{total}");
}
