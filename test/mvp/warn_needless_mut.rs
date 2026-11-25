// Score servers for hacking. Leftovers from refactoring: a `mut` that isn't needed, a helper
// nothing calls any more, an enum variant never built.

#[derive(Debug)]
enum Verdict {
    Hack,
    Grow,
    Skip,
}

fn verdict(money: f64, max: f64) -> Verdict {
    if money >= max * 0.9 { Verdict::Hack } else { Verdict::Grow }
}

fn format_money(x: f64) -> String {
    format!("${:.2}m", x / 1e6)
}

fn main() {
    let mut servers = vec![("n00dles", 70000.0, 1750000.0), ("foodnstuff", 49000000.0, 50000000.0)];
    let mut report = String::new();
    for (name, money, max) in &servers {
        let v = verdict(*money, *max);
        report.push_str(&format!("{name}: {v:?}\n"));
    }
    print!("{report}");
}
