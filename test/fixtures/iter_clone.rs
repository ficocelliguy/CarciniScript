// Cloning an iterator (also as a derived `Clone` field): the copy goes on by itself.
use std::str::CharIndices;

#[derive(Clone)]
struct Lexer<'a> {
    chars: CharIndices<'a>,
}

fn main() {
    let mut it = vec![1, 2, 3].into_iter();
    it.next();
    let copy = it.clone();
    println!("{:?} {:?}", it.collect::<Vec<_>>(), copy.collect::<Vec<_>>());
    let mut l = Lexer { chars: "héllo".char_indices() };
    l.chars.next();
    let mut l2 = l.clone();
    l2.chars.next();
    println!("{:?} {:?}", l.chars.next(), l2.chars.next());
}
