use mac::{classify_tokens, config, empty_vec_of, sum};

fn main() {
    config!(name = "home", retries = 3, mode = fast(1, 2 + 3, "x"));
    println!("{}", sum!(1, 2, 3, 40));
    println!("{}", empty_vec_of!(u8));
    println!("{:?}", classify_tokens!(fn a::b -> "s" x ;));
}
