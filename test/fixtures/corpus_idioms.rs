// Idioms from the Bitburner corpus: an `impl Into<Host>` parameter given a `&str` or a `String`
// (`Crawl::from(ns, "home")`), `(*h).clone()` on the `&&Host` a `filter` closure gets,
// `&mut String::new()` as an argument, `for (slot, v) in opts.iter_mut().zip(..)` writing
// `*slot = Some(..)`, and `step_by` over a huge range.
use std::ops::Deref;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct Host(String);

impl Deref for Host {
    type Target = str;
    fn deref(&self) -> &str {
        &self.0
    }
}

impl From<&str> for Host {
    fn from(s: &str) -> Self {
        Host(s.to_owned())
    }
}

impl From<String> for Host {
    fn from(s: String) -> Self {
        Host(s)
    }
}

struct Crawl {
    start: Host,
}

impl Crawl {
    fn from(start: impl Into<Host>) -> Self {
        Crawl { start: start.into() }
    }
    fn len(&self) -> usize {
        self.start.len()
    }
}

fn greet(h: impl Into<Host>) -> String {
    let h: Host = h.into();
    format!("hello {}", &*h)
}

fn named<H: Into<Host>>(h: H) -> Host {
    h.into()
}

fn digits(expr: &mut String, n: u32, out: &mut Vec<String>) {
    if n == 0 {
        out.push(expr.clone());
        return;
    }
    let len = expr.len();
    expr.push_str(&n.to_string());
    digits(expr, n - 1, out);
    expr.truncate(len);
}

fn main() {
    println!("{} {}", Crawl::from("home").len(), Crawl::from(String::from("n00dles")).len());
    println!("{} {}", greet("joe"), greet(Host("x".into())));
    println!("{:?} {:?}", named("a"), named(String::from("b")));

    let hosts = vec![Host("home".into()), Host("zer0".into()), Host("n00dles".into())];
    let mut failed: Vec<(Host, String)> = Vec::new();
    let kept: Vec<&Host> = hosts
        .iter()
        .filter(|h| {
            if h.len() > 4 {
                failed.push(((*h).clone(), format!("{} is long", h.0)));
                false
            } else {
                true
            }
        })
        .collect();
    println!("{:?} {:?}", kept, failed);

    let mut out = Vec::new();
    digits(&mut String::new(), 3, &mut out);
    println!("{:?}", out);

    let mut opts: Vec<Option<u32>> = vec![None, Some(2), None];
    for (slot, v) in opts.iter_mut().zip([10, 20, 30].iter()) {
        if slot.is_none() {
            *slot = Some(*v);
        }
    }
    println!("{:?}", opts);
    let mut nums = vec![1, 2, 3];
    for (x, d) in nums.iter_mut().zip(vec![10, 20]) {
        *x += d;
    }
    println!("{:?}", nums);

    let n = (0..2_000_000_000u64).step_by(500_000_000).count();
    let last = (0..=1_000_000_000_000i64).step_by(250_000_000_000).last();
    println!("{} {:?}", n, last);
}
