//! `std::time` (`Duration`, `Instant`, `SystemTime`): Rust source compiled in after a program
//! that uses it. Paths like `std::time::Duration` and `use std::time::{..}` resolve to the
//! prelude's top-level items; positions in the program are unchanged.

const PRELUDE: &str = include_str!("time_prelude.rs");

/// `text` with the `std::time` prelude after it, when it names `time::` and doesn't declare
/// those types itself.
pub(crate) fn with_prelude(text: &str) -> Option<String> {
    if !text.contains("time::") || text.contains("// (std::time prelude)") {
        return None;
    }
    let declares = |name: &str| {
        ["struct", "enum", "type", "trait", "union"]
            .iter()
            .any(|kw| text.contains(&format!("{kw} {name}")))
    };
    if ["Duration", "Instant", "SystemTime"]
        .iter()
        .any(|n| declares(n))
    {
        return None;
    }
    Some(format!("{text}\n{PRELUDE}"))
}
