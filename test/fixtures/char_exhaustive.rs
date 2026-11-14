// Exhaustiveness over `char`: literal and range patterns (up to `char::MAX`) that cover every code
// point need no `_` arm.
#[derive(Debug)]
enum Kind {
    Digit,
    Lower,
    Upper,
    Other,
}

fn kind(c: char) -> Kind {
    match c {
        '0'..='9' => Kind::Digit,
        'a'..='z' => Kind::Lower,
        'A'..='Z' => Kind::Upper,
        '\0'..='/' | ':'..='@' | '['..='`' | '{'..=char::MAX => Kind::Other,
    }
}

fn main() {
    let ks: Vec<Kind> = "a1Z?é".chars().map(kind).collect();
    println!("{:?}", ks);
}
