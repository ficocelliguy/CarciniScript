// `f32` values print their own shortest digits (`0.1`, not the double's `0.10000000149011612`),
// with `{}`, `{:?}`, `to_string()`, and inside collections and structs.
#[derive(Debug)]
struct Stats {
    rate: f32,
    history: Vec<f32>,
}

fn main() {
    let x: f32 = 0.1;
    let v: Vec<f32> = vec![0.1, 2.0, 1.0 / 3.0, 16777216.0, 1e-5, 3.4e38];
    let o: Option<f32> = Some(0.3);
    println!("{} {:?} {}", x, x, x.to_string());
    println!("{:?}", v);
    println!("{:?} {}", o, v.iter().map(|f| f.to_string()).collect::<Vec<_>>().join(" "));
    println!("{:?}", Stats { rate: 0.7, history: vec![1.1, 2.2] });
    let sum: f32 = v.iter().take(3).sum();
    println!("{} {}", sum, sum * 3.0);
}
