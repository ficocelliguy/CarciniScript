//! Buys and upgrades purchased servers, doubling RAM whenever it's affordable.
//!
//! run servers.rs [--prefix pserv] [--min-ram 8] [--spend 0.5]

use std::collections::BTreeMap;
use std::iter::successors;

use anyhow::{bail, Context, Result};

#[derive(Debug, Clone)]
struct Config {
    prefix: String,
    min_ram: u32,
    spend: f64,
}

impl Default for Config {
    fn default() -> Self {
        Config { prefix: "pserv".into(), min_ram: 8, spend: 0.5 }
    }
}

fn parse_args(args: &[ScriptArg]) -> Result<Config> {
    let mut cfg = Config::default();
    let words: Vec<String> = args.iter().map(|a| a.as_string().unwrap_or_default()).collect();
    for pair in words.chunks(2) {
        match pair {
            [flag, value] if flag == "--prefix" => cfg.prefix = value.clone(),
            [flag, value] if flag == "--min-ram" => {
                cfg.min_ram = value.parse().with_context(|| format!("--min-ram {value:?}"))?;
                if !cfg.min_ram.is_power_of_two() {
                    bail!("--min-ram must be a power of two, got {}", cfg.min_ram);
                }
            }
            [flag, value] if flag == "--spend" => cfg.spend = value.parse()?,
            [flag] => bail!("{flag} needs a value"),
            other => bail!("unexpected arguments {other:?}"),
        }
    }
    Ok(cfg)
}

/// Powers of two from `from` up to the game's limit.
fn ram_sizes(from: u32, max: u32) -> impl Iterator<Item = u32> {
    successors(Some(from), |&r| r.checked_mul(2)).take_while(move |&r| r <= max)
}

/// The biggest RAM size we can afford for one server, if any beats `current`.
fn best_affordable(ns: &NS, budget: f64, from: u32, current: u32, max: u32) -> Option<u32> {
    ram_sizes(from, max)
        .filter(|&r| r > current)
        .take_while(|&r| ns.cloud.get_server_cost(r as f64) <= budget)
        .last()
}

pub async fn main(ns: NS) -> Result<()> {
    let cfg = parse_args(&ns.args)?;
    let limit = ns.cloud.get_server_limit() as usize;
    let max_ram = ns.cloud.get_ram_limit() as u32;

    loop {
        let owned: BTreeMap<String, u32> = ns
            .cloud
            .get_server_names()
            .into_iter()
            .map(|h| {
                let ram = ns.get_server_max_ram(&h) as u32;
                (h, ram)
            })
            .collect();

        if owned.len() == limit && owned.values().all(|&r| r == max_ram) {
            ns.tprint(&format!("all {limit} servers at {}", ns.format.ram(max_ram as f64)));
            return Ok(());
        }

        let budget = ns.get_server_money_available("home") * cfg.spend;
        if owned.len() < limit {
            if let Some(ram) = best_affordable(&ns, budget, cfg.min_ram, 0, max_ram) {
                let name = format!("{}-{:02}", cfg.prefix, owned.len());
                let host = ns.cloud.purchase_server(&name, ram as f64);
                if host.is_empty() {
                    bail!("purchase of {name} ({ram} GB) failed");
                }
                ns.print(&format!("bought {host} with {ram} GB"));
                continue;
            }
        } else if let Some((host, &ram)) = owned.iter().min_by_key(|(_, r)| **r)
            && let Some(next) = best_affordable(&ns, budget, cfg.min_ram, ram, max_ram)
        {
            let cost = ns.cloud.get_server_upgrade_cost(host, next as f64);
            ns.cloud
                .upgrade_server(host, next as f64)
                .then_some(())
                .with_context(|| format!("upgrading {host} to {next} GB"))?;
            ns.print(&format!("upgraded {host}: {ram} → {next} GB for ${}", ns.format.number(cost)));
            continue;
        }
        ns.sleep(10_000).await;
    }
}
