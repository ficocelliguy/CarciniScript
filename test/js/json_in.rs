// `JSON::parse::<T>`: values back into typed structs, enums, collections and numbers, and the
// errors serde_json reports (wrong types, missing fields, unknown variants, range, syntax).
use js::{JSON, JsonError};
use serde::Deserialize;
use std::collections::{BTreeMap, HashSet};

#[derive(Debug, Deserialize)]
struct Point {
    x: f64,
    y: i32,
}

#[derive(Debug, Deserialize)]
struct Server {
    host: String,
    ram: u32,
    owned: bool,
    note: Option<String>,
    ports: Vec<u16>,
}

#[derive(Debug, Deserialize)]
struct Pair(u8, char);

#[derive(Debug, Deserialize)]
struct Meters(f64);

#[derive(Debug, Deserialize)]
enum Job {
    Idle,
    Hack(String),
    Grow { host: String, threads: u32 },
    Move(i32, i32),
}

#[derive(Debug, Deserialize)]
struct Tree {
    name: String,
    kids: Vec<Tree>,
}

macro_rules! show {
    ($e:expr) => {
        match $e {
            Ok(v) => println!("ok {:?}", v),
            Err(e) => println!("err {}", e),
        }
    };
}

fn main() {
    show!(JSON::parse::<Point>("{\"x\": 1, \"y\": -2}"));
    show!(JSON::parse::<Point>(" { \"y\" : 5 , \"x\" : 2.5e1 , \"extra\": [1, {\"a\": null}] } "));
    show!(JSON::parse::<Point>("[3.5, 4]"));
    show!(JSON::parse::<Server>("{\"host\":\"n00dles\",\"ram\":4,\"owned\":false,\"ports\":[22,80]}"));
    show!(JSON::parse::<Server>("{\"host\":\"home\",\"ram\":64,\"owned\":true,\"note\":\"mine\",\"ports\":[]}"));
    show!(JSON::parse::<Pair>("[7, \"z\"]"));
    show!(JSON::parse::<Meters>("12.5"));
    show!(JSON::parse::<Vec<Job>>("[\"Idle\", {\"Hack\": \"joes\"}, {\"Grow\": {\"host\": \"a\", \"threads\": 8}}, {\"Move\": [1, -1]}]"));
    show!(JSON::parse::<Tree>("{\"name\":\"r\",\"kids\":[{\"name\":\"k\",\"kids\":[]}]}"));
    show!(JSON::parse::<BTreeMap<String, Vec<i64>>>("{\"b\": [1], \"a\": [2, 3]}"));
    show!(JSON::parse::<(u64, i64)>("[9007199254740991, -9007199254740991]"));
    show!(JSON::parse::<u64>("18446744073709551616"));
    show!(JSON::parse::<i64>("-1").map(|x| x.wrapping_mul(9007199254740991)));
    show!(JSON::parse::<BTreeMap<u64, bool>>("{\"9007199254740991\": true, \"2\": false}"));
    show!(JSON::parse::<BTreeMap<u32, bool>>("{\"10\": true, \"2\": false}"));
    show!(JSON::parse::<(i32, String, Option<u8>)>("[1, \"s\", null]"));
    show!(JSON::parse::<String>("\"esc \\\" \\\\ \\n \\u00e9 \\ud83d\\ude00\""));
    show!(JSON::parse::<Option<Vec<f32>>>("[0.1, 2]"));
    let set: HashSet<i32> = JSON::parse("[3, 3, 1]").unwrap();
    println!("{}", set.len());
    // Errors.
    show!(JSON::parse::<Point>("{\"x\": 1}"));
    show!(JSON::parse::<Point>("{\"x\": \"one\", \"y\": 2}"));
    show!(JSON::parse::<Point>("{\"x\": 1, \"y\": 2.5}"));
    show!(JSON::parse::<Point>("{\"x\": 1, \"y\": 2, \"y\": 3}"));
    show!(JSON::parse::<Point>("[1]"));
    show!(JSON::parse::<Point>("7"));
    show!(JSON::parse::<u8>("300"));
    show!(JSON::parse::<u8>("-1"));
    show!(JSON::parse::<i32>("true"));
    show!(JSON::parse::<i32>("null"));
    show!(JSON::parse::<String>("5"));
    show!(JSON::parse::<bool>("\"yes\""));
    show!(JSON::parse::<char>("\"ab\""));
    show!(JSON::parse::<Vec<i32>>("{}"));
    show!(JSON::parse::<(i32, i32)>("[1]"));
    show!(JSON::parse::<Job>("\"Fly\""));
    show!(JSON::parse::<Job>("\"Hack\""));
    show!(JSON::parse::<Job>("{\"Grow\": {\"host\": \"a\"}}"));
    show!(JSON::parse::<Meters>("\"far\""));
    // Syntax errors.
    show!(JSON::parse::<Vec<i32>>("[1, 2"));
    show!(JSON::parse::<Vec<i32>>("[1, 2,]"));
    show!(JSON::parse::<Vec<i32>>("[1 2]"));
    show!(JSON::parse::<Point>("{\"x\" 1}"));
    show!(JSON::parse::<Point>("{x: 1}"));
    show!(JSON::parse::<i32>(""));
    show!(JSON::parse::<i32>("1 2"));
    show!(JSON::parse::<String>("\"open"));
    show!(JSON::parse::<bool>("tru"));
    show!(JSON::parse::<i32>("abc"));
    let e = JSON::parse::<Point>("{}").unwrap_err();
    println!("{} / {:?} / {}", e, e, e.message());
}
