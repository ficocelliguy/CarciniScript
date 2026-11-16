//! Rust-source preludes compiled in after a program that uses them: `std::time` (`Duration`,
//! `Instant`, `SystemTime`), the lazy and once cells (`std::sync::{LazyLock, OnceLock}`,
//! `std::cell::{LazyCell, OnceCell}`, the bundled `once_cell`'s `Lazy` / `OnceCell`) and the
//! bundled `either`, `serde_json` (`Value`, `Number`) and `regex` (`Regex`, `Match`,
//! `Captures`). Paths like `std::time::Duration` and `use std::time::{..}` resolve to the
//! prelude's top-level items; positions in the program are unchanged.

const TIME: &str = include_str!("time_prelude.rs");
const SYNC: &str = include_str!("sync_prelude.rs");
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

/// `text` with the preludes it needs after it, when it names their types and doesn't declare
/// them itself.
pub(crate) fn with_prelude(text: &str) -> Option<String> {
    let declares = |name: &str| {
        ["struct", "enum", "type", "trait", "union"]
            .iter()
            .any(|kw| text.contains(&format!("{kw} {name}")))
    };
    let mut out = String::new();
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
    if (text.contains("time::") || chrono)
        && !text.contains("// (std::time prelude)")
        && !["Duration", "Instant", "SystemTime"]
            .iter()
            .any(|n| declares(n))
    {
        out.push('\n');
        out.push_str(TIME);
    }
    let cells = ["LazyLock", "LazyCell", "Lazy", "OnceLock", "OnceCell"];
    if cells.iter().any(|n| text.contains(n))
        && !text.contains("// (sync prelude)")
        && !cells.iter().any(|n| declares(n))
    {
        out.push('\n');
        out.push_str(SYNC);
    }
    if text.contains("Either")
        && text.contains("either")
        && !text.contains("// (either prelude)")
        && !declares("Either")
    {
        out.push('\n');
        out.push_str(EITHER);
    }
    if text.contains("serde_json")
        && !text.contains("// (serde_json prelude)")
        && !declares("Value")
    {
        out.push('\n');
        out.push_str(SERDE_JSON);
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
        out.push('\n');
        out.push_str(REGEX);
    }
    if text.contains("Cow") && !text.contains("// (cow prelude)") && !declares("Cow") {
        out.push('\n');
        out.push_str(COW);
    }
    // (After the rewrite: `io::Error` is `IoError`, `io::stdout()` `__io_stdout()`, ..)
    let io_names = [
        "IoError", "IoWrite", "__io_std", "IoStd", "IoBuf", "IoRead", "IoCursor",
    ];
    if io_names.iter().any(|n| text.contains(n))
        && !text.contains("// (io prelude)")
        && !declares("IoError")
    {
        out.push('\n');
        out.push_str(IO);
    }
    if crate::mbe::uses_log(text) && !text.contains("// (log prelude)") && !declares("LogLevel") {
        out.push('\n');
        out.push_str(LOG);
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
        out.push('\n');
        out.push_str(NUM);
    }
    let tokio_names = ["__tokio_", "TokioJoin"];
    if tokio_names.iter().any(|n| text.contains(n)) && !text.contains("// (tokio prelude)") {
        out.push('\n');
        out.push_str(TOKIO);
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
        out.push('\n');
        out.push_str(RAND_DISTR);
    }
    if chrono {
        out.push('\n');
        out.push_str(CHRONO);
    }
    // (After the rewrites: `jsx!` makes `__rx_h(..)` calls, `react::Node` is `ReactNode`, ..)
    if (text.contains("__rx_") || text.contains("ReactSetState") || text.contains("ReactRef"))
        && !text.contains("// (react prelude)")
    {
        out.push('\n');
        out.push_str(REACT);
    }
    // (After the rewrite: `hash_map::Entry` is `StdMapEntry`.)
    if text.contains("StdMapEntry") && !text.contains("// (map entry prelude)") {
        out.push('\n');
        out.push_str(ENTRY);
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
        out.push('\n');
        out.push_str(CLAP);
    }
    // (`PhantomData`, `std::path`.)
    out.push_str(&crate::stdgaps::preludes(text));
    (!out.is_empty()).then(|| format!("{text}{out}"))
}
