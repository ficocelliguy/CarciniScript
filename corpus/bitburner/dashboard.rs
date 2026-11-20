//! Tail-window dashboard: reads `host|target|action|amount` lines from port 1 and shows
//! per-target totals and rates, redrawn once a second.

use std::collections::BTreeMap;
use std::fmt::{self, Write as _};
use std::time::{Duration, Instant};

const PORT: i32 = 1;
const CYAN: &str = "\u{1b}[36m";
const GREEN: &str = "\u{1b}[32m";
const RED: &str = "\u{1b}[31m";
const RESET: &str = "\u{1b}[0m";

#[derive(Debug, Default, Clone)]
struct Totals {
    hacks: u32,
    grows: u32,
    weakens: u32,
    stolen: f64,
    workers: std::collections::BTreeSet<String>,
}

#[derive(Debug)]
struct ParseError<'a>(&'a str);

impl fmt::Display for ParseError<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "bad line {:?}", self.0)
    }
}

/// Borrowed view of one message.
struct Event<'a> {
    host: &'a str,
    target: &'a str,
    action: &'a str,
    amount: f64,
}

impl<'a> TryFrom<&'a str> for Event<'a> {
    type Error = ParseError<'a>;
    fn try_from(line: &'a str) -> Result<Self, Self::Error> {
        let mut parts = line.splitn(4, '|');
        let (Some(host), Some(target), Some(action), Some(amount)) =
            (parts.next(), parts.next(), parts.next(), parts.next())
        else {
            return Err(ParseError(line));
        };
        let amount = amount.parse().map_err(|_| ParseError(line))?;
        Ok(Event { host, target, action, amount })
    }
}

struct Dashboard {
    started: Instant,
    by_target: BTreeMap<String, Totals>,
    bad_lines: usize,
}

impl Dashboard {
    fn apply(&mut self, e: Event<'_>) {
        let t = self.by_target.entry(e.target.to_owned()).or_default();
        match e.action.trim() {
            "hack" => {
                t.hacks += 1;
                t.stolen += e.amount;
            }
            "grow" => t.grows += 1,
            "weaken" => t.weakens += 1,
            _ => {}
        }
        if !t.workers.contains(e.host) {
            t.workers.insert(e.host.to_owned());
        }
    }

    fn render(&self, ns: &NS) -> String {
        let elapsed = self.started.elapsed().max(Duration::from_secs(1));
        let mut out = String::new();
        let _ = writeln!(
            out,
            "{CYAN}{:<18}{RESET} {:>6} {:>6} {:>6} {:>10} {:>10} {:>4}",
            "target", "H", "G", "W", "stolen", "$/s", "hosts"
        );
        for (target, t) in &self.by_target {
            let rate = t.stolen / elapsed.as_secs_f64();
            let color = if rate > 0.0 { GREEN } else { RED };
            let _ = writeln!(
                out,
                "{target:<18} {:>6} {:>6} {:>6} {:>10} {color}{:>10}{RESET} {:>4}",
                t.hacks,
                t.grows,
                t.weakens,
                ns.format.number(t.stolen),
                ns.format.number(rate),
                t.workers.len(),
            );
        }
        let total: f64 = self.by_target.values().map(|t| t.stolen).sum();
        let _ = write!(
            out,
            "total ${} in {:.0?}{}",
            ns.format.number(total),
            elapsed,
            if self.bad_lines > 0 { format!(" ({} bad lines)", self.bad_lines) } else { String::new() }
        );
        out
    }
}

pub async fn main(ns: NS) {
    ns.disable_log("ALL");
    ns.ui.open_tail();
    ns.ui.resize_tail(720, 360);
    let port = ns.get_port_handle(PORT);
    let mut dash = Dashboard { started: Instant::now(), by_target: BTreeMap::new(), bad_lines: 0 };
    let mut last_draw = Instant::now() - Duration::from_secs(2);

    loop {
        while !port.empty() {
            let Some(line) = port.read().as_string() else { continue };
            match Event::try_from(line.as_str()) {
                Ok(event) => dash.apply(event),
                Err(e) => {
                    dash.bad_lines += 1;
                    ns.print(&e.to_string());
                }
            }
        }
        if last_draw.elapsed() >= Duration::from_secs(1) {
            ns.clear_log();
            ns.print(&dash.render(&ns));
            last_draw = Instant::now();
        }
        port.next_write().await;
    }
}
