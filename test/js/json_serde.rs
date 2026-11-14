// serde's attributes with `JSON`: `rename_all` on structs and enums (variants, and a variant's
// fields), `rename`, `skip`, `skip_serializing`, `default`, and missing `Option` fields.
use js::JSON;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ServerInfo {
    host_name: String,
    max_ram: f64,
    #[serde(rename = "root")]
    has_root: bool,
    #[serde(skip)]
    cache: Vec<u8>,
    #[serde(default)]
    tries: u32,
    #[serde(skip_serializing)]
    secret: String,
    backdoor: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Action {
    HackServer,
    GrowMoney(f64),
    #[serde(rename = "wait")]
    Sleep { seconds: u32 },
    #[serde(rename_all = "SCREAMING_SNAKE_CASE")]
    Share { thread_count: u32 },
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct Limits {
    min_money: u64,
    max_security_level: f32,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Upper {
    first_name: String,
}

fn main() {
    let s = ServerInfo {
        host_name: "n00dles".into(),
        max_ram: 4.0,
        has_root: true,
        cache: vec![1, 2],
        tries: 3,
        secret: "hidden".into(),
        backdoor: None,
    };
    println!("{}", JSON::stringify(&s));
    let back: ServerInfo = JSON::parse(
        "{\"hostName\":\"joes\",\"maxRam\":16,\"root\":false,\"secret\":\"pw\",\"cache\":[9]}",
    )
    .unwrap();
    println!("{:?}", back);
    println!("{:?}", JSON::parse::<ServerInfo>("{\"hostName\":\"x\",\"maxRam\":1,\"secret\":\"s\"}").map(|s| s.host_name));
    let actions = vec![
        Action::HackServer,
        Action::GrowMoney(1.5),
        Action::Sleep { seconds: 2 },
        Action::Share { thread_count: 4 },
    ];
    let text = JSON::stringify(&actions);
    println!("{}", text);
    let again: Vec<Action> = JSON::parse(&text).unwrap();
    println!("{:?}", again);
    println!("{:?}", JSON::parse::<Action>("\"HackServer\"").map(|_| ()));
    let l = Limits { min_money: 1000, max_security_level: 5.5 };
    println!("{}", JSON::stringify_pretty(&l));
    println!("{:?}", JSON::parse::<Limits>("{\"min-money\": 7, \"max-security-level\": 2}"));
    println!("{}", JSON::stringify(&Upper { first_name: "a".into() }));
}
