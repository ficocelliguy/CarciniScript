// `OsString::push` takes a string or a path (`OsString` is a `String` here, whose `push` takes a
// `char`).
use std::ffi::{OsStr, OsString};
use std::path::Path;

fn add(s: &mut OsString, part: &str) {
    s.push(part);
}

// A struct field declared `OsString`, through a binding and through `self`.
struct Cmd {
    line: OsString,
}

impl Cmd {
    fn arg(&mut self, a: &str) {
        self.line.push(" ");
        self.line.push(a);
    }
}

fn main() {
    let mut c = Cmd { line: OsString::from("run") };
    c.arg("x.js");
    c.line.push(Path::new(" -t"));
    println!("{:?}", c.line);
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

// The elements of a `Vec<OsString>` (also through a field).
struct Args {
    list: Vec<OsString>,
}

fn elements(mut v: Vec<OsString>) -> Vec<OsString> {
    v[0].push("1");
    v.last_mut().unwrap().push("2");
    if let Some(f) = v.first_mut() {
        f.push("3");
    }
    for s in v.iter_mut() {
        s.push("!");
    }
    for s in &mut v {
        s.push(Path::new("?"));
    }
    let mut a = Args { list: v };
    a.list[1].push("4");
    a.list
}
