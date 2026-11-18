//! Batch worker: runs one hack, grow or weaken with a start delay, then reports to the
//! batcher's port so it can tell whether the batch landed in order.
//!
//! args: <op> <target> <delay-ms> <batch-id> <port>

use serde::{Deserialize, Serialize};
use std::str::FromStr;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Op {
    Hack,
    Grow,
    Weaken,
}

impl FromStr for Op {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "hack" | "h" => Ok(Op::Hack),
            "grow" | "g" => Ok(Op::Grow),
            "weaken" | "w" => Ok(Op::Weaken),
            other => Err(format!("unknown op {other:?}")),
        }
    }
}

/// What a worker sends back when it finishes.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub batch_id: u32,
    pub op: Op,
    pub target: String,
    pub host: String,
    pub result: f64,
    pub finished_at: f64,
}

fn arg_string(v: &ScriptArg) -> String {
    v.as_string()
        .or_else(|| v.as_f64().map(|n| n.to_string()))
        .unwrap_or_default()
}

pub async fn main(ns: NS) {
    let args: Vec<String> = ns.args.iter().map(arg_string).collect();
    let [op, target, delay, batch_id, port] = args.as_slice() else {
        ns.tprint("usage: hwgw.rs <op> <target> <delay> <batch-id> <port>");
        return;
    };
    let op: Op = match op.parse() {
        Ok(op) => op,
        Err(e) => return ns.tprint(&format!("ERROR {e}")),
    };
    let delay = delay.parse::<f64>().unwrap_or(0.0).max(0.0);
    let batch_id: u32 = batch_id.parse().expect("batch id is a number");
    let port: u32 = port.parse().expect("port is a number");

    let opts = BasicHGWOptions {
        additional_msec: Some(delay),
        ..Default::default()
    };
    let result = match op {
        Op::Hack => ns.hack(target, opts).await,
        Op::Grow => ns.grow(target, opts).await,
        Op::Weaken => ns.weaken(target, opts).await,
    };

    let report = Report {
        batch_id,
        op,
        target: target.clone(),
        host: ns.get_hostname(),
        result,
        finished_at: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs_f64() * 1000.0)
            .unwrap_or_default(),
    };
    let json = serde_json::to_string(&report).expect("report serializes");
    // A full port drops the oldest message; the batcher only uses reports for stats.
    ns.try_write_port(port as f64, json);
}
