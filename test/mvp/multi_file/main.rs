// A report script split in two: formatting helpers live in util.rs.
mod util;

use util::{format_money, Server};

fn main() {
    let servers = vec![
        Server::new("n00dles", 70_000.0, 1_750_000.0),
        Server::new("foodnstuff", 2_000_000.0, 50_000_000.0),
        Server::new("phantasy", 24_000_000.0, 600_000_000.0),
    ];
    for s in &servers {
        println!("{:<12} {:>10} / {:>10} ({:>5.1}%)", s.name, format_money(s.money), format_money(s.max_money), s.fill() * 100.0);
    }
    let richest = servers.iter().max_by(|a, b| a.max_money.total_cmp(&b.max_money)).unwrap();
    println!("richest: {}", richest);
    println!("{}", util::bar(0.42, 20));
}
