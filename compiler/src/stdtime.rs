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
const ITERTOOLS: &str = include_str!("itertools_prelude.rs");
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
    // (The program's own types don't ask for a prelude: `mod read; use read::IoRead;` and
    // `struct IoRead` are its `IoRead`, not the io prelude's.)
    // (Nor do its comments: a doc comment naming `io::Write`.)
    let full = text;
    let code = crate::lineindex::blank_comments(text);
    let masked = mask_own_types(&code);
    let text: &str = &masked;
    let declares = |name: &str| {
        ["struct", "enum", "type", "trait", "union"]
            .iter()
            .any(|kw| has_word(&code, &format!("{kw} {name}")))
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
    if text.contains("itertools")
        && ["minmax", "fold_while", "exactly_one", "MinMaxResult", "FoldWhile", "ExactlyOneError"]
            .iter()
            .any(|n| text.contains(n))
        && !text.contains("// (itertools prelude)")
        && !declares("MinMaxResult")
    {
        out.push(ITERTOOLS);
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
        "IoError", "IoWrite", "__io_std", "IoStd", "IoBuf", "IoRead", "IoIoSlice", "IoCursor", "IoSeek", "FmtWrite", "FmtAlignment",
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
    // (`Path`'s methods that read the filesystem are the os prelude's.)
    let path_fs = (has_word(text, "Path") || text.contains("PathBuf"))
        && [".metadata(", ".symlink_metadata(", ".read_dir(", ".read_link(", ".canonicalize("]
            .iter()
            .any(|m| text.contains(m));
    let os = (path_fs || os_names.iter().any(|n| text.contains(n)))
        && !text.contains("// (os prelude)")
        && !declares("FsFile");
    // (`use std::io::Write;` / `Read` alone stay as written: their methods on a `Vec<u8>` /
    // `&[u8]` need the impls.)
    if (os
        || io_names.iter().any(|n| text.contains(n))
        || text.contains("io::Write")
        || text.contains("io::Read")
        || (text.contains("io::{") && (has_word(text, "Read") || has_word(text, "Write")))
        || text.contains("io::prelude")
        // (serde_json's `to_writer` / `from_reader` take an `io::Write` / `io::Read`.)
        || (text.contains("serde_json") && (text.contains("to_writer") || text.contains("from_reader"))))
        && !text.contains("// (io prelude)")
        && !declares("IoError")
    {
        out.push(IO);
    }
    // (`log::Level` alone is `LogLevel` by now.)
    if (crate::mbe::uses_log(text) || text.contains("LogLevel") || text.contains("__log_"))
        && !text.contains("// (log prelude)")
        && !declares("LogLevel")
    {
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
        "TokioOneshot",
        "TokioWatch",
        "TokioBroadcast",
        "TokioNotify",
        "TokioInterval",
        "TokioMissedTickBehavior",
        "TokioRecvError",
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
        "TokioOneshot",
        "__tokio_oneshot_channel",
        "TokioWatch",
        "__tokio_watch_channel",
        "TokioBroadcast",
        "__tokio_broadcast_channel",
        "TokioNotify",
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
    // (`PhantomData`, `std::path`: the os prelude's `impl PathBuf`, `DirEntry::path()`.)
    out.extend(crate::stdgaps::preludes(text, os));
    if out.is_empty() {
        return None;
    }
    let modules = crate::prune::module_texts(full, filename, resolver);
    let trimmed = crate::prune::trimmed(full, &modules, &out, whole_impls);
    Some(format!("{full}{trimmed}"))
}

/// `text` with the program's own type names blanked out, for deciding which preludes it needs:
/// the types it defines (`struct IoRead`), and every word a `use` brings in from one of its file
/// modules (`mod read;` then `use read::IoRead;` / `pub use crate::read::{IoRead, Other as O}`)
/// or a path names there (`read::IoRead::new(..)`).
fn mask_own_types(text: &str) -> std::borrow::Cow<'_, str> {
    let ident = |c: char| c.is_alphanumeric() || c == '_';
    let mut names: Vec<String> = Vec::new();
    // (Not the types of a prelude compiled in already.)
    let marker = text.lines().find(|l| l.starts_with("// (") && l.ends_with(" prelude)"));
    let own = marker
        .and_then(|m| text.find(m))
        .map_or(text, |i| &text[..i]);
    for kw in ["struct ", "enum ", "trait ", "type ", "union "] {
        let mut from = 0;
        while let Some(i) = own[from..].find(kw) {
            let at = from + i;
            from = at + kw.len();
            if own[..at].chars().next_back().is_some_and(ident) {
                continue;
            }
            let name: String = own[from..].chars().take_while(|c| ident(*c)).collect();
            if name.starts_with(|c: char| c.is_uppercase()) && !names.contains(&name) {
                names.push(name);
            }
        }
    }
    let mut mods: Vec<&str> = Vec::new();
    for line in text.lines() {
        let l = line.trim_start();
        let l = l.strip_prefix("pub").map_or(l, |r| {
            let r = r.trim_start();
            match r.strip_prefix('(') {
                Some(r) => r.split_once(')').map_or(r, |x| x.1).trim_start(),
                None => r,
            }
        });
        if let Some(rest) = l.strip_prefix("mod ")
            && let Some(name) = rest.trim().strip_suffix(';')
            && name.chars().all(|c| c.is_alphanumeric() || c == '_')
        {
            mods.push(name.trim_start_matches("r#"));
        }
    }
    let mut from = 0;
    while let Some(i) = text[from..].find("use ") {
        let at = from + i;
        from = at + 4;
        if text[..at]
            .chars()
            .next_back()
            .is_some_and(|c| c.is_alphanumeric() || c == '_')
        {
            continue;
        }
        let Some(end) = text[from..].find(';') else { break };
        let stmt = &text[from..from + end];
        let mut path = stmt.trim_start().trim_start_matches("::");
        for p in ["crate::", "self::"] {
            path = path.strip_prefix(p).unwrap_or(path);
        }
        while let Some(p) = path.strip_prefix("super::") {
            path = p;
        }
        let first = path.split("::").next().unwrap_or("").trim();
        if !mods.contains(&first) {
            continue;
        }
        // (The names it brings in: each last segment, or the `as` name.)
        for part in path.split(|c| matches!(c, ',' | '{' | '}')) {
            let part = part.trim();
            let name = match part.split_once(" as ") {
                Some((_, alias)) => alias.trim(),
                None => part.rsplit("::").next().unwrap_or("").trim(),
            };
            if name.starts_with(|c: char| c.is_uppercase())
                && name.chars().all(|c| c.is_alphanumeric() || c == '_')
            {
                names.push(name.to_string());
            }
        }
    }
    // (And the names it reaches by path: `de::IoRead::new(..)`.)
    for m in &mods {
        let pat = format!("{m}::");
        let mut from = 0;
        while let Some(i) = text[from..].find(&pat) {
            let at = from + i;
            from = at + pat.len();
            if text[..at]
                .chars()
                .next_back()
                .is_some_and(|c| c.is_alphanumeric() || c == '_')
            {
                continue;
            }
            let name: String = text[from..]
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            if name.starts_with(|c: char| c.is_uppercase()) && !names.contains(&name) {
                names.push(name);
            }
        }
    }
    if names.is_empty() {
        return text.into();
    }
    let mut out = text.to_string();
    for n in &names {
        let blank = "_".repeat(n.len());
        let mut from = 0;
        while let Some(i) = out[from..].find(n.as_str()) {
            let at = from + i;
            let ident = |c: char| c.is_alphanumeric() || c == '_';
            let before = out[..at].chars().next_back().is_some_and(ident);
            let after = out[at + n.len()..].chars().next().is_some_and(ident);
            if !before && !after {
                out.replace_range(at..at + n.len(), &blank);
            }
            from = at + n.len();
        }
    }
    out.into()
}

/// Where the Rust-source preludes compiled in after the program start in `text` (its first
/// `// (.. prelude)` marker line), or its length.
pub(crate) fn prelude_start(text: &str) -> usize {
    let mut at = 0;
    for line in text.split_inclusive('\n') {
        let l = line.trim_end();
        if l.starts_with("// (") && l.ends_with(" prelude)") {
            return at;
        }
        at += line.len();
    }
    text.len()
}

/// Whether the program's own `fn Ok` / `fn Err` is what a bare `Ok(..)` at `node` calls: not
/// inside the preludes, whose `Ok(..)` is std's.
pub(crate) fn user_ctor_fn_at(text: &str, node: &ra_ap_syntax::SyntaxNode) -> bool {
    usize::from(node.text_range().start()) < prelude_start(text)
}
