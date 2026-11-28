// An impl for a concrete box next to one for the type it boxes, one type in JS: `Box<str>` and
// `String` (clap's `ValueParserFactory`), `Box<[T]>` and `[T]` (wasm-bindgen's describe), with
// associated functions only. A path naming the box, or a type argument written as one, reaches
// the box's impl.
use std::path::PathBuf;

pub trait ValueParserFactory {
    type Parser;
    fn value_parser() -> Self::Parser;
}
impl ValueParserFactory for String {
    type Parser = u32;
    fn value_parser() -> u32 { 1 }
}
impl ValueParserFactory for Box<str> {
    type Parser = u32;
    fn value_parser() -> u32 { 2 }
}
impl ValueParserFactory for PathBuf {
    type Parser = u32;
    fn value_parser() -> u32 { 3 }
}
pub trait Describe {
    fn describe() -> &'static str;
}
impl<T> Describe for [T] {
    fn describe() -> &'static str { "slice" }
}
impl<T> Describe for Box<[T]> {
    fn describe() -> &'static str { "boxed slice" }
}
fn parser<T: ValueParserFactory<Parser = u32>>() -> u32 { T::value_parser() }
fn main() {
    println!(
        "{} {} {}",
        <String as ValueParserFactory>::value_parser(),
        <Box<str> as ValueParserFactory>::value_parser(),
        <PathBuf as ValueParserFactory>::value_parser()
    );
    println!("{} {} {}", parser::<String>(), parser::<Box<str>>(), parser::<PathBuf>());
    println!("{} {}", <[u8] as Describe>::describe(), <Box<[u8]> as Describe>::describe());
}
