//! Shared network helpers: crawling, rooting, and a RAM pool that hands out threads.
//!
//! Used by the batcher, the share/XP scripts and the contract solver via `mod net;`.

use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap, HashSet, VecDeque};
use std::fmt;
use std::ops::Deref;

/// A hostname. A newtype so it can't be confused with a script name or a file path.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Host(pub String);

impl Deref for Host {
    type Target = str;
    fn deref(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for Host {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl From<&str> for Host {
    fn from(s: &str) -> Self {
        Host(s.to_owned())
    }
}

impl From<String> for Host {
    fn from(s: String) -> Self {
        Host(s)
    }
}

impl fmt::Display for Host {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.pad(&self.0)
    }
}

/// The five port-opening programs, in the order players usually unlock them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PortOpener {
    BruteSsh,
    FtpCrack,
    RelaySmtp,
    HttpWorm,
    SqlInject,
}

impl PortOpener {
    pub const ALL: [PortOpener; 5] = [
        PortOpener::BruteSsh,
        PortOpener::FtpCrack,
        PortOpener::RelaySmtp,
        PortOpener::HttpWorm,
        PortOpener::SqlInject,
    ];

    pub const fn program(self) -> &'static str {
        match self {
            PortOpener::BruteSsh => "BruteSSH.exe",
            PortOpener::FtpCrack => "FTPCrack.exe",
            PortOpener::RelaySmtp => "relaySMTP.exe",
            PortOpener::HttpWorm => "HTTPWorm.exe",
            PortOpener::SqlInject => "SQLInject.exe",
        }
    }

    /// Runs the opener against `host`. Returns whether the port is now open.
    pub fn open(self, ns: &NS, host: &str) -> bool {
        match self {
            PortOpener::BruteSsh => ns.brutessh(host),
            PortOpener::FtpCrack => ns.ftpcrack(host),
            PortOpener::RelaySmtp => ns.relaysmtp(host),
            PortOpener::HttpWorm => ns.httpworm(host),
            PortOpener::SqlInject => ns.sqlinject(host),
        }
    }

    pub fn available(ns: &NS) -> impl Iterator<Item = PortOpener> + '_ {
        Self::ALL
            .into_iter()
            .filter(move |p| ns.file_exists(p.program(), "home"))
    }
}

/// Breadth-first walk of the network from `home`, yielding each host once along with
/// its distance and the path that reaches it.
pub struct Crawl<'a> {
    ns: &'a NS,
    queue: VecDeque<(Host, Vec<Host>)>,
    seen: HashSet<Host>,
}

impl<'a> Crawl<'a> {
    pub fn from(ns: &'a NS, start: impl Into<Host>) -> Self {
        let start = start.into();
        let mut seen = HashSet::new();
        seen.insert(start.clone());
        Crawl {
            ns,
            queue: VecDeque::from([(start, Vec::new())]),
            seen,
        }
    }
}

impl Iterator for Crawl<'_> {
    type Item = (Host, Vec<Host>);

    fn next(&mut self) -> Option<Self::Item> {
        let (host, path) = self.queue.pop_front()?;
        for next in self.ns.scan(&host) {
            let next = Host(next);
            if self.seen.insert(next.clone()) {
                let mut route = path.clone();
                route.push(next.clone());
                self.queue.push_back((next, route));
            }
        }
        Some((host, path))
    }
}

/// Every host reachable from home, home included.
pub fn all_hosts(ns: &NS) -> Vec<Host> {
    Crawl::from(ns, "home").map(|(h, _)| h).collect()
}

/// Opens what ports we can and nukes. `Ok(true)` if we have root afterwards.
pub fn try_root(ns: &NS, host: &str) -> Result<bool, RootError> {
    if ns.has_root_access(host) {
        return Ok(true);
    }
    let needed = ns.get_server_num_ports_required(host) as usize;
    let opened = PortOpener::available(ns)
        .filter(|p| p.open(ns, host))
        .count();
    if opened < needed {
        return Err(RootError::NotEnoughPorts { needed, opened });
    }
    let level = ns.get_hacking_level();
    let required = ns.get_server_required_hacking_level(host);
    if level < required {
        return Err(RootError::LevelTooLow { level, required });
    }
    Ok(ns.nuke(host))
}

#[derive(Debug)]
pub enum RootError {
    NotEnoughPorts { needed: usize, opened: usize },
    LevelTooLow { level: i32, required: i32 },
}

impl fmt::Display for RootError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RootError::NotEnoughPorts { needed, opened } => {
                write!(f, "needs {needed} ports, only {opened} opened")
            }
            RootError::LevelTooLow { level, required } => {
                write!(f, "hacking level {level} < {required}")
            }
        }
    }
}

impl std::error::Error for RootError {}

/// Free RAM on one host, with a reservation for home so the player can still run things.
#[derive(Debug, Clone, PartialEq)]
pub struct Slot {
    pub host: Host,
    pub free: f64,
    pub cores: u32,
}

/// A pool of free RAM across rooted servers. Hands out the biggest slot first so big
/// jobs (grow, weaken) don't get fragmented across many small servers.
pub struct RamPool {
    slots: Vec<Slot>,
}

pub const HOME_RESERVE_GB: f64 = 32.0;

impl RamPool {
    pub fn scan(ns: &NS) -> Self {
        let mut slots: Vec<Slot> = all_hosts(ns)
            .into_iter()
            .filter(|h| ns.has_root_access(h))
            .filter_map(|host| {
                let server = ns.get_server(&host);
                let reserve = if &*host == "home" { HOME_RESERVE_GB } else { 0.0 };
                let free = server.max_ram - server.ram_used - reserve;
                (free >= 1.6).then_some(Slot {
                    host,
                    free,
                    cores: server.cpu_cores as u32,
                })
            })
            .collect();
        slots.sort_by(|a, b| b.free.total_cmp(&a.free));
        RamPool { slots }
    }

    pub fn total_free(&self) -> f64 {
        self.slots.iter().map(|s| s.free).sum()
    }

    pub fn len(&self) -> usize {
        self.slots.len()
    }

    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }

    pub fn largest(&self) -> Option<&Slot> {
        self.slots.first()
    }

    /// Reserves `threads` × `ram` on one host if `split` is false, or spread over several
    /// otherwise. Returns where each chunk landed, or `None` (leaving the pool unchanged).
    pub fn reserve(&mut self, threads: u32, ram: f64, split: bool) -> Option<Vec<(Host, u32)>> {
        if threads == 0 {
            return Some(Vec::new());
        }
        if !split {
            let slot = self
                .slots
                .iter_mut()
                .filter(|s| s.free >= ram * threads as f64)
                // smallest slot that fits: best-fit keeps the big ones for later
                .min_by(|a, b| a.free.total_cmp(&b.free))?;
            slot.free -= ram * threads as f64;
            return Some(vec![(slot.host.clone(), threads)]);
        }
        let snapshot = self.slots.clone();
        let mut left = threads;
        let mut out = Vec::new();
        for slot in self.slots.iter_mut() {
            let fit = (slot.free / ram).floor() as u32;
            let take = fit.min(left);
            if take == 0 {
                continue;
            }
            slot.free -= take as f64 * ram;
            out.push((slot.host.clone(), take));
            left -= take;
            if left == 0 {
                break;
            }
        }
        if left > 0 {
            self.slots = snapshot;
            return None;
        }
        self.slots.retain(|s| s.free >= 1.6);
        Some(out)
    }
}

/// Picks the `k` hosts with the highest score without sorting everything.
pub fn top_k<T, F>(items: impl IntoIterator<Item = T>, k: usize, mut score: F) -> Vec<(f64, T)>
where
    F: FnMut(&T) -> f64,
{
    #[derive(PartialEq)]
    struct Scored(f64);
    impl Eq for Scored {}
    impl PartialOrd for Scored {
        fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
            Some(self.cmp(other))
        }
    }
    impl Ord for Scored {
        fn cmp(&self, other: &Self) -> std::cmp::Ordering {
            self.0.total_cmp(&other.0)
        }
    }

    let mut heap: BinaryHeap<(Reverse<Scored>, usize)> = BinaryHeap::new();
    let mut store: HashMap<usize, T> = HashMap::new();
    for (i, item) in items.into_iter().enumerate() {
        let s = score(&item);
        heap.push((Reverse(Scored(s)), i));
        store.insert(i, item);
        if heap.len() > k
            && let Some((_, dropped)) = heap.pop()
        {
            store.remove(&dropped);
        }
    }
    let mut out: Vec<(f64, T)> = heap
        .into_iter()
        .filter_map(|(Reverse(Scored(s)), i)| store.remove(&i).map(|t| (s, t)))
        .collect();
    out.sort_by(|a, b| b.0.total_cmp(&a.0));
    out
}

/// Disables the noisy default logs for the given functions.
#[macro_export]
macro_rules! quiet {
    ($ns:expr, $($name:literal),+ $(,)?) => {
        $( $ns.disable_log($name); )+
    };
}
