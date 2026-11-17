// `s.as_bytes()[i]` past the end of a long non-ASCII string (the byte count, not the char count).
fn main() {
    let s = "αβγ δεζ ηθι κλμ νξο 123 πρσ τυφ χψω 456 the end of it all";
    let mut i = 0;
    let mut n = 0u32;
    while i < s.len() {
        n += s.as_bytes()[i] as u32;
        i += 10;
    }
    println!("{} {}", n, s.chars().count());
    println!("{}", s.as_bytes()[s.len()]);
}
