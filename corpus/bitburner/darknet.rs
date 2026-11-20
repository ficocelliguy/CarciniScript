//! Darknet crawler. Runs on a darknet server, probes its neighbours, cracks each one by
//! model, saves passwords to a shared vault, then copies itself over and keeps spreading.
//! Also keeps the server busy with the helper tasks in `dn_tasks.rs`.
//!
//! run darknet.rs [--once] [--verbose]

mod dn_models;

use dn_models::{Candidates, Congruence, Hill, Model};

use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::fmt;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use thiserror::Error;

const SELF: &str = "darknet.rs";
const FILES: [&str; 3] = [SELF, "dn_models.rs", "dn_tasks.rs"];
const VAULT_HOST: &str = "darkweb";
const VAULT_FILE: &str = "dn/vault.json";
const MAX_GUESSES: usize = 400;
const PERMUTATION_LIMIT: usize = 40_320; // 8!

#[derive(Debug, Error)]
enum DnError {
    #[error("{host} is not directly connected")]
    NotConnected { host: String },
    #[error("{host} went offline")]
    Offline { host: String },
    #[error("need {need} charisma for {host}")]
    Charisma { host: String, need: i32 },
    #[error("unsupported model {0}")]
    Unsupported(Model),
    #[error(transparent)]
    UnknownModel(#[from] dn_models::UnknownModel),
    #[error("gave up on {host} after {tries} guesses")]
    Exhausted { host: String, tries: usize },
    #[error("bad feedback from {host}: {detail}")]
    Feedback { host: String, detail: String },
    #[error("vault: {0}")]
    Vault(#[from] serde_json::Error),
}

type Result<T> = std::result::Result<T, DnError>;

/// Passwords we've found, shared through a file on `darkweb` so every copy of the
/// crawler benefits.
#[derive(Debug, Default, Serialize, Deserialize)]
struct Vault {
    passwords: BTreeMap<String, String>,
    #[serde(default)]
    failures: HashMap<String, u32>,
}

impl Vault {
    fn load(ns: &NS) -> Result<Vault> {
        let here = ns.get_hostname();
        if here != VAULT_HOST {
            ns.scp(VAULT_FILE, &here, VAULT_HOST);
        }
        let text = ns.read(VAULT_FILE);
        if text.is_empty() {
            return Ok(Vault::default());
        }
        Ok(serde_json::from_str(&text)?)
    }

    /// Merges with what's on disk now (another crawler may have written since we loaded),
    /// then writes back here and on `darkweb`.
    fn save(&mut self, ns: &NS) -> Result<()> {
        if let Ok(theirs) = Vault::load(ns) {
            for (host, pw) in theirs.passwords {
                self.passwords.entry(host).or_insert(pw);
            }
        }
        ns.write(VAULT_FILE, &serde_json::to_string_pretty(self)?, "w");
        ns.scp(VAULT_FILE, VAULT_HOST);
        Ok(())
    }

    fn get(&self, host: &str) -> Option<&str> {
        self.passwords.get(host).map(String::as_str)
    }
}

/// One heartbleed log line.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LogEntry {
    password_attempted: Option<String>,
    #[serde(default)]
    message: String,
    #[serde(default)]
    data: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
enum Feedback {
    Accepted,
    Rejected { message: String, data: Option<String> },
}

impl Feedback {
    fn data(&self) -> Option<&str> {
        match self {
            Feedback::Rejected { data, .. } => data.as_deref(),
            Feedback::Accepted => None,
        }
    }
}

/// An authentication session against one neighbour.
struct Session<'a> {
    ns: &'a NS,
    host: String,
    details: DarknetServerDetails,
    vault: &'a mut Vault,
    tries: usize,
    verbose: bool,
}

impl<'a> Session<'a> {
    async fn attempt(&mut self, password: &str) -> Result<Feedback> {
        self.tries += 1;
        if self.tries > MAX_GUESSES {
            return Err(DnError::Exhausted { host: self.host.clone(), tries: self.tries });
        }
        let result = self.ns.dnet.authenticate(&self.host, password).await;
        if self.verbose {
            self.ns.print(&format!("{} <- {password:?}: {}", self.host, result.message));
        }
        let codes = &self.ns.enums.darknet_response_code;
        match result.code {
            c if c == codes.success => {
                self.vault.passwords.insert(self.host.clone(), password.to_owned());
                self.vault.save(self.ns)?;
                Ok(Feedback::Accepted)
            }
            c if c == codes.direct_connection_required => {
                Err(DnError::NotConnected { host: self.host.clone() })
            }
            c if c == codes.not_found || c == codes.service_unavailable => {
                Err(DnError::Offline { host: self.host.clone() })
            }
            c if c == codes.not_enough_charisma => Err(DnError::Charisma {
                host: self.host.clone(),
                need: self.details.required_charisma_skill,
            }),
            _ => Ok(Feedback::Rejected {
                message: result.message,
                data: result.data.and_then(|d| d.as_string()),
            }),
        }
    }

    /// Tries each candidate until one works.
    async fn first_success<I>(&mut self, candidates: I) -> Result<Option<String>>
    where
        I: IntoIterator,
        I::Item: AsRef<str>,
    {
        for candidate in candidates {
            let pw = candidate.as_ref();
            if self.attempt(pw).await? == Feedback::Accepted {
                return Ok(Some(pw.to_owned()));
            }
        }
        Ok(None)
    }

    /// Reads the server's recent logs (without consuming them) for richer feedback.
    async fn logs(&self, count: u32) -> Vec<LogEntry> {
        let opts = HeartbleedOptions {
            peek: Some(true),
            logs_to_capture: Some(count as i32),
            ..Default::default()
        };
        let result = self.ns.dnet.heartbleed(&self.host, opts).await;
        if !result.success {
            return Vec::new();
        }
        result
            .logs
            .iter()
            .filter_map(|line| serde_json::from_str::<LogEntry>(line).ok())
            .collect()
    }

    fn len(&self) -> usize {
        self.details.password_length as usize
    }
}

impl fmt::Debug for Session<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Session")
            .field("host", &self.host)
            .field("model", &self.details.model_id)
            .field("tries", &self.tries)
            .finish_non_exhaustive()
    }
}

/// Cracks `s.host` according to its model. `Ok(None)` means "not this time".
async fn crack(s: &mut Session<'_>) -> Result<Option<String>> {
    let model: Model = s.details.model_id.parse()?;
    let hint = s.details.password_hint.clone();
    let data = s.details.data.clone();

    // Models we can decode outright.
    let decoded: Option<String> = match model {
        Model::NoPassword => Some(String::new()),
        Model::EchoVuln => Some(dn_models::echo_password(&hint)),
        Model::Captcha => Some(dn_models::captcha(&data)),
        Model::RomanNumeral => dn_models::parse_roman(&data).map(|n| n.to_string()),
        Model::ConvertToBase10 => dn_models::convert_to_base10(&data),
        Model::ParsedExpression => dn_models::evaluate(&data).ok().map(dn_models::format_number),
        Model::LargestPrimeFactor => data
            .parse::<u64>()
            .ok()
            .and_then(dn_models::largest_prime_factor)
            .map(|p| p.to_string()),
        Model::BinaryEncodedFeedback => dn_models::decode_binary(&data),
        Model::EncryptedPassword => dn_models::decode_xor(&data),
        _ => None,
    };
    if let Some(pw) = decoded {
        return s.first_success([pw]).await;
    }

    match model {
        Model::DefaultPassword => s.first_success(dn_models::DEFAULT_PASSWORDS).await,
        Model::DogNames => s.first_success(dn_models::DOG_NAMES).await,
        Model::CommonPasswordDictionary => s.first_success(dn_models::common_passwords()).await,
        Model::EuCountryDictionary => s.first_success(dn_models::EU_COUNTRIES.iter().copied()).await,
        Model::SortedEchoVuln => {
            let digits = data.split(';').next().unwrap_or_default().to_owned();
            s.first_success(dn_models::permutations(&digits, PERMUTATION_LIMIT)).await
        }
        Model::GuessNumber => guess_number(s).await,
        Model::Yesnt => yesnt(s).await,
        Model::MastermindHint => mastermind(s).await,
        Model::SpiceLevel => per_position(s, dn_models::spice_count).await,
        Model::TimingAttack => timing_attack(s).await,
        Model::BufferOverflow => {
            let n = s.len();
            s.first_success(["■".repeat(n * 2)]).await
        }
        Model::DivisibilityTest => divisibility(s).await,
        Model::TripleModulo => triple_modulo(s).await,
        Model::GlobalMaxima => king_of_the_hill(s).await,
        Model::PacketSniffer => {
            let entries = s.logs(8).await;
            let host = s.host.clone();
            let found: Vec<String> = entries
                .iter()
                .filter_map(|e| e.data.as_deref())
                .filter_map(|d| dn_models::sniff_password(d, &host))
                .map(str::to_owned)
                .collect();
            s.first_success(found).await
        }
        Model::Labyrinth => labyrinth(s).await,
        other => Err(DnError::Unsupported(other)),
    }
}

/// Binary search on "Higher" / "Lower".
async fn guess_number(s: &mut Session<'_>) -> Result<Option<String>> {
    let max: u64 = s
        .details
        .password_hint
        .rsplit(' ')
        .next()
        .and_then(|n| n.parse().ok())
        .unwrap_or(u64::from(u32::MAX));
    let (mut lo, mut hi) = (0u64, max);
    while lo <= hi {
        let mid = lo + (hi - lo) / 2;
        let fb = s.attempt(&mid.to_string()).await?;
        match fb.data() {
            None if fb == Feedback::Accepted => return Ok(Some(mid.to_string())),
            Some("Higher") => lo = mid + 1,
            Some("Lower") => hi = mid.checked_sub(1).ok_or_else(|| DnError::Feedback {
                host: s.host.clone(),
                detail: "lower than 0".into(),
            })?,
            other => {
                return Err(DnError::Feedback {
                    host: s.host.clone(),
                    detail: format!("{other:?}"),
                });
            }
        }
    }
    Ok(None)
}

/// "yes,yesn't,yes..." per position: fix each position one symbol at a time.
async fn yesnt(s: &mut Session<'_>) -> Result<Option<String>> {
    let alphabet = dn_models::alphabet(&s.details.password_format);
    let n = s.len();
    let mut known: Vec<Option<char>> = vec![None; n];
    for &c in &alphabet {
        let guess: String = known.iter().map(|k| k.unwrap_or(c)).collect();
        let fb = s.attempt(&guess).await?;
        let Some(data) = fb.data() else {
            return Ok(Some(guess));
        };
        for (slot, verdict) in known.iter_mut().zip(data.split(',')) {
            if slot.is_none() && verdict == "yes" {
                *slot = Some(c);
            }
        }
        if let Some(pw) = known.iter().copied().collect::<Option<String>>() {
            return s.first_success([pw]).await;
        }
    }
    Ok(None)
}

/// Finds each position by probing one symbol at a time and counting exact matches.
async fn per_position<F>(s: &mut Session<'_>, exact_from: F) -> Result<Option<String>>
where
    F: Fn(&str) -> usize,
{
    let alphabet = dn_models::alphabet(&s.details.password_format);
    let n = s.len();
    // How many of each symbol appear in the password.
    let mut counts: Vec<(char, usize)> = Vec::new();
    for &c in &alphabet {
        let fb = s.attempt(&c.to_string().repeat(n)).await?;
        let Some(data) = fb.data() else {
            return Ok(Some(c.to_string().repeat(n)));
        };
        match exact_from(data) {
            0 => {}
            k => counts.push((c, k)),
        }
        if counts.iter().map(|&(_, k)| k).sum::<usize>() == n {
            break;
        }
    }
    let filler = alphabet
        .iter()
        .copied()
        .find(|c| !counts.iter().any(|&(k, _)| k == *c))
        .unwrap_or('?');
    let mut password = vec![filler; n];
    for (c, mut remaining) in counts {
        for i in 0..n {
            if remaining == 0 {
                break;
            }
            if password[i] != filler {
                continue;
            }
            let mut probe = vec![filler; n];
            probe[i] = c;
            let probe: String = probe.into_iter().collect();
            let fb = s.attempt(&probe).await?;
            if fb.data().map(&exact_from) == Some(1) {
                password[i] = c;
                remaining -= 1;
            }
        }
    }
    let pw: String = password.into_iter().collect();
    s.first_success([pw]).await
}

/// Mastermind: learn the digit multiset, then filter permutations by the hints.
async fn mastermind(s: &mut Session<'_>) -> Result<Option<String>> {
    let n = s.len();
    let mut counts = BTreeMap::new();
    for d in 0u8..10 {
        let guess = char::from(b'0' + d).to_string().repeat(n);
        let fb = s.attempt(&guess).await?;
        let Some((exact, _)) = fb.data().and_then(dn_models::parse_mastermind) else {
            if fb == Feedback::Accepted {
                return Ok(Some(guess));
            }
            continue;
        };
        if exact > 0 {
            counts.insert(d, exact);
        }
    }
    let digits = dn_models::digits_from_counts(&counts);
    let mut pool = Candidates::new(dn_models::permutations(&digits, PERMUTATION_LIMIT));
    while let Some(guess) = pool.best().cloned() {
        let fb = s.attempt(&guess).await?;
        let Some(score) = fb.data().and_then(dn_models::parse_mastermind) else {
            return Ok(Some(guess));
        };
        pool.retain_consistent(&guess, |candidate, g| dn_models::mastermind_score(candidate, g) == score);
    }
    Ok(None)
}

/// The mismatch index is in the hint text: "(3)".
async fn timing_attack(s: &mut Session<'_>) -> Result<Option<String>> {
    let alphabet = dn_models::alphabet(&s.details.password_format);
    let n = s.len();
    let mut prefix = String::with_capacity(n);
    let mismatch_at = |message: &str| -> Option<usize> {
        let open = message.rfind('(')?;
        message[open + 1..].trim_end_matches(')').parse().ok()
    };
    'position: while prefix.chars().count() < n {
        for &c in &alphabet {
            let mut guess = prefix.clone();
            guess.push(c);
            let padded = format!("{guess:0<n$}");
            match s.attempt(&padded).await? {
                Feedback::Accepted => return Ok(Some(padded)),
                Feedback::Rejected { message, .. } => {
                    if mismatch_at(&message).is_some_and(|i| i > prefix.chars().count()) {
                        prefix = guess;
                        continue 'position;
                    }
                }
            }
        }
        return Ok(None);
    }
    s.first_success([prefix]).await
}

/// "Password IS divisible by n": find the power of each prime that divides it.
async fn divisibility(s: &mut Session<'_>) -> Result<Option<String>> {
    let limit = 10u64.saturating_pow(s.len() as u32);
    let primes = (2u64..).filter(|&k| (2..k).take_while(|d| d * d <= k).all(|d| k % d != 0));
    let mut value = 1u64;
    for p in primes.take_while(|&p| p <= 1_000) {
        let mut power = 1u64;
        while let Some(next) = power.checked_mul(p).filter(|&q| value.saturating_mul(q) <= limit) {
            if s.attempt(&next.to_string()).await?.data() != Some("true") {
                break;
            }
            power = next;
        }
        value *= power;
    }
    s.first_success([value.to_string()]).await
}

/// BigMo%od: learn `password % n` for coprime `n` up to 32, then solve with the CRT.
async fn triple_modulo(s: &mut Session<'_>) -> Result<Option<String>> {
    let mut known = Congruence::ANY;
    for n in dn_models::CRT_PROBES {
        let probe = n.to_string();
        let fb = s.attempt(&probe).await?;
        if fb == Feedback::Accepted {
            return Ok(Some(probe));
        }
        let residue: u64 = fb
            .data()
            .and_then(|d| d.trim().parse().ok())
            .ok_or_else(|| DnError::Feedback { host: s.host.clone(), detail: format!("{fb:?}") })?;
        known = known.and(residue, n).ok_or_else(|| DnError::Feedback {
            host: s.host.clone(),
            detail: format!("{residue} (mod {n}) contradicts {known:?}"),
        })?;
    }
    let digits = s.len() as u32;
    s.first_success(known.with_digits(digits).map(|x| x.to_string())).await
}

/// KingOfTheHill: scan at half a hill's width, so the best sample is on the tallest hill
/// rather than a side one, then jump to where that hill's shape puts its top.
async fn king_of_the_hill(s: &mut Session<'_>) -> Result<Option<String>> {
    /// The altitude at `x`, or `None` if `x` was the password.
    async fn altitude(s: &mut Session<'_>, x: u64) -> Result<Option<f64>> {
        match s.attempt(&x.to_string()).await? {
            Feedback::Accepted => Ok(None),
            Feedback::Rejected { data, message } => data
                .as_deref()
                .and_then(|d| d.parse().ok())
                .map(Some)
                .ok_or(DnError::Feedback { host: s.host.clone(), detail: message }),
        }
    }

    let digits = s.len() as u32;
    let hill = Hill::for_digits(digits);
    let lo = if digits <= 1 { 0 } else { 10u64.pow(digits - 1) };
    let hi = 10u64.pow(digits) - 1;
    let step = ((hill.width / 2.0) as usize).max(1);

    let mut seen: BTreeMap<u64, f64> = BTreeMap::new();
    let mut next: Vec<u64> = (lo..=hi).step_by(step).collect();
    while !next.is_empty() {
        for x in next.drain(..) {
            match altitude(s, x).await? {
                None => return Ok(Some(x.to_string())),
                Some(a) => {
                    seen.insert(x, a);
                }
            }
        }
        let Some((&best, &top)) = seen.iter().max_by(|a, b| a.1.total_cmp(b.1)) else {
            break;
        };
        next = hill
            .centres(best as f64, top)
            .into_iter()
            .flatten()
            .map(|c| c.round().clamp(lo as f64, hi as f64) as u64)
            .chain([best.saturating_sub(1).max(lo), (best + 1).min(hi)])
            .filter(|x| !seen.contains_key(x))
            .collect();
        next.dedup();
    }
    Ok(None)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Dir {
    North,
    East,
    South,
    West,
}

impl Dir {
    const ALL: [Dir; 4] = [Dir::North, Dir::East, Dir::South, Dir::West];

    fn opposite(self) -> Dir {
        Dir::ALL[(self as usize + 2) % 4]
    }

    fn word(self) -> &'static str {
        match self {
            Dir::North => "north",
            Dir::East => "east",
            Dir::South => "south",
            Dir::West => "west",
        }
    }

    /// Row/column of the neighbouring cell in a 3×3 surroundings view.
    fn cell(self) -> (usize, usize) {
        match self {
            Dir::North => (0, 1),
            Dir::East => (1, 2),
            Dir::South => (2, 1),
            Dir::West => (1, 0),
        }
    }
}

/// Which directions are open in a surroundings picture like "█ █\n @ \n███".
fn open_dirs(view: &str) -> Vec<Dir> {
    let grid: Vec<Vec<char>> = view.lines().map(|l| l.chars().collect()).collect();
    Dir::ALL
        .into_iter()
        .filter(|d| {
            let (r, c) = d.cell();
            grid.get(r).and_then(|row| row.get(c)) == Some(&' ')
        })
        .collect()
}

/// Depth-first walk of the labyrinth with an explicit stack, backtracking by moving
/// the opposite way.
async fn labyrinth(s: &mut Session<'_>) -> Result<Option<String>> {
    type Pos = (i32, i32);
    let step = |(x, y): Pos, d: Dir| -> Pos {
        match d {
            Dir::North => (x, y - 1),
            Dir::East => (x + 1, y),
            Dir::South => (x, y + 1),
            Dir::West => (x - 1, y),
        }
    };
    let mut pos: Pos = (0, 0);
    let mut visited: HashSet<Pos> = HashSet::from([pos]);
    // (direction we came in by, directions still to try here)
    let mut stack: Vec<(Option<Dir>, VecDeque<Dir>)> = Vec::new();

    let first = s.attempt("look").await?;
    let view = first.data().unwrap_or_default().to_owned();
    stack.push((None, open_dirs(&view).into()));

    while let Some((came_by, todo)) = stack.last_mut() {
        let Some(dir) = todo.pop_front() else {
            // dead end: step back
            let back = came_by.map(Dir::opposite);
            stack.pop();
            if let Some(b) = back {
                s.attempt(&format!("go {}", b.word())).await?;
                pos = step(pos, b);
            }
            continue;
        };
        let next = step(pos, dir);
        if !visited.insert(next) {
            continue;
        }
        match s.attempt(&format!("go {}", dir.word())).await? {
            Feedback::Accepted => {
                // the final response carries the lab's password, which `attempt` can't see;
                // the vault entry is the move itself, so look it up from the logs
                let logs = s.logs(1).await;
                let pw = logs.into_iter().find_map(|e| e.data);
                return Ok(pw);
            }
            Feedback::Rejected { message, data } if message.starts_with("You have moved") => {
                pos = next;
                let dirs = open_dirs(data.as_deref().unwrap_or_default());
                let todo: VecDeque<Dir> = dirs.into_iter().filter(|d| *d != dir.opposite()).collect();
                stack.push((Some(dir), todo));
            }
            Feedback::Rejected { .. } => {}
        }
    }
    Ok(None)
}

// ---------------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Task {
    Stasis,
    Clear,
    Caches,
    Phish,
}

impl Task {
    fn arg(self) -> &'static str {
        match self {
            Task::Stasis => "stasis",
            Task::Clear => "clear",
            Task::Caches => "caches",
            Task::Phish => "phish",
        }
    }
}

/// The most useful helper task for this server right now, if any.
fn next_task(ns: &NS) -> Option<Task> {
    let here = ns.get_hostname();
    let near_lab = ns.dnet.probe().iter().any(|s| s.contains("l4byr1nth"));
    let linked = ns.dnet.get_stasis_linked_servers().contains(&here);
    [
        (Task::Stasis, near_lab && !linked),
        (Task::Clear, ns.dnet.get_blocked_ram() > 0.0),
        (Task::Caches, !ns.ls(&here, ".cache").is_empty()),
        (Task::Phish, true),
    ]
    .into_iter()
    .find_map(|(task, wanted)| wanted.then_some(task))
}

fn ensure_task(ns: &NS) {
    let already = ns
        .ps()
        .iter()
        .any(|p| p.filename == "dn_tasks.rs");
    if already {
        return;
    }
    if let Some(task) = next_task(ns) {
        ns.run("dn_tasks.rs", RunOptions { temporary: Some(true), ..Default::default() }, task.arg());
    }
}

/// Copies the crawler over and starts it, unless it's already running there.
fn spread(ns: &NS, host: &str) -> bool {
    let here = ns.get_hostname();
    if !FILES.iter().all(|f| ns.scp(*f, host, &here)) {
        return false;
    }
    let opts = RunOptions {
        prevent_duplicates: Some(true),
        temporary: Some(true),
        ..Default::default()
    };
    ns.exec(SELF, host, opts) > 0
}

#[derive(Default)]
struct Tally {
    cracked: u32,
    known: u32,
    failed: BTreeMap<String, u32>,
}

impl fmt::Display for Tally {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "cracked {}, reused {}", self.cracked, self.known)?;
        if !self.failed.is_empty() {
            let reasons = self
                .failed
                .iter()
                .map(|(why, n)| format!("{why}×{n}"))
                .collect::<Vec<_>>()
                .join(", ");
            write!(f, ", failed: {reasons}")?;
        }
        Ok(())
    }
}

pub async fn main(ns: NS) {
    ns.disable_log("ALL");
    let args: Vec<String> = ns.args.iter().filter_map(|a| a.as_string()).collect();
    let once = args.iter().any(|a| a == "--once");
    let verbose = args.iter().any(|a| a == "--verbose");

    let mut vault = match Vault::load(&ns) {
        Ok(v) => v,
        Err(e) => {
            ns.print(&format!("WARN vault unreadable ({e}); starting fresh"));
            Vault::default()
        }
    };

    loop {
        ensure_task(&ns);
        let mut tally = Tally::default();

        for host in ns.dnet.probe() {
            let details = ns.dnet.get_server_details(&host);
            if !details.is_connected_to_current_server || !details.is_online {
                continue;
            }

            // A saved password only needs a session.
            let known = vault.get(&host).map(str::to_owned);
            let authed = match known {
                Some(pw) if ns.dnet.connect_to_session(&host, &pw).success => {
                    tally.known += 1;
                    true
                }
                _ => {
                    let mut session = Session {
                        ns: &ns,
                        host: host.clone(),
                        details,
                        vault: &mut vault,
                        tries: 0,
                        verbose,
                    };
                    match crack(&mut session).await {
                        Ok(Some(_)) => {
                            tally.cracked += 1;
                            true
                        }
                        Ok(None) => false,
                        Err(e) => {
                            if verbose {
                                ns.print(&format!("{host}: {e}"));
                            }
                            let key = match &e {
                                DnError::Unsupported(m) => format!("{m:?}"),
                                other => other.to_string().split(':').next().unwrap_or("?").to_owned(),
                            };
                            *tally.failed.entry(key).or_insert(0) += 1;
                            *vault.failures.entry(host.clone()).or_default() += 1;
                            false
                        }
                    }
                }
            };

            if authed && !spread(&ns, &host) && verbose {
                ns.print(&format!("could not start on {host}"));
            }
        }

        ns.print(&format!("[{}] {tally}", ns.get_hostname()));
        if once {
            break;
        }
        // Mutations rearrange the net; wait for one rather than polling blindly.
        ns.dnet.next_mutation().await;
        ns.sleep(Duration::from_millis(500).as_millis() as f64).await;
    }
}
