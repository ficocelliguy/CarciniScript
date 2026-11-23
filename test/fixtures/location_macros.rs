// `file!()`, `line!()` and `column!()`, alone, in `concat!` and from a `macro_rules!` body.
macro_rules! here {
    () => {
        concat!("line ", line!())
    };
}
macro_rules! log {
    ($m:expr) => {
        println!(concat!("{}:{}: ", $m), file!().ends_with(".rs"), line!())
    };
}

fn main() {
    println!("{} {}", line!(), column!());
    let l: u32 = line!();
    println!("{}", l + 1);
    println!(concat!("{}", ":", line!(), " {}"), file!().ends_with(".rs"), 5);
    let s = concat!(line!(), "-", column!());
    println!("{}", s);
    println!("{}", here!());
    log!("hi");
    let é = 1; let c = column!(); println!("{} {}", é, c);
}
