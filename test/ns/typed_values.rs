// Typed NS values: `ns.args` as `JsValue`s read with `as_string()` / `as_f64()` (also as
// function values), NS option interfaces built as structs (`RunOptions { .., ..Default::default() }`)
// and from another value (`Server { .., ..base }`), `Player` where a `Person` is expected,
// intersection results (`DarknetResult & { logs }`), tuples, `Map` as `HashMap`, inline object
// results, and `serde_wasm_bindgen` between `JsValue`s and typed values.
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
#[serde(untagged)]
enum Answer {
    Int(i64),
    Pairs(Vec<[i64; 2]>),
}

fn arg_text(v: &JsValue) -> String {
    v.as_string()
        .or_else(|| v.as_f64().map(|n| n.to_string()))
        .or_else(|| v.as_bool().map(|b| b.to_string()))
        .unwrap_or_default()
}

/// A `Server` field in a struct of the script's own.
struct Plan {
    target: Server,
    opts: BasicHGWOptions,
}

pub async fn main(ns: NS) {
    // A2: arguments.
    let target = ns.args.first().and_then(|a| a.as_string()).unwrap_or_default();
    let threads = ns.args.get(1).and_then(JsValue::as_f64).unwrap_or(1.0);
    let words: Vec<String> = ns.args.iter().map(arg_text).collect();
    ns.tprint(&format!("{target} {threads} {words:?} {}", ns.args[2].is_string()));

    // B1: option interfaces.
    let opts = RunOptions { threads: Some(threads), temporary: Some(true), ..Default::default() };
    ns.exec("worker.rs", &target, opts, &target);
    let plain = RunOptions::default();
    ns.run("worker.rs", plain);
    let base = ns.get_server(&target);
    let prepped = Server { money_available: base.money_max, hack_difficulty: Some(1.0), ..base };
    let plan = Plan { target: prepped, opts: BasicHGWOptions { additional_msec: Some(20.0), ..Default::default() } };
    ns.hack(&plan.target.hostname, plan.opts).await;

    // B3: `Player` where a `Person` is expected.
    let player = ns.get_player();
    let pct = ns.formulas.hacking.hack_percent(&plan.target, &player);
    ns.tprint(&format!("hack percent {pct:.3} with money {:?}", plan.target.money_available));

    // B2: intersection results.
    let result = ns.dnet.authenticate("dark", "pw").await;
    let data = result.data.and_then(|d| d.as_string());
    ns.tprint(&format!("auth {} {:?} {data:?}", result.success, result.message));
    let peek = HeartbleedOptions { peek: Some(true), ..Default::default() };
    let logs = ns.dnet.heartbleed("dark", peek).await.logs;
    let details: DarknetServerDetails = ns.dnet.get_server_details("dark");
    let online = ns.dnet.get_server_details("dark").is_online;
    ns.tprint(&format!("logs {logs:?} model {} online {online}", details.model_id));

    // B4: tuples, maps and inline object results.
    let [long, avg, short, short_avg]: [f64; 4] = ns.stock.get_position("ECP");
    ns.tprint(&format!("position {long} {avg} {short} {short_avg}"));
    let reset = ns.get_reset_info();
    let sf8 = reset.owned_sf.get(&8).copied().unwrap_or(0.0);
    let mut sfs: Vec<i32> = reset.owned_sf.keys().copied().collect();
    sfs.sort();
    ns.tprint(&format!("sf8 {sf8} owned {sfs:?}"));
    let outcome = ns.go.make_move(1.0, 2.0).await;
    ns.tprint(&format!("go {} {:?} {:?}", outcome.r#type, outcome.x, outcome.y));

    // B5: serde_wasm_bindgen.
    let raw = ns.codingcontract.get_data("c.cct", "home");
    let pairs: Vec<[i64; 2]> = serde_wasm_bindgen::from_value(raw).expect("pairs");
    let answer = Answer::Pairs(pairs);
    let reward = ns.codingcontract.attempt(serde_wasm_bindgen::to_value(&answer).unwrap(), "c.cct", "home");
    let bad = serde_wasm_bindgen::from_value::<Vec<i64>>(JsValue::from_str("x"));
    ns.tprint(&format!("{reward} {:?}", bad.map_err(|e| e.to_string())));
    let n = serde_wasm_bindgen::to_value(&Answer::Int(6)).unwrap();
    ns.tprint(&format!("int answer {}", n.as_debug_string()));
}
