// The bundled `log`'s `Level` by its full path, with no `use log`: as a type, a pattern and a
// value.
fn lvl(l: log::Level) -> u8 {
    match l {
        log::Level::Error => 1,
        log::Level::Warn => 2,
        _ => 3,
    }
}

fn main() {
    let all = [log::Level::Error, log::Level::Warn, log::Level::Info];
    for l in all {
        println!("{} {}", l, lvl(l));
    }
    println!("{}", log::Level::Debug < log::Level::Trace);
}
