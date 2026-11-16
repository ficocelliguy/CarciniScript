// `#[cfg(..)]` on items, statements, fields, variants, match arms and expressions: only what's
// on survives, so alternative definitions of one name don't collide. (Target predicates are in
// test/js-node/target_cfg.rs: their answers differ from the native host's.)
#[cfg(any())]
fn pick() -> &'static str {
    "never"
}

#[cfg(all())]
fn pick() -> &'static str {
    "always"
}

#[cfg(not(any(windows, unix, target_family = "wasm")))]
fn platform() -> &'static str {
    "nowhere"
}

#[cfg(any(windows, unix, target_family = "wasm"))]
fn platform() -> &'static str {
    "somewhere"
}

#[cfg(test)]
mod tests {
    pub fn only_in_tests() {}
}

#[derive(Debug)]
struct Config {
    name: String,
    #[cfg(any())]
    secret: u32,
    #[cfg(not(any()))]
    level: u8,
}

#[derive(Debug)]
enum Mode {
    Fast,
    #[cfg(any())]
    Hidden,
    Slow,
}

fn describe(m: &Mode) -> &'static str {
    match m {
        Mode::Fast => "fast",
        #[cfg(any())]
        Mode::Hidden => "hidden",
        Mode::Slow => "slow",
    }
}

fn main() {
    println!("{} {}", pick(), platform());
    let c = Config {
        name: "x".into(),
        #[cfg(any())]
        secret: 1,
        #[cfg(all())]
        level: 3,
    };
    println!("{:?}", c);
    #[cfg(any())]
    println!("never printed");
    #[cfg(all(not(test), not(miri)))]
    println!("printed");
    let mut n = 0;
    #[cfg(any())]
    {
        n += 100;
    }
    #[cfg(not(any()))]
    {
        n += 1;
    }
    #[cfg(debug_assertions)]
    let mode = "debug";
    #[cfg(not(debug_assertions))]
    let mode = "release";
    println!("{} {} {} {}", n, describe(&Mode::Fast), describe(&Mode::Slow), mode.len() > 0);
    let v = [
        1,
        #[cfg(any())]
        2,
        3,
    ];
    println!("{:?} {}", v, cfg!(all(not(test), any(debug_assertions, not(debug_assertions)))));
}
