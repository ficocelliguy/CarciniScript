// Comments inside `#[derive(..)]`.
#[derive(Debug, Clone /* , Copy */, PartialEq // compared below
)]
struct P {
    x: u8,
}

#[derive(
    Debug, // printed
    Default,
)]
enum Mode {
    #[default]
    Idle,
    #[allow(dead_code)]
    Busy,
}

fn main() {
    let p = P { x: 1 };
    println!("{:?} {} {:?}", p.clone(), p == p.clone(), Mode::default());
}
