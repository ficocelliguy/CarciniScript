//! A live tail-window monitor of every rooted server with money: a bar of money against max, the
//! security above the minimum, the hack chance, refreshed every second, with a button to change
//! the sort order and one to show only targets worth hacking.

use react::{jsx, use_effect, use_state, Node};
use std::collections::{HashSet, VecDeque};

#[derive(Clone, Debug, PartialEq)]
struct Row {
    host: String,
    money: f64,
    max_money: f64,
    sec_over: f64,
    chance: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum SortBy {
    Money,
    Chance,
    Name,
}

impl SortBy {
    fn next(self) -> SortBy {
        match self {
            SortBy::Money => SortBy::Chance,
            SortBy::Chance => SortBy::Name,
            SortBy::Name => SortBy::Money,
        }
    }
    fn label(self) -> &'static str {
        match self {
            SortBy::Money => "max money",
            SortBy::Chance => "hack chance",
            SortBy::Name => "name",
        }
    }
}

/// Every host reachable from home.
fn hosts(ns: &NS) -> Vec<String> {
    let mut seen = HashSet::from(["home".to_string()]);
    let mut queue = VecDeque::from(["home".to_string()]);
    let mut out = Vec::new();
    while let Some(h) = queue.pop_front() {
        for n in ns.scan(&h) {
            if seen.insert(n.clone()) {
                queue.push_back(n);
            }
        }
        out.push(h);
    }
    out
}

fn snapshot(ns: &NS) -> Vec<Row> {
    hosts(ns)
        .into_iter()
        .filter(|h| h != "home" && ns.has_root_access(h) && ns.get_server_max_money(h) > 0.0)
        .map(|h| Row {
            money: ns.get_server_money_available(&h),
            max_money: ns.get_server_max_money(&h),
            sec_over: ns.get_server_security_level(&h) - ns.get_server_min_security_level(&h),
            chance: ns.hack_analyze_chance(&h),
            host: h,
        })
        .collect()
}

fn money(x: f64) -> String {
    match x {
        x if x >= 1e12 => format!("${:.2}t", x / 1e12),
        x if x >= 1e9 => format!("${:.2}b", x / 1e9),
        x if x >= 1e6 => format!("${:.2}m", x / 1e6),
        x if x >= 1e3 => format!("${:.2}k", x / 1e3),
        x => format!("${:.0}", x),
    }
}

fn Meter(fraction: f64, color: &str) -> Node {
    let pct = (fraction.clamp(0.0, 1.0) * 100.0).round();
    jsx! {
        <div style="background: #333; height: 6px; width: 120px">
            <div style={format!("background: {}; height: 6px; width: {}%", color, pct)}></div>
        </div>
    }
}

fn Line(row: Row) -> Node {
    let full = if row.max_money > 0.0 { row.money / row.max_money } else { 0.0 };
    let sec_color = if row.sec_over < 1.0 { "#0c0" } else { "#fa0" };
    jsx! {
        <tr>
            <td>{row.host.clone()}</td>
            <td><Meter fraction={full} color="#0c0" /></td>
            <td>{money(row.money)} " / " {money(row.max_money)}</td>
            <td style={format!("color: {}", sec_color)}>{format!("+{:.2}", row.sec_over)}</td>
            <td>{format!("{:.0}%", row.chance * 100.0)}</td>
        </tr>
    }
}

pub async fn main(ns: NS) {
    ns.disable_log("ALL");
    ns.ui.open_tail();
    ns.ui.resize_tail(620.0, 420.0);

    let monitor = {
        let ns = ns.clone();
        move || -> Node {
            let (rows, set_rows) = use_state(|| snapshot(&ns));
            let (sort, set_sort) = use_state(|| SortBy::Money);
            let (only_ready, set_only_ready) = use_state(|| false);
            let refresh = {
                let ns = ns.clone();
                let set_rows = set_rows.clone();
                move || set_rows.set(snapshot(&ns))
            };
            use_effect(
                move || {
                    let id = js::set_interval(refresh.clone(), 1000.0);
                    move || js::clear_interval(id)
                },
                (),
            );
            let mut shown: Vec<Row> = rows
                .iter()
                .filter(|r| !only_ready || (r.chance > 0.5 && r.sec_over < 1.0))
                .cloned()
                .collect();
            match sort {
                SortBy::Money => shown.sort_by(|a, b| b.max_money.total_cmp(&a.max_money)),
                SortBy::Chance => shown.sort_by(|a, b| b.chance.total_cmp(&a.chance)),
                SortBy::Name => shown.sort_by(|a, b| a.host.cmp(&b.host)),
            }
            let total: f64 = shown.iter().map(|r| r.money).sum();
            let (next_sort, toggle) = (set_sort.clone(), set_only_ready.clone());
            jsx! {
                <div style="font-family: monospace; font-size: 12px">
                    <div>
                        <button on_click={move |_e| next_sort.update(|s| s.next())}>
                            "sort: " {sort.label()}
                        </button>
                        " "
                        <button on_click={move |_e| toggle.update(|o| !o)}>
                            {if only_ready { "showing ready targets" } else { "showing all" }}
                        </button>
                        " " {shown.len()} " servers, " {money(total)} " on them"
                    </div>
                    <table>
                        <thead>
                            <tr><th>"host"</th><th>"money"</th><th></th><th>"sec"</th><th>"chance"</th></tr>
                        </thead>
                        <tbody>{shown.into_iter().map(|row| jsx! { <Line key={row.host.clone()} row={row.clone()} /> }).collect::<Vec<_>>()}</tbody>
                    </table>
                </div>
            }
        }
    };
    ns.print_raw(jsx! { <monitor /> });
    // (The tail window's React tree calls `ns` from its interval, so the script stays alive.)
    loop {
        ns.sleep(60_000.0).await;
    }
}
