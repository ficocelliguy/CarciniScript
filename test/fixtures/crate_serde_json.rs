// The bundled serde_json: derives, `to_string` / `to_string_pretty` / `from_str` with
// serde_json's errors (and their positions), `Value` (`json!`, indexing, `get`, `as_*`,
// `pointer`, matching, `{}` / `{:#}` / `{:?}`), and `to_value` / `from_value`.
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
struct Server {
    name: String,
    ram: u32,
    ports: Vec<u16>,
    root: bool,
    #[serde(default)]
    tags: Vec<String>,
    owner: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "snake_case")]
enum Action {
    Hack { target: String },
    Grow(String),
    Idle,
}

#[derive(Serialize, Deserialize, Debug)]
struct Plan {
    actions: Vec<Action>,
    weights: HashMap<String, f64>,
    meta: Value,
}

fn load(text: &str) -> Result<Server, Box<dyn std::error::Error>> {
    let s: Server = serde_json::from_str(text)?;
    Ok(s)
}

fn main() -> Result<(), serde_json::Error> {
    let s = Server {
        name: "n00dles".to_string(),
        ram: 4,
        ports: vec![22, 80],
        root: true,
        tags: vec![],
        owner: None,
    };
    let text = serde_json::to_string(&s)?;
    println!("{}", text);
    println!("{}", serde_json::to_string_pretty(&s)?);
    let back: Server = serde_json::from_str(&text)?;
    println!("{:?} {}", back, back == s);

    // Errors, with where they happened.
    for bad in [
        "",
        "{",
        "{\"name\": \"x\"}",
        "{\"name\": 5, \"ram\": 1}",
        "{\"name\":\"a\",\"ram\":70000000000,\"ports\":[],\"root\":true}",
        "{\n  \"name\": \"a\",\n  \"ram\": -1\n}",
        "nul",
        "{\"name\":\"a\",\"ram\":1,\"ports\":[],\"root\":false} x",
    ] {
        match serde_json::from_str::<Server>(bad) {
            Ok(v) => println!("ok {:?}", v),
            Err(e) => println!(
                "{} | line {} col {} eof {} syntax {} data {} | {:?}",
                e,
                e.line(),
                e.column(),
                e.is_eof(),
                e.is_syntax(),
                e.is_data(),
                e
            ),
        }
    }
    println!("{}", serde_json::from_str::<Vec<u32>>("[1, 2,]").unwrap_err());
    match load("{\"name\": true}") {
        Ok(_) => {}
        Err(e) => println!("boxed: {}", e),
    }

    // Values.
    let v = json!({
        "name": "foodnstuff",
        "ram": 16,
        "money": 1.5e6,
        "ports": [21, 22],
        "nested": { "deep": [true, null, "x"] },
        "neg": -3,
        "ratio": 0.25
    });
    println!("{}", v);
    println!("{:#}", v);
    println!("{:?}", v["ports"]);
    println!("{} {} {}", v["name"], v["ports"][1], v["missing"]);
    println!("{:?} {:?} {:?}", v["ram"].as_u64(), v["money"].as_f64(), v["name"].as_str());
    println!("{:?} {:?}", v["neg"].as_i64(), v["neg"].as_u64());
    println!("{:?} {:?}", v.get("ram"), v.get("nope"));
    println!("{:?}", v.pointer("/nested/deep/2"));
    println!("{} {} {}", v.is_object(), v["ports"].is_array(), v["ratio"].is_f64());
    if let Some(ports) = v["ports"].as_array() {
        let total: u64 = ports.iter().filter_map(|p| p.as_u64()).sum();
        println!("port total {}", total);
    }
    if let Value::Object(map) = &v {
        let keys: Vec<&String> = map.keys().collect();
        println!("{:?}", keys);
    }
    let name = "sigma";
    let n = 7;
    let built = json!({ "server": name, "threads": n * 2, "list": vec![1, 2, 3], "s": s.name });
    println!("{}", built);
    println!("{}", json!([1, "two", null, false, { "k": [] }]));
    println!("{}", json!(null));

    // Parsing into a Value and back.
    let parsed: Value = serde_json::from_str("{\"b\": [1, 2.5, -7, 1e3], \"a\": {\"x\": \"\\u00e9\\n\"}}")?;
    println!("{}", parsed);
    println!("{:?}", parsed);
    let as_server = serde_json::to_value(&s)?;
    println!("{}", as_server);
    let server2: Server = serde_json::from_value(as_server.clone())?;
    println!("{:?}", server2);
    let wrong: Result<Server, _> = serde_json::from_value(json!({"name": 1}));
    println!("{}", wrong.unwrap_err());
    let v2: Value = "[1, 2]".parse().unwrap();
    println!("{}", v2);

    // Values inside structs, and enums.
    let mut weights = HashMap::new();
    weights.insert("hack".to_string(), 0.5);
    let plan = Plan {
        actions: vec![
            Action::Hack { target: "n00dles".into() },
            Action::Grow("joesguns".into()),
            Action::Idle,
        ],
        weights,
        meta: json!({ "v": 2, "tags": ["a"] }),
    };
    let ptext = serde_json::to_string(&plan)?;
    println!("{}", ptext);
    let p2: Plan = serde_json::from_str(&ptext)?;
    println!("{:?}", p2);
    println!("{}", serde_json::to_string_pretty(&plan.meta)?);
    let mut m = serde_json::Map::new();
    m.insert("z".to_string(), Value::from(1));
    m.insert("a".to_string(), Value::String("s".into()));
    println!("{}", Value::Object(m));
    Ok(())
}
