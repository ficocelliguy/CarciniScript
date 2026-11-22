//! Rust-source preludes compiled in after a program that uses them: `std::time` (`Duration`,
//! `Instant`, `SystemTime`), the lazy and once cells (`std::sync::{LazyLock, OnceLock}`,
//! `std::cell::{LazyCell, OnceCell}`, the bundled `once_cell`'s `Lazy` / `OnceCell`), the locks,
//! atomics and channels (`std::sync::{Mutex, RwLock, mpsc}`, `thread_local!`) and the
//! bundled `either`, `serde_json` (`Value`, `Number`) and `regex` (`Regex`, `Match`,
//! `Captures`). Paths like `std::time::Duration` and `use std::time::{..}` resolve to the
//! prelude's top-level items; positions in the program are unchanged.

const TIME: &str = include_str!("time_prelude.rs");
const SYNC: &str = include_str!("sync_prelude.rs");
const LOCK: &str = include_str!("lock_prelude.rs");
const EITHER: &str = include_str!("either_prelude.rs");
const SERDE_JSON: &str = include_str!("serde_json_prelude.rs");
const REGEX: &str = include_str!("regex_prelude.rs");
const COW: &str = include_str!("cow_prelude.rs");
const IO: &str = include_str!("io_prelude.rs");
const LOG: &str = include_str!("log_prelude.rs");
const NUM: &str = include_str!("num_prelude.rs");
const TOKIO: &str = include_str!("tokio_prelude.rs");
const RAND_DISTR: &str = include_str!("rand_distr_prelude.rs");
const CHRONO: &str = include_str!("chrono_prelude.rs");
const CLAP: &str = include_str!("clap_prelude.rs");
const ENTRY: &str = include_str!("entry_prelude.rs");
const REACT: &str = include_str!("react_prelude.rs");
const TASK: &str = include_str!("task_prelude.rs");
const OS: &str = include_str!("os_prelude.rs");
const NET: &str = include_str!("net_prelude.rs");
const ORDERED_FLOAT: &str = include_str!("ordered_float_prelude.rs");
const PETGRAPH: &str = include_str!("petgraph_prelude.rs");

/// Whether `text` has `name` as a whole word: not inside a longer identifier (`Cow` isn't in
/// `Cowboy`, `time::` isn't in `realtime::`). A `name` that starts or ends in a non-identifier
/// character is open on that side.
pub(crate) fn has_word(text: &str, name: &str) -> bool {
    let ident = |c: char| c.is_alphanumeric() || c == '_';
    let (first, last) = (
        name.chars().next().is_some_and(ident),
        name.chars().next_back().is_some_and(ident),
    );
    let mut from = 0;
    while let Some(i) = text[from..].find(name) {
        let at = from + i;
        let before = first && text[..at].chars().next_back().is_some_and(ident);
        let after = last && text[at + name.len()..].chars().next().is_some_and(ident);
        if !before && !after {
            return true;
        }
        from = at + name.chars().next().map_or(1, char::len_utf8);
    }
    false
}

/// `text` with the preludes it needs after it, when it names their types and doesn't declare
/// them itself.
pub(crate) fn with_prelude(
    text: &str,
    filename: &str,
    resolver: crate::typeck::Resolver,
    whole_impls: bool,
) -> Option<String> {
    let declares = |name: &str| {
        ["struct", "enum", "type", "trait", "union"]
            .iter()
            .any(|kw| text.contains(&format!("{kw} {name}")))
    };
    let mut out: Vec<&'static str> = Vec::new();
    // (After the rewrite: `chrono::NaiveDate` is `ChronoNaiveDate`, ..; its `TimeDelta::to_std`
    // needs `std::time::Duration`.)
    let chrono = [
        "ChronoNaive",
        "ChronoDateTime",
        "ChronoUtc",
        "ChronoLocal",
        "ChronoFixedOffset",
        "ChronoTimeDelta",
        "ChronoWeekday",
        "ChronoMonth",
        "ChronoDatelike",
        "ChronoTimelike",
        "ChronoTimeZone",
        "ChronoParse",
        "ChronoDays",
        "ChronoSecondsFormat",
        "ChronoIsoWeek",
        "ChronoOutOfRange",
        "ChronoTz",
    ]
    .iter()
    .any(|n| text.contains(n))
        && !text.contains("// (chrono prelude)")
        && !declares("ChronoNaiveDate");
    // (The tokio prelude and std `mpsc`'s `recv_timeout` take a `Duration`.)
    let needs_duration = ["__tokio_", "TokioJoin", "TokioMutex", "TokioRwLock", "TokioElapsed", "MpscReceiver"]
        .iter()
        .any(|n| text.contains(n));
    if (has_word(text, "time::") || chrono || needs_duration)
        && !text.contains("// (std::time prelude)")
        && !["Duration", "Instant", "SystemTime"]
            .iter()
            .any(|n| declares(n))
    {
        out.push(TIME);
    }
    let cells = ["LazyLock", "LazyCell", "Lazy", "OnceLock", "OnceCell"];
    if cells.iter().any(|n| has_word(text, n))
        && !text.contains("// (sync prelude)")
        && !cells.iter().any(|n| declares(n))
    {
        out.push(SYNC);
    }
    // (After the rewrites: `atomic::Ordering` is `AtomicOrdering`, `thread_local!` declares
    // `LocalKey`s.)
    let locks = [
        "Mutex",
        "RwLock",
        "AtomicOrdering",
        "AtomicBool",
        "AtomicUsize",
        "AtomicIsize",
        "AtomicU8",
        "AtomicU16",
        "AtomicU32",
        "AtomicU64",
        "AtomicI8",
        "AtomicI16",
        "AtomicI32",
        "AtomicI64",
        "LocalKey",
        "PoisonError",
        "__atomic_fence",
        "MpscSender",
        "MpscReceiver",
        "__mpsc_channel",
        "__mpsc_sync_channel",
    ];
    if locks.iter().any(|n| has_word(text, n))
        && !text.contains("// (lock prelude)")
        && !["Mutex", "RwLock", "AtomicOrdering"].iter().any(|n| declares(n))
    {
        out.push(LOCK);
    }
    if has_word(text, "Either")
        && text.contains("either")
        && !text.contains("// (either prelude)")
        && !declares("Either")
    {
        out.push(EITHER);
    }
    if text.contains("serde_json")
        && !text.contains("// (serde_json prelude)")
        && !declares("Value")
    {
        out.push(SERDE_JSON);
    }
    // (After the rewrites: `regex::Error` is `RegexError`, `regex::escape` `__regex_escape`.)
    if (text.contains("regex::")
        || text.contains("use regex")
        || text.contains("RegexError")
        || text.contains("__regex_escape"))
        && !text.contains("// (regex prelude)")
        && !text.contains("mod regex")
        && !declares("Regex")
    {
        out.push(REGEX);
    }
    if has_word(text, "Cow") && !text.contains("// (cow prelude)") && !declares("Cow") {
        out.push(COW);
    }
    // (After the rewrite: `io::Error` is `IoError`, `io::stdout()` `__io_stdout()`, ..)
    let io_names = [
        "IoError", "IoWrite", "__io_std", "IoStd", "IoBuf", "IoRead", "IoCursor", "FmtWrite",
    ];
    // (After the rewrite: `std::fs::File` is `FsFile`, `process::Command` `ProcCommand`, ..)
    let os_names = [
        "FsFile",
        "FsOpenOptions",
        "FsMetadata",
        "FsDirEntry",
        "__fs_",
        "ProcCommand",
        "ProcStdio",
        "ProcExitStatus",
        "ProcOutput",
        "ProcChild",
    ];
    let os = os_names.iter().any(|n| text.contains(n))
        && !text.contains("// (os prelude)")
        && !declares("FsFile");
    // (`use std::io::Write;` alone stays as written: its methods on a `Vec<u8>` need the impl.)
    if (os
        || io_names.iter().any(|n| text.contains(n))
        || text.contains("io::Write")
        || text.contains("io::prelude"))
        && !text.contains("// (io prelude)")
        && !declares("IoError")
    {
        out.push(IO);
    }
    if crate::mbe::uses_log(text) && !text.contains("// (log prelude)") && !declares("LogLevel") {
        out.push(LOG);
    }
    // (After the rewrite: `num::Integer` is `NumInteger`, `num::integer::gcd` `__num_gcd`, ..)
    let num_names = [
        "NumInteger",
        "NumZero",
        "NumOne",
        "NumSigned",
        "NumBounded",
        "NumToPrimitive",
        "NumFromPrimitive",
        "__num_",
    ];
    if num_names.iter().any(|n| text.contains(n)) && !text.contains("// (num prelude)") {
        out.push(NUM);
    }
    let tokio_names = [
        "__tokio_",
        "TokioJoin",
        "TokioMutex",
        "TokioRwLock",
        "TokioElapsed",
        "TokioSender",
        "TokioUnboundedSender",
        "TokioReceiver",
    ];
    if tokio_names.iter().any(|n| text.contains(n)) && !text.contains("// (tokio prelude)") {
        out.push(TOKIO);
    }
    // (After the rewrite: `rand_distr::Normal` is `RandNormal`, `Distribution` `RandDistribution`.)
    let rand_names = [
        "RandDistribution",
        "RandStandard",
        "RandExp",
        "RandOpen",
        "RandUniform",
        "RandWeight",
        "RandBernoulli",
        "RandAlphanumeric",
        "RandNormal",
        "RandLogNormal",
        "RandPoisson",
        "RandBinomial",
        "RandGamma",
        "RandBeta",
        "RandChiSquared",
        "RandStudentT",
        "RandTriangular",
        "RandPareto",
        "RandCauchy",
        "RandWeibull",
        "RandGeo",
    ];
    if rand_names.iter().any(|n| text.contains(n))
        && !text.contains("// (rand_distr prelude)")
        && !declares("RandDistribution")
    {
        out.push(RAND_DISTR);
    }
    if chrono {
        out.push(CHRONO);
    }
    // (After the rewrites: `jsx!` makes `__rx_h(..)` calls, `react::Node` is `ReactNode`, ..)
    let react_names = [
        "__rx_",
        "ReactSetState",
        "ReactRef",
        "ReactContext",
        "ReactDispatch",
        "ReactIntoNode",
    ];
    if react_names.iter().any(|n| text.contains(n)) && !text.contains("// (react prelude)") {
        out.push(REACT);
    }
    // (After the rewrite: `hash_map::Entry` is `StdMapEntry`.)
    if text.contains("StdMapEntry") && !text.contains("// (map entry prelude)") {
        out.push(ENTRY);
    }
    // (After the derives and the rewrite: `clap::Command` is `ClapCommand`, ..)
    let clap_names = [
        "ClapCommand",
        "ClapArg",
        "ClapMatches",
        "ClapError",
        "ClapAction",
        "ClapValueParser",
        "ClapPossibleValue",
    ];
    if clap_names.iter().any(|n| text.contains(n))
        && !text.contains("// (clap prelude)")
        && !declares("ClapCommand")
    {
        out.push(CLAP);
    }
    // (`use ordered_float::OrderedFloat as OF;` / `ordered_float::NotNan` are the names here.)
    if text.contains("ordered_float")
        && ["OrderedFloat", "NotNan", "FloatIsNan"]
            .iter()
            .any(|n| text.contains(n))
        && !text.contains("// (ordered_float prelude)")
        && !declares("OrderedFloat")
    {
        out.push(ORDERED_FLOAT);
    }
    // (After the rewrites: `petgraph::Graph` is `PgGraph`, `dijkstra(&g, ..)` `(&g).__pg_dijkstra(..)`.)
    if (text.contains("Pg") || text.contains("__pg_"))
        && crate::bundled::PETGRAPH_NAMES
            .iter()
            .any(|(_, n)| text.contains(n))
        && !text.contains("// (petgraph prelude)")
    {
        out.push(PETGRAPH);
    }
    if os {
        out.push(OS);
    }
    // (After the rewrite: `std::net::IpAddr` is `NetIpAddr`, ..)
    if ["NetIp", "NetSocketAddr", "NetAddrParseError"]
        .iter()
        .any(|n| text.contains(n))
        && !text.contains("// (net prelude)")
        && !declares("NetIpAddr")
    {
        out.push(NET);
    }
    // (After the rewrite: `std::task::Poll` is `TaskPoll`, `std::future::ready` `__future_ready`.)
    // (The tokio prelude's channel is a hand-written future.)
    let task_names = [
        "TaskPoll",
        "TokioReceiver",
        "__tokio_channel",
        "__tokio_unbounded_channel",
        "TaskContext",
        "TaskWaker",
        "__future_",
        "FutureReady",
        "FuturePending",
        "FuturePollFn",
    ];
    if task_names.iter().any(|n| text.contains(n))
        && !text.contains("// (task prelude)")
        && !declares("TaskPoll")
    {
        out.push(TASK);
    }
    // (`PhantomData`, `std::path`, which `DirEntry::path()` returns.)
    let dir_entries = os && (text.contains("FsDirEntry") || text.contains("__fs_read_dir"));
    out.extend(crate::stdgaps::preludes(text, dir_entries));
    if out.is_empty() {
        return None;
    }
    let modules = crate::prune::module_texts(text, filename, resolver);
    let trimmed = crate::prune::trimmed(text, &modules, &out, whole_impls);
    Some(format!("{text}{trimmed}"))
}
