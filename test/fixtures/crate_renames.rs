// Renamed crates and renamed imports of a crate's types: `extern crate itertools as it;` (then
// `it::` paths and `use it::..`), `extern crate alloc as core_alloc;` (a std crate under another
// name), `extern crate std as core;`, and `use serde_json::Value as Json` / `use
// it::Itertools as Iters` (a bundled crate's type or trait under a new name).
extern crate alloc as core_alloc;
extern crate itertools as it;
extern crate std as core;

use core_alloc::collections::BTreeMap;
use core_alloc::string::String as Text;
use it::Itertools as Iters;
use serde_json::Value as Json;

fn label(n: &Json) -> Text {
    match n {
        Json::Number(x) => format!("num {x}"),
        Json::String(s) => format!("str {s}"),
        _ => "other".to_string(),
    }
}

struct Joiner;

impl Joiner {
    fn join(&self, v: &[i32]) -> Text {
        v.iter().sorted().join("-")
    }
}

fn main() {
    let v = vec![3, 1, 2];
    println!("{}", Joiner.join(&v));
    println!("{:?}", v.iter().map(|x| x * 2).collect_vec());
    let mut m: BTreeMap<i32, Text> = core_alloc::collections::BTreeMap::new();
    m.insert(2, "two".to_string());
    m.insert(1, "one".to_string());
    println!("{:?}", m);
    let v: core_alloc::vec::Vec<u8> = core_alloc::vec![1, 2];
    println!("{:?} {}", v, core::cmp::max(4, 9));
    let j: Json = serde_json::from_str(r#"[1, "x", true]"#).unwrap();
    if let Json::Array(items) = &j {
        for i in items {
            println!("{}", label(i));
        }
    }
    let p = Json::Bool(false);
    println!("{} {}", p, Json::Null.is_null());
}
