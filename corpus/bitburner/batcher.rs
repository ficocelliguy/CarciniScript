//! HWGW batcher. Preps the best target to min security / max money, then launches
//! batches of hack → weaken → grow → weaken timed to land `SPACER` ms apart, as many as
//! RAM allows, and keeps going until killed.
//!
//! run batcher.rs [--target <host>] [--fraction <0..1>] [--spacer <ms>] [--dry-run]

mod hwgw;
#[macro_use]
mod net;

use hwgw::{Op, Report};
use net::{Host, RamPool, top_k, try_root};

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::fmt;
use std::ops::{Index, IndexMut};
use std::rc::Rc;
use std::time::Duration;

use thiserror::Error;

const WORKER: &str = "hwgw.rs";
const REPORT_PORT: u32 = 7;
const MAX_BATCHES: usize = 2_000;
const HACK_SEC: f64 = 0.002;
const GROW_SEC: f64 = 0.004;

/// The four jobs of a batch, in landing order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
enum Step {
    Hack,
    Weaken1,
    Grow,
    Weaken2,
}

impl Step {
    const ORDER: [Step; 4] = [Step::Hack, Step::Weaken1, Step::Grow, Step::Weaken2];

    fn op(self) -> Op {
        match self {
            Step::Hack => Op::Hack,
            Step::Grow => Op::Grow,
            Step::Weaken1 | Step::Weaken2 => Op::Weaken,
        }
    }

    /// Grow and weaken need all their threads on one host to keep the math right
    /// (cores change the effect); hack threads can be split.
    fn splittable(self) -> bool {
        matches!(self, Step::Hack)
    }
}

/// Per-step thread counts.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct Threads([u32; 4]);

impl Index<Step> for Threads {
    type Output = u32;
    fn index(&self, s: Step) -> &u32 {
        &self.0[s as usize]
    }
}

impl IndexMut<Step> for Threads {
    fn index_mut(&mut self, s: Step) -> &mut u32 {
        &mut self.0[s as usize]
    }
}

impl Threads {
    fn total(&self) -> u32 {
        self.0.iter().sum()
    }
}

#[derive(Debug, Clone, Copy)]
struct Timings {
    hack: Duration,
    grow: Duration,
    weaken: Duration,
}

impl Timings {
    fn of(&self, step: Step) -> Duration {
        match step.op() {
            Op::Hack => self.hack,
            Op::Grow => self.grow,
            Op::Weaken => self.weaken,
        }
    }
}

#[derive(Debug, Clone)]
struct BatchPlan {
    target: Host,
    fraction: f64,
    threads: Threads,
    timings: Timings,
    money_per_batch: f64,
}

impl BatchPlan {
    fn ram(&self, script_ram: f64) -> f64 {
        self.threads.total() as f64 * script_ram
    }

    /// Delay for each step so they finish `spacer` apart, with the last weaken finishing
    /// `weaken + 3 * spacer` after launch.
    fn delays(&self, spacer: Duration) -> impl Iterator<Item = (Step, Duration)> + '_ {
        let end = self.timings.weaken + spacer * 3;
        Step::ORDER.iter().enumerate().map(move |(i, &step)| {
            let finish = end - spacer * (3 - i as u32);
            (step, finish.saturating_sub(self.timings.of(step)))
        })
    }
}

impl fmt::Display for BatchPlan {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let t = &self.threads;
        write!(
            f,
            "{:<18} {:>5.1}%  H{:<4} W{:<4} G{:<4} W{:<4} ${:.3e}/batch  {:.1?}",
            self.target,
            self.fraction * 100.0,
            t[Step::Hack],
            t[Step::Weaken1],
            t[Step::Grow],
            t[Step::Weaken2],
            self.money_per_batch,
            self.timings.weaken,
        )
    }
}

#[derive(Debug, Error)]
enum BatchError {
    #[error("target {0} has no money")]
    NoMoney(Host),
    #[error("cannot root {host}: {source}")]
    Root {
        host: Host,
        #[source]
        source: net::RootError,
    },
    #[error("not enough RAM for even one batch ({needed:.1} GB needed, {free:.1} GB free)")]
    NoRam { needed: f64, free: f64 },
    #[error("bad argument: {0}")]
    Args(String),
}

type Result<T, E = BatchError> = std::result::Result<T, E>;

#[derive(Debug)]
struct Options {
    target: Option<Host>,
    fraction: Option<f64>,
    spacer: Duration,
    dry_run: bool,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            target: None,
            fraction: None,
            spacer: Duration::from_millis(20),
            dry_run: false,
        }
    }
}

impl TryFrom<&[ScriptArg]> for Options {
    type Error = BatchError;

    fn try_from(raw: &[ScriptArg]) -> Result<Self> {
        let mut opts = Options::default();
        let mut args = raw
            .iter()
            .map(|v| v.as_string().unwrap_or_else(|| format!("{v:?}")))
            .peekable();
        while let Some(flag) = args.next() {
            let mut value = || args.next().ok_or_else(|| BatchError::Args(format!("{flag} needs a value")));
            match flag.as_str() {
                "--target" | "-t" => opts.target = Some(Host(value()?)),
                "--fraction" | "-f" => {
                    let f: f64 = value()?.parse().map_err(|e| BatchError::Args(format!("{e}")))?;
                    if !(0.0..1.0).contains(&f) {
                        return Err(BatchError::Args(format!("fraction {f} out of range")));
                    }
                    opts.fraction = Some(f);
                }
                "--spacer" => {
                    let ms: u64 = value()?.parse().map_err(|e| BatchError::Args(format!("{e}")))?;
                    opts.spacer = Duration::from_millis(ms);
                }
                "--dry-run" => opts.dry_run = true,
                other => return Err(BatchError::Args(format!("unknown flag {other}"))),
            }
        }
        Ok(opts)
    }
}

/// Thread math for one batch at the given hack fraction, assuming a prepped target.
fn plan(ns: &NS, target: &Host, fraction: f64, cores: u32) -> Result<BatchPlan> {
    let player = ns.get_player();
    let base = ns.get_server(target);
    let max = base.money_max.filter(|&m| m > 0.0).ok_or_else(|| BatchError::NoMoney(target.clone()))?;
    let prepped = Server {
        money_available: Some(max),
        hack_difficulty: base.min_difficulty,
        ..base
    };
    let f = &ns.formulas.hacking;

    let per_thread = f.hack_percent(prepped.clone(), player.clone());
    let mut threads = Threads::default();
    threads[Step::Hack] = ((fraction / per_thread).floor() as u32).max(1);
    let stolen = (threads[Step::Hack] as f64 * per_thread).min(1.0);

    let after_hack = Server {
        money_available: Some(max * (1.0 - stolen)),
        ..prepped.clone()
    };
    threads[Step::Grow] = f
        .grow_threads(after_hack, player.clone(), max, cores as f64)
        .ceil() as u32;

    let weaken_per = ns.weaken_analyze(1, cores as f64);
    threads[Step::Weaken1] = (threads[Step::Hack] as f64 * HACK_SEC / weaken_per).ceil() as u32;
    threads[Step::Weaken2] = (threads[Step::Grow] as f64 * GROW_SEC / weaken_per).ceil() as u32;

    let ms = |t: f64| Duration::from_secs_f64(t / 1000.0);
    let timings = Timings {
        hack: ms(f.hack_time(prepped.clone(), player.clone())),
        grow: ms(f.grow_time(prepped.clone(), player.clone())),
        weaken: ms(f.weaken_time(prepped.clone(), player.clone())),
    };
    let chance = f.hack_chance(prepped, player);

    Ok(BatchPlan {
        target: target.clone(),
        fraction: stolen,
        threads,
        timings,
        money_per_batch: max * stolen * chance,
    })
}

/// Largest x in [lo, hi] with `ok(x)`, assuming `ok` is monotone (true then false).
fn bisect(mut lo: f64, mut hi: f64, iterations: usize, ok: impl Fn(f64) -> bool) -> Option<f64> {
    if !ok(lo) {
        return None;
    }
    for _ in 0..iterations {
        let mid = (lo + hi) / 2.0;
        if ok(mid) { lo = mid } else { hi = mid }
    }
    Some(lo)
}

/// Money per second per GB for the target, with a sensible plan for the pool we have.
fn score(ns: &NS, host: &Host) -> Option<f64> {
    let server = ns.get_server(host);
    let required = server.required_hacking_skill?;
    if server.purchased_by_player || required > ns.get_hacking_level() / 2.0 {
        return None;
    }
    let plan = plan(ns, host, 0.1, 1).ok()?;
    let ram = plan.ram(ns.get_script_ram(WORKER));
    Some(plan.money_per_batch / plan.timings.weaken.as_secs_f64() / ram)
}

fn choose_target(ns: &NS) -> Option<Host> {
    let candidates = net::all_hosts(ns)
        .into_iter()
        .filter(|h| try_root(ns, h).unwrap_or(false));
    let best = top_k(candidates, 5, |h| score(ns, h).unwrap_or(f64::NEG_INFINITY));
    for (s, h) in &best {
        ns.print(&format!("  candidate {h:<20} {s:>12.3}"));
    }
    best.into_iter()
        .find(|(s, _)| s.is_finite())
        .map(|(_, h)| h)
}

/// Grows and weakens until the target is at min security and max money.
async fn prep(ns: &NS, target: &Host, pids: &Rc<RefCell<Vec<f64>>>) -> Result<()> {
    let ram = ns.get_script_ram(WORKER);
    loop {
        let server = ns.get_server(target);
        let (Some(sec), Some(min_sec), Some(money), Some(max)) = (
            server.hack_difficulty,
            server.min_difficulty,
            server.money_available,
            server.money_max,
        ) else {
            return Err(BatchError::NoMoney(target.clone()));
        };
        let sec_gap = sec - min_sec;
        if sec_gap < 0.01 && money >= max * 0.999 {
            ns.print(&format!("prepped {target}"));
            return Ok(());
        }
        let mut pool = RamPool::scan(ns);
        let weaken_per = ns.weaken_analyze(1, 1);
        let w = (sec_gap / weaken_per).ceil() as u32;
        let g = if money < max {
            ns.growth_analyze(target, max / money.max(1.0), 1).ceil() as u32
        } else {
            0
        };
        let w2 = (g as f64 * GROW_SEC / weaken_per).ceil() as u32;
        let mut launched = 0;
        for (op, count) in [(Op::Weaken, w), (Op::Grow, g), (Op::Weaken, w2)] {
            let fit = ((pool.total_free() / ram).floor() as u32).min(count);
            let Some(chunks) = pool.reserve(fit, ram, true) else { continue };
            for (host, n) in chunks {
                launched += n;
                let pid = spawn(ns, &host, op, target, Duration::ZERO, 0, n);
                pids.borrow_mut().extend(pid);
            }
        }
        let wait = ns.get_weaken_time(target) + 100.0;
        ns.print(&format!(
            "prep {target}: sec +{sec_gap:.2}, money {:.1}%, {launched} threads, waiting {}",
            money / max * 100.0,
            ns.format.time(wait)
        ));
        ns.sleep(wait).await;
    }
}

fn spawn(ns: &NS, host: &Host, op: Op, target: &Host, delay: Duration, batch: u32, threads: u32) -> Option<f64> {
    if !ns.file_exists(WORKER, host) {
        ns.scp(WORKER, host, "home");
    }
    let pid = ns.exec(
        WORKER,
        host,
        RunOptions {
            threads: Some(threads as f64),
            temporary: Some(true),
            ..Default::default()
        },
        format!("{op:?}"),
        target.to_string(),
        delay.as_millis() as f64,
        batch,
        REPORT_PORT,
    );
    (pid > 0.0).then_some(pid)
}

/// Reads every pending report off the port.
fn drain_reports(ns: &NS) -> impl Iterator<Item = Report> + '_ {
    let port = ns.get_port_handle(REPORT_PORT as f64);
    std::iter::from_fn(move || {
        if port.empty() {
            return None;
        }
        Some(port.read())
    })
    .filter_map(|v| v.as_string())
    .filter_map(|s| serde_json::from_str::<Report>(&s).ok())
}

#[derive(Default)]
struct Stats {
    launched: u32,
    finished: HashMap<Op, u32>,
    stolen: f64,
    out_of_order: u32,
    last_finish: BTreeMap<u32, (Op, f64)>,
}

impl Stats {
    fn record(&mut self, r: Report) {
        *self.finished.entry(r.op).or_default() += 1;
        if r.op == Op::Hack {
            self.stolen += r.result;
        }
        if let Some((prev, at)) = self.last_finish.insert(r.batch_id, (r.op, r.finished_at)) {
            // a hack landing after the batch's grow means security wasn't at minimum
            if prev == Op::Grow && r.op == Op::Hack && r.finished_at > at {
                self.out_of_order += 1;
            }
        }
        // keep the map from growing without bound
        while self.last_finish.len() > MAX_BATCHES {
            self.last_finish.pop_first();
        }
    }
}

pub async fn main(ns: NS) {
    quiet!(ns, "scan", "sleep", "exec", "scp", "getServerMaxRam", "getServerUsedRam");
    ns.ui.open_tail();

    let opts = match Options::try_from(ns.args.as_slice()) {
        Ok(o) => o,
        Err(e) => {
            ns.tprint(&format!("ERROR {e}"));
            return;
        }
    };

    if let Err(e) = run(&ns, opts).await {
        ns.tprint(&format!("ERROR batcher stopped: {e}"));
        let mut source = std::error::Error::source(&e);
        while let Some(cause) = source {
            ns.tprint(&format!("  caused by: {cause}"));
            source = cause.source();
        }
    }
}

async fn run(ns: &NS, opts: Options) -> Result<()> {
    let pids: Rc<RefCell<Vec<f64>>> = Rc::default();
    {
        let pids = Rc::clone(&pids);
        let ns2 = ns.clone();
        ns.at_exit(move || {
            for pid in pids.borrow().iter() {
                ns2.kill(*pid);
            }
        });
    }

    let target = match opts.target {
        Some(t) => {
            try_root(ns, &t).map_err(|source| BatchError::Root { host: t.clone(), source })?;
            t
        }
        None => choose_target(ns).ok_or_else(|| BatchError::Args("no viable target".into()))?,
    };
    ns.print(&format!("target: {target}"));

    prep(ns, &target, &pids).await?;

    let script_ram = ns.get_script_ram(WORKER);
    let mut stats = Stats::default();
    let mut batch_id: u32 = 0;

    'cycle: loop {
        let pool = RamPool::scan(ns);
        let cores = pool.largest().map_or(1, |s| s.cores);
        let free = pool.total_free();

        // Biggest hack fraction whose batch fits in the largest slot.
        let largest = pool.largest().map_or(0.0, |s| s.free);
        let fraction = match opts.fraction {
            Some(f) => f,
            None => bisect(0.001, 0.9, 20, |f| {
                plan(ns, &target, f, cores).is_ok_and(|p| p.ram(script_ram) <= largest)
            })
            .ok_or(BatchError::NoRam { needed: 0.0, free })?,
        };
        let plan = plan(ns, &target, fraction, cores)?;
        let batch_ram = plan.ram(script_ram);
        if batch_ram > free {
            return Err(BatchError::NoRam { needed: batch_ram, free });
        }
        ns.print(&format!("{plan}"));

        // Launch as many batches as fit, each `4 * spacer` after the previous one.
        let window = plan.timings.weaken;
        let by_time = (window.as_millis() / (opts.spacer * 4).as_millis().max(1)) as usize;
        let by_ram = (free / batch_ram).floor() as usize;
        let count = by_time.min(by_ram).min(MAX_BATCHES);
        if opts.dry_run {
            ns.tprint(&format!("dry run: {count} batches of {plan}"));
            return Ok(());
        }

        let mut pool = pool;
        let mut started = 0;
        'batch: for _ in 0..count {
            let mut placed: Vec<(Step, Vec<(Host, u32)>)> = Vec::with_capacity(4);
            for step in Step::ORDER {
                match pool.reserve(plan.threads[step], script_ram, step.splittable()) {
                    Some(chunks) => placed.push((step, chunks)),
                    None => break 'batch,
                }
            }
            batch_id = batch_id.wrapping_add(1);
            let delays: HashMap<Step, Duration> = plan.delays(opts.spacer).collect();
            for (step, chunks) in placed {
                for (host, n) in chunks {
                    if let Some(pid) = spawn(ns, &host, step.op(), &target, delays[&step], batch_id, n) {
                        pids.borrow_mut().push(pid);
                    }
                }
            }
            started += 1;
            stats.launched += 1;
            // Keep the launch cadence steady.
            ns.sleep((opts.spacer * 4).as_millis() as f64).await;

            for report in drain_reports(ns) {
                stats.record(report);
            }
        }

        ns.print(&format!(
            "launched {started} batches ({} total), stolen ${}, {} out of order",
            stats.launched,
            ns.format.number(stats.stolen),
            stats.out_of_order,
        ));

        // Wait for the last batch to land, then forget finished pids.
        ns.sleep((window + opts.spacer * 8).as_millis() as f64).await;
        pids.borrow_mut().retain(|&pid| ns.is_running(pid));
        for report in drain_reports(ns) {
            stats.record(report);
        }

        // Re-prep if a desync left the target off its minimum.
        let server = ns.get_server(&target);
        if server.hack_difficulty > server.min_difficulty.map(|m| m + 0.5) {
            ns.print("desync detected, re-prepping");
            prep(ns, &target, &pids).await?;
            continue 'cycle;
        }
    }
}
