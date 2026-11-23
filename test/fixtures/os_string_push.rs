// `OsString::push` takes a string or a path (`OsString` is a `String` here, whose `push` takes a
// `char`).
use std::ffi::{OsStr, OsString};
use std::path::Path;

fn add(s: &mut OsString, part: &str) {
    s.push(part);
}

fn main() {
    let mut s = OsString::from("a");
    s.push("bc");
    s.push(Path::new("/d"));
    s.push(OsStr::new("e"));
    add(&mut s, "f");
    let mut t: OsString = OsStr::new("g").to_os_string();
    t.push(&s);
    let mut name = String::new();
    name.push('h');
    println!("{:?} {:?} {}", s, t, name);
}
