// Tagged enums and flattened fields: `#[serde(tag = "..")]` (internally tagged: the variant's
// name as a field next to its content's), `#[serde(tag = "..", content = "..")]` (adjacently
// tagged), `#[serde(tag = "..")]` on a struct, and `#[serde(flatten)]` of a struct, a map and an
// `Option` (the port-message shapes between scripts), with serde_json's errors.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
struct Target {
    host: String,
    threads: u32,
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
#[serde(tag = "type")]
enum Message {
    Ping,
    Hack { target: String, threads: u32 },
    #[serde(rename = "grow")]
    Grow(Target),
    #[serde(rename_all = "camelCase")]
    Report { total_money: f64, servers: Vec<String> },
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
#[serde(tag = "kind", content = "data", rename_all = "snake_case")]
enum Event {
    Started,
    Scanned(Vec<String>),
    Money(f64, f64),
    Done { ok: bool },
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
enum Plain {
    Money(f64, f64),
    Unit,
    Named { a: u8 },
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
#[serde(tag = "type")]
struct Config {
    name: String,
    retries: u8,
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
struct Envelope {
    id: u32,
    #[serde(flatten)]
    target: Target,
    #[serde(flatten)]
    extra: BTreeMap<String, i64>,
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
struct Retry {
    after: u32,
    max: u8,
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
struct Tagged {
    seq: u32,
    #[serde(flatten)]
    message: Message,
    #[serde(flatten)]
    retry: Option<Retry>,
}

fn roundtrip<T: Serialize + for<'de> Deserialize<'de> + std::fmt::Debug + PartialEq>(v: &T) {
    let text = serde_json::to_string(v).unwrap();
    let back: T = serde_json::from_str(&text).unwrap();
    println!("{text} -> {back:?} {}", back == *v);
}

fn main() {
    let messages = vec![
        Message::Ping,
        Message::Hack { target: "n00dles".into(), threads: 12 },
        Message::Grow(Target { host: "joesguns".into(), threads: 3 }),
        Message::Report { total_money: 1.5e6, servers: vec!["a".into(), "b".into()] },
    ];
    for m in &messages {
        roundtrip(m);
    }
    println!("{}", serde_json::to_string_pretty(&messages).unwrap());
    let events = vec![
        Event::Started,
        Event::Scanned(vec!["home".into(), "n00dles".into()]),
        Event::Money(10.0, 2.5),
        Event::Done { ok: true },
    ];
    for e in &events {
        roundtrip(e);
    }
    println!("{}", serde_json::to_string_pretty(&events).unwrap());
    roundtrip(&Config { name: "x".into(), retries: 3 });
    let mut extra = BTreeMap::new();
    extra.insert("delay".to_string(), 250);
    extra.insert("port".to_string(), 1);
    roundtrip(&Envelope { id: 7, target: Target { host: "home".into(), threads: 1 }, extra });
    roundtrip(&Tagged { seq: 1, message: Message::Ping, retry: None });
    roundtrip(&Tagged {
        seq: 2,
        message: Message::Hack { target: "n00dles".into(), threads: 2 },
        retry: Some(Retry { after: 500, max: 9 }),
    });
    println!(
        "{}",
        serde_json::to_string_pretty(&Tagged {
            seq: 3,
            message: Message::Grow(Target { host: "j".into(), threads: 1 }),
            retry: None,
        })
        .unwrap()
    );

    // Reading what other scripts send: fields in any order, extras ignored, tag last.
    let reads = [
        r#"{"threads": 4, "target": "foodnstuff", "type": "Hack", "note": "x"}"#,
        r#"{"type":"Ping","ignored":1}"#,
        r#"{"host":"h","type":"grow","threads":"2"}"#,
        r#"{"type":"grow","host":"h"}"#,
        r#"{"type":"Hack","target":"t"}"#,
        r#"{"threads":1}"#,
        r#"{"type":"Nuke"}"#,
        r#"{"type":5}"#,
        r#""Ping""#,
        r#"[1]"#,
    ];
    for text in reads {
        println!("{:?}", serde_json::from_str::<Message>(text).map_err(|e| e.to_string()));
    }
    let reads = [
        r#"{"data":["a"],"kind":"scanned"}"#,
        r#"{"kind":"started"}"#,
        r#"{"kind":"started","data":null}"#,
        r#"{"kind":"started","data":1}"#,
        r#"{"kind":"done"}"#,
        r#"{"kind":"money","data":[1.0]}"#,
        r#"{"kind":"done","data":{"ok":false},"other":true}"#,
        r#"{"data":{"ok":false}}"#,
        r#"{"kind":"finished"}"#,
        r#"7"#,
    ];
    for text in reads {
        println!("{:?}", serde_json::from_str::<Event>(text).map_err(|e| e.to_string()));
    }
    let reads = [
        r#"{"id":1,"host":"h","threads":2,"a":1,"b":2}"#,
        r#"{"id":1,"host":"h","a":1}"#,
        r#"{"host":"h","threads":2}"#,
    ];
    for text in reads {
        println!("{:?}", serde_json::from_str::<Envelope>(text).map_err(|e| e.to_string()));
    }
    let reads = [
        r#"{"seq":1,"type":"Ping"}"#,
        r#"{"seq":1,"type":"Hack","target":"t","threads":1,"after":5,"max":1}"#,
        r#"{"seq":1,"type":"Hack","target":"t","threads":1,"after":5}"#,
        r#"{"seq":1,"type":"Ping","max":2,"after":"soon"}"#,
        r#"{"seq":1}"#,
        r#"{"seq":1,"type":"Hack"}"#,
    ];
    for text in reads {
        println!("{:?}", serde_json::from_str::<Tagged>(text).map_err(|e| e.to_string()));
    }
    let reads = [r#"{"name":"n","retries":1}"#, r#"{"type":"Config","name":"n","retries":1}"#, r#"{"type":"Other","name":"n","retries":1}"#];
    for text in reads {
        println!("{:?}", serde_json::from_str::<Config>(text).map_err(|e| e.to_string()));
    }
    let reads = [r#"{"Money":[1.0]}"#, r#"{"Unit":1}"#, r#"{"Named":{"a":"x"}}"#, r#"{"Named":{}}"#, r#"{"Money":[1.0,2.0,3.0]}"#, r#""Unit""#];
    for text in reads {
        println!("{:?}", serde_json::from_str::<Plain>(text).map_err(|e| e.to_string()));
    }
    let v = serde_json::to_value(&Message::Hack { target: "t".into(), threads: 1 }).unwrap();
    println!("{v} {:?}", serde_json::from_value::<Message>(v.clone()).unwrap());
}
