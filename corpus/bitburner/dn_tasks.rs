//! Background chores for a darknet server, started by `darknet.rs`:
//!
//!   run dn_tasks.rs stasis   link this server so it stops migrating
//!   run dn_tasks.rs clear    free the RAM the owner is squatting on
//!   run dn_tasks.rs caches   open any .cache files lying around
//!   run dn_tasks.rs phish    phish for money and open whatever caches it drops

use std::fmt;
use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Chore {
    Stasis,
    Clear,
    Caches,
    Phish,
}

impl FromStr for Chore {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, String> {
        Ok(match s {
            "stasis" => Chore::Stasis,
            "clear" => Chore::Clear,
            "caches" => Chore::Caches,
            "phish" => Chore::Phish,
            _ => return Err(format!("unknown chore '{s}'")),
        })
    }
}

/// What opening caches got us, summed.
#[derive(Debug, Default, Clone, PartialEq)]
struct Loot {
    money: f64,
    karma: f64,
    programs: Vec<String>,
    shares: Vec<(String, f64)>,
}

impl std::ops::AddAssign<CacheReward> for Loot {
    fn add_assign(&mut self, r: CacheReward) {
        self.money += r.money.unwrap_or(0.0);
        self.karma += r.karma_loss;
        self.programs.extend(r.program_name);
        if let (Some(sym), Some(n)) = (r.stock_symbol, r.stock_shares) {
            self.shares.push((sym, n));
        }
    }
}

impl std::ops::AddAssign for Loot {
    fn add_assign(&mut self, other: Loot) {
        self.money += other.money;
        self.karma += other.karma;
        self.programs.extend(other.programs);
        self.shares.extend(other.shares);
    }
}

impl fmt::Display for Loot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "${:.0}", self.money)?;
        if self.karma != 0.0 {
            write!(f, ", karma -{:.1}", self.karma)?;
        }
        if !self.programs.is_empty() {
            write!(f, ", programs [{}]", self.programs.join(", "))?;
        }
        for (sym, n) in &self.shares {
            write!(f, ", {n} {sym}")?;
        }
        Ok(())
    }
}

fn open_caches(ns: &NS) -> Loot {
    let here = ns.get_hostname();
    let mut loot = Loot::default();
    for file in ns.ls(&here, ".cache") {
        loot += ns.dnet.open_cache(&file, true);
    }
    loot
}

pub async fn main(ns: NS) {
    let Some(chore) = ns.args.first().and_then(|a| a.as_string()) else {
        ns.tprint("usage: dn_tasks.rs <stasis|clear|caches|phish>");
        return;
    };
    let chore: Chore = match chore.parse() {
        Ok(c) => c,
        Err(e) => return ns.tprint(&e),
    };

    match chore {
        Chore::Stasis => {
            let r = ns.dnet.set_stasis_link(true).await;
            ns.print(&format!("stasis link: {}", r.message));
        }
        Chore::Clear => {
            let mut rounds = 0u32;
            while ns.dnet.get_blocked_ram() > 0.0 {
                let r = ns.dnet.memory_reallocation().await;
                rounds += 1;
                if !r.success {
                    ns.print(&format!("reallocation failed: {}", r.message));
                    break;
                }
            }
            ns.print(&format!("freed blocked RAM in {rounds} rounds"));
        }
        Chore::Caches => {
            let loot = open_caches(&ns);
            if loot != Loot::default() {
                ns.toast(&format!("caches: {loot}"), "success", 4000);
            }
        }
        Chore::Phish => {
            let mut total = Loot::default();
            let (mut wins, mut tries) = (0u32, 0u32);
            loop {
                let r = ns.dnet.phishing_attack().await;
                tries += 1;
                wins += r.success as u32;
                total += open_caches(&ns);
                if tries % 20 == 0 {
                    ns.print(&format!(
                        "phishing: {wins}/{tries} ({:.0}%), loot {total}",
                        100.0 * f64::from(wins) / f64::from(tries)
                    ));
                }
                // Let the crawler take the server back for something better.
                if ns.dnet.get_blocked_ram() > 0.0 {
                    break;
                }
            }
        }
    }
}
