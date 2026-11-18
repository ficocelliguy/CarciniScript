// `include!` of a file that defines and calls `macro_rules!` macros and includes another file
// next to itself (`data/more.in`, written `"more.in"`), and a spaced-out `macro_rules !`
// (generated code writes them so).
include!("data/tables.in");

macro_rules ! twice {
    ($e:expr) => {
        $e * 2
    };
}

fn main() {
    println!("{:?} {:?}", SQUARES, CUBES);
    println!("{}", twice!(SQUARES.iter().sum::<u32>()));
    println!("{}", tag!());
}
