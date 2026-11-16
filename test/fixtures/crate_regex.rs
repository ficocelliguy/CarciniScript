// The bundled regex: matching, groups (numbered and named), iteration, replacing (`$1`,
// `${name}`, closures, `NoExpand`), splitting, flags, Unicode and byte offsets, errors.
use regex::{Captures, NoExpand, Regex};
use std::sync::LazyLock;

static SERVER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(?P<name>[a-z0-9-]+)\s+(?P<ram>\d+)GB$").unwrap());

fn parse(line: &str) -> Option<(String, u32)> {
    let caps = SERVER.captures(line)?;
    Some((caps["name"].to_string(), caps["ram"].parse().ok()?))
}

fn main() {
    let re = Regex::new(r"(\d{4})-(\d{2})-(\d{2})").unwrap();
    let text = "Dates: 2024-01-15, 2025-12-31 and 1999-07-04.";
    println!("{}", re.is_match(text));
    let m = re.find(text).unwrap();
    println!("{} {} {:?} {:?}", m.start(), m.end(), m.as_str(), m);
    for caps in re.captures_iter(text) {
        println!("{}/{}/{} ({})", &caps[3], &caps[2], &caps[1], caps.len());
    }
    let years: Vec<&str> = re.find_iter(text).map(|m| &m.as_str()[..4]).collect();
    println!("{:?}", years);
    println!("{}", re.replace_all(text, "$3.$2.$1"));
    println!("{}", re.replace(text, "<${1}>"));
    println!("{}", re.replacen(text, 2, "X"));
    println!("{}", re.replace_all(text, NoExpand("$1")));
    let bumped = re.replace_all(text, |c: &Captures| {
        let y: u32 = c[1].parse().unwrap();
        format!("{}-{}-{}", y + 1, &c[2], &c[3])
    });
    println!("{}", bumped);

    println!("{:?}", parse("n00dles 4GB"));
    println!("{:?}", parse("bad line"));

    let named = Regex::new(r"(?<key>\w+)=(?<val>\w*)").unwrap();
    let c = named.captures("a=1 bee=two").unwrap();
    println!("{:?} {:?}", c.name("key").map(|m| m.as_str()), c.get(2).map(|m| (m.start(), m.end())));
    println!("{:?}", c);
    let names: Vec<Option<&str>> = named.capture_names().collect();
    println!("{:?} {}", names, named.captures_len());
    println!("{}", named.replace_all("a=1 bee=two", "$val:$key"));

    let ws = Regex::new(r"\s*[,;]\s*").unwrap();
    let parts: Vec<&str> = ws.split("a, b;c ,d").collect();
    println!("{:?}", parts);
    let parts: Vec<&str> = ws.splitn("a, b;c ,d", 2).collect();
    println!("{:?}", parts);

    let ci = Regex::new(r"(?i)hack|grow").unwrap();
    let hits: Vec<&str> = ci.find_iter("HACK then Grow then weaken").map(|m| m.as_str()).collect();
    println!("{:?}", hits);
    let ml = Regex::new(r"(?m)^\w+$").unwrap();
    println!("{}", ml.find_iter("one\ntwo three\nfour").count());

    // Unicode: `\w` and `\d` are Unicode, offsets are bytes.
    let word = Regex::new(r"\w+").unwrap();
    for m in word.find_iter("héllo wörld 日本") {
        println!("{} {}..{}", m.as_str(), m.start(), m.end());
    }
    println!("{:?}", Regex::new(r"\d+").unwrap().find("abc ٣٤ 12"));

    // Empty matches.
    let empty = Regex::new(r"x*").unwrap();
    let spans: Vec<(usize, usize)> = empty.find_iter("axxb").map(|m| (m.start(), m.end())).collect();
    println!("{:?}", spans);
    println!("{}", empty.replace_all("axxb", "-"));

    println!("{} {:?}", re, re);
    println!("{}", regex::escape("1+1=2? (yes)"));
    println!("{:?}", Regex::new(r"[[:alpha:]]+").unwrap().find("123abc456"));
    println!("{}", Regex::new(r"a\.b").unwrap().is_match("a.b"));
    println!("{}", Regex::new("(unclosed").is_err());
    let bad: Result<Regex, regex::Error> = Regex::new("a)");
    println!("{}", bad.is_err());
}
