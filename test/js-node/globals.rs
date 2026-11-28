// The `js` crate's browser-free globals under Node: Math, performance, crypto, base64, URI
// components, URLs, globals, `structured_clone`, timers, idle callbacks and `Promise::all` / `race`.
use std::collections::HashMap;

use js::{JSON, JsValue, Math, Promise, Url, UrlSearchParams, crypto, performance};
use js::{atob, btoa, set_timeout, structured_clone};

#[derive(Clone, Debug)]
struct Team {
    name: String,
    scores: Vec<u32>,
}

async fn after(ms: f64, value: i32) -> i32 {
    js::sleep(ms).await;
    value
}

pub async fn main() {
    // Math and performance
    let r = Math::random();
    println!("random in [0, 1): {}", (0.0..1.0).contains(&r));
    let t0 = performance::now();
    println!("now >= 0: {}, origin > 1.7e12: {}", t0 >= 0.0, performance::time_origin() > 1.7e12);
    performance::mark("a");

    // crypto
    let id = crypto::random_uuid();
    let dashes: Vec<usize> = id.char_indices().filter(|(_, c)| *c == '-').map(|(i, _)| i).collect();
    println!("uuid: {} chars, dashes at {:?}, version {}", id.len(), dashes, &id[14..15]);
    let bytes = crypto::random_bytes(16);
    println!("bytes: {}", bytes.len());
    let hash = crypto::digest("SHA-256", "abc".as_bytes()).await;
    let hex: String = hash.iter().map(|b| format!("{:02x}", b)).collect();
    println!("sha-256: {} bytes, {}", hash.len(), hex);
    let data: Vec<u8> = (0..10).collect();
    let part = crypto::digest("SHA-1", &data[2..5]).await;
    let whole = crypto::digest("SHA-1", &[2, 3, 4]).await;
    println!("sha-1 of a slice: {} bytes, same: {}", part.len(), part == whole);

    // base64 and URI components
    let b = btoa("hello, world");
    println!("{} -> {}", b, atob(&b));
    let e = js::encode_uri_component("a b&c=d/é");
    println!("{} -> {}", e, js::decode_uri_component(&e));

    // URLs
    let u = Url::parse("https://user:pw@example.com:8080/a/b?x=1&y=two&x=3#frag").unwrap();
    println!("{} {} {} {} {} {}", u.protocol, u.hostname, u.port, u.pathname, u.search, u.hash);
    println!("{} {} {}", u.origin, u.username, u.password);
    let q = u.search_params;
    println!("x={:?} all={:?} z={:?} has y: {} size {}", q.get("x"), q.get_all("x"), q.get("z"), q.has("y"), q.size);
    q.set("z", "a b");
    q.delete("y");
    q.sort();
    println!("{}", q);
    println!("{}", u);
    println!("{:?}", Url::parse("not a url").map(|u| u.href));
    let rel = Url::parse("../c?d", "https://example.com/a/b/").unwrap();
    println!("{}", rel.href);
    let p = UrlSearchParams::new("?a=1&b=%20&a=2");
    p.append("c", "&");
    println!("{} {:?}", p.to_string(), p.get_all("a"));

    // globals
    js::set_global("carcini_answer", 42);
    println!("global: {}", js::global("carcini_answer"));

    // structured_clone: deep copies with the argument's type
    let grid = vec![vec![1, 2], vec![3]];
    let mut copy = structured_clone(&grid);
    copy[0][0] = 9;
    println!("{:?} {:?}", grid, copy);
    let team = Team { name: "a".to_string(), scores: vec![1, 2] };
    let mut t2 = structured_clone(&team);
    t2.scores.push(3);
    t2.name.push('b');
    println!("{:?} {:?}", team, t2);
    let mut m: HashMap<String, Vec<i32>> = HashMap::new();
    m.insert("k".to_string(), vec![1]);
    let mut m2 = structured_clone(&m);
    m2.get_mut("k").unwrap().push(2);
    println!("{:?} {:?}", m["k"], m2["k"]);
    let raw = JSON::parse::<JsValue>("{\"a\":{\"b\":[1,2]}}").unwrap();
    let raw2 = structured_clone(&raw);
    js::set_global("carcini_raw", raw2);
    println!("{} {}", JSON::stringify(&raw), JSON::stringify(&js::global("carcini_raw")));

    // timers: a timeout fires during a later sleep; a cleared one never does
    set_timeout(|| println!("timeout fired"), 5.0);
    let cancelled = set_timeout(|| println!("cancelled timeout fired"), 10.0);
    js::clear_timeout(cancelled);
    js::queue_microtask(|| println!("microtask"));
    println!("before sleep");
    let idle = js::request_idle_callback(|d| println!("idle: timed out {}, time left {}", d.did_timeout, d.time_remaining() > 0.0), 20.0);
    let dropped = js::request_idle_callback(|_| println!("cancelled idle fired"));
    js::cancel_idle_callback(dropped);
    let _ = idle;
    js::sleep(30.0).await;
    println!("after sleep, {}ms+ passed: {}", 30, performance::now() - t0 >= 29.0);

    // Promise::all keeps the order; race takes the first to finish
    let all = Promise::all(vec![after(20.0, 1), after(5.0, 2), after(10.0, 3)]).await;
    println!("all: {:?}", all);
    let first = Promise::race(vec![after(30.0, 1), after(5.0, 2)]).await;
    println!("race: {}", first);
}
