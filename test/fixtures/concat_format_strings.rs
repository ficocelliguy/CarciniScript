// `concat!(..)` of literals as a format string, also from a macro's `$msg:literal`.
macro_rules! log {
    ($msg:literal $(, $p:expr)*) => {
        println!(concat!("[{}] ", $msg), "x" $(, $p)*)
    };
}

fn main() {
    let s = format!(concat!("{}", "-{}"), 1, 2);
    println!("{s}");
    log!("v={} w={}", 3, 4);
    print!(concat!("a", 'b', 1, true, "\n"));
    println!("{}", concat!("n", 2, 'x'));
}
