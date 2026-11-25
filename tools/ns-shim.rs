// The NS API natively, for comparing a Netscript script's output and diagnostics with rustc's
// (test/mvp/). The script is compiled as `mod script { use crate::ns::*; include!("x.rs"); }`
// next to this module, and its `main(ns)` is run to completion. The network below is also the JS
// mock's: test/run.mjs reads `SERVERS` out of this file, so the two can't drift apart.
#![allow(warnings)]
use std::cell::RefCell;
use std::fmt;
use std::future::Future;
use std::pin::pin;
use std::task::{Context, Poll, Waker};

// (name, ports, hacking level, max money, min security, security, money, max RAM, neighbors)
#[rustfmt::skip]
const SERVERS: &[(&str, i32, i32, f64, f64, f64, f64, f64, &[&str])] = &[
    ("home", 0, 1, 0.0, 1.0, 1.0, 0.0, 64.0, &["n00dles", "foodnstuff", "sigma-cosmetics"]),
    ("n00dles", 0, 1, 1750000.0, 1.0, 1.0, 70000.0, 4.0, &["home", "CSEC"]),
    ("foodnstuff", 0, 1, 50000000.0, 3.0, 10.0, 2000000.0, 16.0, &["home", "max-hardware"]),
    ("sigma-cosmetics", 0, 5, 57500000.0, 3.0, 10.0, 2300000.0, 16.0, &["home", "zer0", "nectar-net"]),
    ("CSEC", 1, 58, 0.0, 1.0, 1.0, 0.0, 8.0, &["n00dles"]),
    ("max-hardware", 1, 80, 250000000.0, 5.0, 15.0, 10000000.0, 32.0, &["foodnstuff", "omega-net"]),
    ("zer0", 1, 75, 187500000.0, 3.0, 8.0, 7500000.0, 32.0, &["sigma-cosmetics"]),
    ("nectar-net", 0, 20, 68750000.0, 7.0, 20.0, 2750000.0, 16.0, &["sigma-cosmetics", "neo-net", "phantasy"]),
    ("neo-net", 1, 50, 125000000.0, 8.0, 25.0, 5000000.0, 32.0, &["nectar-net"]),
    ("phantasy", 2, 100, 600000000.0, 7.0, 20.0, 24000000.0, 32.0, &["nectar-net"]),
    ("omega-net", 2, 199, 1700000000.0, 10.0, 30.0, 68000000.0, 32.0, &["max-hardware"]),
];
const HACKING_LEVEL: i32 = 50;
const PROGRAMS: &[&str] = &["BruteSSH.exe", "FTPCrack.exe"];

struct Server {
    money: f64,
    security: f64,
    opened: i32,
    rooted: bool,
}

pub struct NS {
    pub args: Vec<ScriptArg>,
    servers: RefCell<Vec<Server>>,
}

fn index(host: &str) -> usize {
    SERVERS.iter().position(|s| s.0 == host).unwrap_or_else(|| panic!("unknown host {host}"))
}

impl NS {
    pub fn new(args: Vec<ScriptArg>) -> NS {
        let servers = SERVERS
            .iter()
            .map(|s| Server { money: s.6, security: s.5, opened: 0, rooted: s.0 == "home" })
            .collect();
        NS { args, servers: RefCell::new(servers) }
    }
    pub fn tprint(&self, s: impl fmt::Display) {
        println!("{s}");
    }
    pub fn print(&self, s: impl fmt::Display) {
        println!("[log] {s}");
    }
    pub fn get_hostname(&self) -> String {
        "home".to_string()
    }
    pub fn get_hacking_level(&self) -> i32 {
        HACKING_LEVEL
    }
    pub fn scan(&self, host: &str) -> Vec<String> {
        SERVERS[index(host)].8.iter().map(|s| s.to_string()).collect()
    }
    pub fn file_exists(&self, filename: &str, host: &str) -> bool {
        host == "home" && PROGRAMS.contains(&filename)
    }
    pub fn brutessh(&self, host: &str) -> bool {
        self.servers.borrow_mut()[index(host)].opened += 1;
        true
    }
    pub fn ftpcrack(&self, host: &str) -> bool {
        self.servers.borrow_mut()[index(host)].opened += 1;
        true
    }
    pub fn nuke(&self, host: &str) -> bool {
        let i = index(host);
        let mut servers = self.servers.borrow_mut();
        if servers[i].opened < SERVERS[i].1 {
            return false;
        }
        servers[i].rooted = true;
        true
    }
    pub fn has_root_access(&self, host: &str) -> bool {
        self.servers.borrow()[index(host)].rooted
    }
    pub fn get_server_num_ports_required(&self, host: &str) -> i32 {
        SERVERS[index(host)].1
    }
    pub fn get_server_required_hacking_level(&self, host: &str) -> i32 {
        SERVERS[index(host)].2
    }
    pub fn get_server_max_money(&self, host: &str) -> f64 {
        SERVERS[index(host)].3
    }
    pub fn get_server_min_security_level(&self, host: &str) -> f64 {
        SERVERS[index(host)].4
    }
    pub fn get_server_security_level(&self, host: &str) -> f64 {
        self.servers.borrow()[index(host)].security
    }
    pub fn get_server_money_available(&self, host: &str) -> f64 {
        self.servers.borrow()[index(host)].money
    }
    pub fn get_server_max_ram(&self, host: &str) -> f64 {
        SERVERS[index(host)].7
    }
    pub fn get_server_used_ram(&self, host: &str) -> f64 {
        if host == "home" { 8.0 } else { 0.0 }
    }
    // hack takes a tenth of the money (none without root access or the level), grow multiplies
    // it by 1.5 plus 10000 up to the maximum and returns the factor, weaken takes 2 off the
    // security down to the minimum and returns what it took.
    pub async fn hack(&self, host: &str) -> f64 {
        let i = index(host);
        let mut servers = self.servers.borrow_mut();
        let s = &mut servers[i];
        if !s.rooted || SERVERS[i].2 > HACKING_LEVEL {
            return 0.0;
        }
        let stolen = s.money * 0.1;
        s.money -= stolen;
        s.security += 0.5;
        stolen
    }
    pub async fn grow(&self, host: &str) -> f64 {
        let i = index(host);
        let mut servers = self.servers.borrow_mut();
        let s = &mut servers[i];
        if !s.rooted {
            return 1.0;
        }
        let before = s.money;
        s.money = (s.money * 1.5 + 10000.0).min(SERVERS[i].3);
        s.security += 1.0;
        s.money / before
    }
    pub async fn weaken(&self, host: &str) -> f64 {
        let i = index(host);
        let mut servers = self.servers.borrow_mut();
        let s = &mut servers[i];
        if !s.rooted {
            return 0.0;
        }
        let before = s.security;
        s.security = (s.security - 2.0).max(SERVERS[i].4);
        before - s.security
    }
    pub async fn sleep(&self, millis: f64) -> bool {
        true
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ScriptArg {
    Str(String),
    Num(f64),
    Bool(bool),
}
impl ScriptArg {
    pub fn as_string(&self) -> Option<String> {
        if let ScriptArg::Str(s) = self { Some(s.clone()) } else { None }
    }
    pub fn is_string(&self) -> bool {
        matches!(self, ScriptArg::Str(_))
    }
    pub fn as_f64(&self) -> Option<f64> {
        if let ScriptArg::Num(n) = self { Some(*n) } else { None }
    }
    pub fn as_bool(&self) -> Option<bool> {
        if let ScriptArg::Bool(b) = self { Some(*b) } else { None }
    }
}
impl fmt::Display for ScriptArg {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ScriptArg::Str(s) => write!(f, "{s}"),
            ScriptArg::Num(n) => write!(f, "{n}"),
            ScriptArg::Bool(b) => write!(f, "{b}"),
        }
    }
}

/// Runs a future whose awaits are all ready (the shim's are).
pub fn block_on<F: Future>(f: F) -> F::Output {
    let mut f = pin!(f);
    match f.as_mut().poll(&mut Context::from_waker(Waker::noop())) {
        Poll::Ready(v) => v,
        Poll::Pending => panic!("the script awaited something that isn't ready"),
    }
}
