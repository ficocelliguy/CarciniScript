// `#[serde(untagged)]` enums: written as their content alone, read as the first variant whose
// content matches (and an error naming the enum when none does).
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, PartialEq)]
#[serde(untagged)]
enum Answer {
    Int(i64),
    Text(String),
    Ints(Vec<i64>),
    Pairs(Vec<[i64; 2]>),
    Point { x: f64, y: f64 },
    Nothing,
}

#[derive(Serialize, Deserialize, Debug)]
struct Attempt {
    id: u32,
    answer: Answer,
    alternatives: Vec<Answer>,
}

fn main() {
    let answers = vec![
        Answer::Int(6),
        Answer::Text("()()".into()),
        Answer::Ints(vec![1, -2, 3]),
        Answer::Pairs(vec![[1, 3], [5, 9]]),
        Answer::Point { x: 1.5, y: -2.0 },
        Answer::Nothing,
    ];
    for a in &answers {
        let text = serde_json::to_string(a).unwrap();
        let back: Answer = serde_json::from_str(&text).unwrap();
        println!("{text} -> {back:?} {}", back == *a);
    }
    let attempt = Attempt { id: 3, answer: Answer::Int(42), alternatives: answers };
    let text = serde_json::to_string_pretty(&attempt).unwrap();
    println!("{text}");
    let back: Attempt = serde_json::from_str(&text).unwrap();
    println!("{back:?}");
    // The first variant that fits: `[1, 2]` is `Ints`, not `Pairs`.
    println!("{:?}", serde_json::from_str::<Answer>("[[1, 2]]").unwrap());
    println!("{:?}", serde_json::from_str::<Answer>("[1, 2]").unwrap());
    println!("{:?}", serde_json::from_str::<Answer>("true").map_err(|e| e.to_string()));
    let v = serde_json::to_value(Answer::Text("x".into())).unwrap();
    println!("{v}");
}
