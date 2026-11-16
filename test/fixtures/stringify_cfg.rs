// `stringify!` spacing as rustc prints tokens, and `cfg!` predicates.
fn main() {
    println!("[{}]", stringify!(a + b));
    println!("[{}]", stringify!(a + b));
    println!("[{}]", stringify!(Vec<i32>));
    println!("[{}]", stringify!(foo(x, y).bar));
    println!("[{}]", stringify!(x));
    println!("[{}]", stringify!("s" 'c' 1.5));
    println!("[{}]", stringify!(if a { b } else { c }));
    println!("[{}]", stringify!(a, b));
    println!("[{}]", stringify!(&mut x[0]));
    println!(
        "[{}]",
        stringify!(
            let y = 2;
        )
    );
    println!("[{}]", stringify!(a::b::<T>()));
    println!(
        "[{}]",
        stringify!(
            #[derive(Debug)]
            struct S;
        )
    );
    println!("[{} {}]", cfg!(debug_assertions), cfg!(not(test)));
}
