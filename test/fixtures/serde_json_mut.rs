use serde_json::{json, Value};

fn main() {
    // IndexMut: assign through a key, inserting into an object (and turning Null into one).
    let mut v = json!({"name": "n00dles", "ram": 4});
    v["ram"] = json!(8);
    v["tags"] = json!(["cheap"]);
    v["tags"][0] = Value::from("easy");
    v["nested"]["deep"] = json!(true);
    println!("{}", v);
    let mut n = Value::Null;
    n["a"] = json!(1);
    println!("{}", n);
    if let Some(x) = v["ram"].as_i64() {
        v["ram"] = json!(x * 2);
    }
    println!("{}", v["ram"]);

    // PartialEq with numbers, strings and bools, both ways round.
    println!("{} {} {}", v["ram"] == 16, v["ram"] == 16.0, v["ram"] == 3u8);
    println!("{} {}", v["name"] == "n00dles", v["name"] == String::from("x"));
    println!("{} {}", v["nested"]["deep"] == true, 16 == v["ram"]);
    println!("{} {}", "n00dles" == v["name"], v["missing"] == 0);
    let s: &str = "n00dles";
    println!("{}", v["name"] == s);

    // to_vec / from_slice / to_writer / from_reader.
    let bytes = serde_json::to_vec(&v).unwrap();
    println!("{} bytes", bytes.len());
    let back: Value = serde_json::from_slice(&bytes).unwrap();
    println!("{}", back == v);
    let nums: Vec<i32> = serde_json::from_slice(b"[1, 2, 3]").unwrap();
    println!("{:?}", nums);
    let mut out: Vec<u8> = Vec::new();
    serde_json::to_writer(&mut out, &nums).unwrap();
    println!("{}", String::from_utf8(out).unwrap());
    let mut pretty: Vec<u8> = Vec::new();
    serde_json::to_writer_pretty(&mut pretty, &json!({"a": [1]})).unwrap();
    println!("{}", String::from_utf8(pretty).unwrap());
    let r: Vec<u32> = serde_json::from_reader("[4,5]".as_bytes()).unwrap();
    println!("{:?}", r);
    let pv = serde_json::to_vec_pretty(&nums).unwrap();
    println!("{}", pv.len());
    let bad: Result<Vec<i32>, _> = serde_json::from_slice(b"[1,");
    println!("{}", bad.is_err());
}
