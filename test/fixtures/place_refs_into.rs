// `let r = &mut <element or field>` of a number, string, bool or `Option` kept in a variable (a
// slot over the place, made once: changing the index afterwards doesn't move it), and `.into()` on
// an `impl Into<H>` / `H: Into<Host>` parameter with no other hint of its target.
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
struct Host(String);

impl From<&str> for Host {
    fn from(s: &str) -> Self {
        Host(s.to_string())
    }
}

impl From<String> for Host {
    fn from(s: String) -> Self {
        Host(s)
    }
}

fn visit(start: impl Into<Host>) -> usize {
    let start = start.into();
    start.0.len()
}

fn named<H: Into<Host>>(h: H) -> usize {
    let h = h.into();
    h.0.len()
}

fn named_where<H>(h: H) -> String
where
    H: Into<Host>,
{
    let h = h.into();
    h.0.to_uppercase()
}

#[derive(Debug, Default)]
struct Stats {
    hits: u32,
    last: Option<String>,
}

impl Stats {
    fn record(&mut self, what: &str) {
        let hits = &mut self.hits;
        *hits += 1;
        let last = &mut self.last;
        *last = Some(what.to_string());
    }
}

fn main() {
    println!("{} {} {}", visit("home"), named(String::from("n00dles")), named_where("csec"));

    let mut m = HashMap::new();
    m.insert("a", vec![1, 2]);
    let first = &mut m.get_mut("a").unwrap()[0];
    *first += 10;
    println!("{:?}", m);

    let mut v = vec![String::from("x"), String::from("y")];
    let mut i = 0;
    let r = &mut v[i];
    i += 1;
    r.push('!');
    *r = format!("{r}?");
    println!("{v:?} {i}");

    let mut s = Stats::default();
    s.record("hack");
    s.record("grow");
    println!("{s:?}");

    let mut flags = [false; 3];
    let f = &mut flags[2];
    *f = !*f;
    println!("{flags:?}");
}
