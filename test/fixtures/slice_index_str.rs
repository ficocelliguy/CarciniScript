// `I: SliceIndex<str>` on a function's or method's parameter (inline or in `where`): `&s[i]` and
// `s.get(i)` take a part of the string by whatever range `i` is, and `I::Output` is `str`. Also
// `s.get(r)` with a range value.
use std::ops::Range;
use std::slice::SliceIndex;

pub struct Text {
    s: String,
}

impl Text {
    pub fn get<I: SliceIndex<str>>(&self, i: I) -> Option<&I::Output> {
        self.s.get(i)
    }
    pub fn part<I>(&self, i: I) -> &I::Output
    where
        I: SliceIndex<str>,
    {
        &self.s[i]
    }
}

fn sub<I>(s: &str, i: I) -> &I::Output
where
    I: SliceIndex<str>,
{
    &s[i]
}

fn shout<I: SliceIndex<str, Output = str>>(s: &str, i: I) -> String {
    s[i].to_uppercase()
}

fn by_range(s: &str, r: Range<usize>) -> Option<&str> {
    s.get(r)
}

fn main() {
    let t = Text { s: "hello world".to_string() };
    println!("{:?} {:?} {:?}", t.get(0..5), t.get(6..), t.get(..20));
    println!("[{}] [{}] [{}]", t.part(..5), t.part(6..=8), t.part(..));
    println!("{} {} {}", sub("carcini", 2..), sub("carcini", ..3), sub("carcini", 1..=2));
    println!("{}", shout("quiet please", 6..));
    println!("{:?} {:?}", by_range("héllo", 0..3), by_range("héllo", 0..2));
    let r = 1..4;
    println!("{:?}", "abcdef".get(r.clone()).map(|p| p.len()));
}
