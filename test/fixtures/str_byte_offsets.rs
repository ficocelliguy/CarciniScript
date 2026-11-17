// UTF-8 byte offsets on ASCII and non-ASCII strings, short and long (the runtime caches a long
// string's UTF-8 length and bytes): `len`, `as_bytes()[i]`, `&s[a..b]`, `is_char_boundary`,
// `find`, byte loops, and lone surrogates from `char` arithmetic can't happen, so emoji and
// multi-byte letters cover the width classes.
fn count_digits(s: &str) -> usize {
    let mut i = 0;
    let mut n = 0;
    while i < s.len() {
        if s.as_bytes()[i].is_ascii_digit() {
            n += 1;
        }
        i += 1;
    }
    n
}

fn words(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < s.len() {
        let start = i;
        while i < s.len() && s.as_bytes()[i] != b' ' {
            i += 1;
        }
        if s.is_char_boundary(start) && s.is_char_boundary(i) {
            out.push(s[start..i].to_string());
        }
        i += 1;
    }
    out
}

fn boundaries(s: &str) -> String {
    let mut out = String::new();
    for i in 0..=s.len() + 1 {
        out.push(if s.is_char_boundary(i) { '1' } else { '0' });
    }
    out
}

fn main() {
    let short = "ab7 cd9";
    let long_ascii = "the quick brown fox jumps over 12 lazy dogs 345 times a day";
    let mixed = "héllo wörld ñ 12 日本語 34 🙂 end 5";
    let long_mixed = "αβγ δεζ ηθι κλμ νξο 123 πρσ τυφ χψω 456 🙂🙃 ωω 7 end of the line here";
    for s in [short, long_ascii, mixed, long_mixed] {
        println!("len={} chars={} digits={}", s.len(), s.chars().count(), count_digits(s));
        println!("{:?}", words(s));
        println!("{}", boundaries(s));
        println!("{:?} {:?} {:?}", s.find(' '), s.find("12"), s.rfind('e'));
        let bytes = s.as_bytes();
        let mut sum: u32 = 0;
        for &b in bytes {
            sum = sum.wrapping_mul(31).wrapping_add(b as u32);
        }
        println!("{} {} {}", bytes.len(), bytes[0], sum);
        let sp = s.find(' ').unwrap();
        println!("{} {}", &s[0..sp], &s[s.len() - 2..]);
    }
    // Slices at multi-byte boundaries, and the same string through several owners.
    let owned = String::from(long_mixed);
    let copy = owned.clone();
    println!("{} {} {}", &owned[0..2], &copy[2..6], &long_mixed[..=1]);
    println!("{} {}", &mixed[6..12], &mixed[14..16]);
    println!("{:?}", mixed.as_bytes()[6..12].to_vec());
    println!("{}", long_mixed.as_bytes()[12]);
    // A string that grows: the cached length follows the value, not the variable.
    let mut grown = String::from("abc");
    for i in 0..40 {
        grown.push(if i % 10 == 0 { 'é' } else { 'x' });
        if i % 13 == 0 {
            println!("{} {} {}", grown.len(), grown.as_bytes()[grown.len() - 1], grown.is_char_boundary(grown.len() - 1));
        }
    }
    println!("{} {}", grown.len(), &grown[grown.len() - 3..]);
    let empty = "";
    println!("{} {} {:?} {}", empty.len(), empty.is_char_boundary(0), empty.find('a'), empty.as_bytes().len());
}
