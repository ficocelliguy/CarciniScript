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

/// `text` with the preludes it needs after it, when it names their types and doesn't declare
/// them itself.
pub(crate) fn with_prelude(text: &str) -> Option<String> {
    let declares = |name: &str| {
        ["struct", "enum", "type", "trait", "union"]
            .iter()
            .any(|kw| text.contains(&format!("{kw} {name}")))
    };
    let mut out = String::new();
    if text.contains("time::")
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
    (!out.is_empty()).then(|| format!("{text}{out}"))
}
