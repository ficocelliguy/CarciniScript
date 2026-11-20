//! Help lines for players who know JavaScript or Python but not Rust.
//!
//! A diagnostic's `message` stays rustc's one-liner (a web search for it finds the same answer
//! a Rust developer would); `help` carries the rule behind it in one or two sentences, and what
//! to try. The constants are the catalog, one per concept; `for_message` attaches one to a
//! message by its shape when the site that produced it didn't choose one itself.

pub const MISSING_RET: &str = "A function without `-> Type` returns nothing, and its last expression without `;` is its return value. Declare the return type, or end that line with `;` to discard the value.";

pub const STRAY_SEMI: &str = "A function's value is its last expression written without `;` (`return x;` also works). A line ending in `;` is a statement and produces nothing.";

pub const MOVES: &str = "Passing or assigning a `String`, `Vec` or struct hands it over; the old name can't be used again. Pass `&s` to lend it, or `s.clone()` to copy it.";

pub const BORROWS: &str = "`&v` lends read access and `&mut v` write access; while a `&mut` is alive nothing else may touch `v`, and while any `&` is alive nothing may change it. Finish using the reference before the next change, or copy the value out first.";

/// Appended to `BORROWS` when borrow conflicts are warnings.
pub const LAX_BORROWS: &str = "This runs anyway here (rustc would refuse); turn on 'Rust: borrow conflicts are errors' to match rustc.";

pub const RETURNED_REF: &str = "A reference points at a variable that is deleted when the function ends. Return the value itself (`String`, not `&String`), or `.clone()` it.";

pub const MUTABILITY: &str = "Variables are read-only unless declared `let mut`.";

pub const BEHIND_REF: &str = "A `&T` parameter lends the value read-only. Declare it `&mut T` to change it through, and pass `&mut x` at the call.";

pub const REFS: &str = "`&` in a parameter type means the function borrows the value instead of taking it; add `&` in front of the argument.";

pub const NUMBERS: &str = "Rust never converts between number types for you; NS numbers are `f64` (whole ones, like threads and pids, `i32`). Write `1.0`, or cast with `as f64` / `as usize`.";

pub const OPTION_RESULT: &str = "`Option<T>` is a value that may be missing (`null` in JavaScript); `Result<T, E>` is a value or an error. Get the inside with `.unwrap()` (panics if missing), `if let Some(x) = ..`, or `?` inside a function returning the same wrapper.";

pub const FUTURES: &str = "An NS call returns a future, not its value, until it is awaited (like a Promise). Write `.await` after the call, inside an `async fn`.";

pub const JS_NAMES: &str = "Rust's std names differ from JavaScript's; docs/FEATURES.md lists what is supported.";

pub const SEMI: &str = "Every statement ends with `;` (an `if`, `match` or loop used as a statement is the exception), and a block's last expression without `;` is its value.";

pub const BRACES: &str = "Blocks, struct bodies and `match` arms are delimited by `{ }`, and every opening bracket needs its closing one.";

pub const UNSUPPORTED: &str = "CarciniScript compiles a subset of Rust; see docs/FEATURES.md for what's in.";

/// A help line for a message no site chose one for, by the message's shape. `severity` tells a
/// borrow conflict that is a warning (strict borrows off) from one that stops the compile.
pub fn for_message(message: &str, severity: &str) -> Option<String> {
    let m = message;
    let has = |s: &str| m.starts_with(s);
    let help: &str = if has("use of moved value") || has("use of partially moved value") {
        MOVES
    } else if has("cannot return reference to") || has("cannot return value referencing") {
        if severity == "warning" {
            return Some(format!("{RETURNED_REF} {LAX_BORROWS}"));
        }
        RETURNED_REF
    } else if has("cannot assign twice to immutable variable") {
        MUTABILITY
    } else if has("cannot borrow `") && m.contains("as it is not declared as mutable") {
        MUTABILITY
    } else if has("cannot borrow `") && m.contains("as it is behind a `&` reference") {
        BEHIND_REF
    } else if has("cannot borrow `")
        || (has("cannot assign to `") && m.contains("because it is borrowed"))
        || has("cannot move out of `")
        || (has("cannot use `") && m.contains("because it was mutably borrowed"))
        || m.contains("does not live long enough")
        || has("temporary value dropped while borrowed")
        || has("lifetime may not live long enough")
    {
        if severity == "warning" {
            return Some(format!("{BORROWS} {LAX_BORROWS}"));
        }
        BORROWS
    } else if m.contains("did you forget `.await`?") {
        FUTURES
    } else if m.contains("is not supported yet") || m.contains("isn't supported") {
        UNSUPPORTED
    } else if m == "expected `;`" {
        SEMI
    } else if matches!(m, "expected `}`" | "expected `{`" | "expected `)`" | "expected `]`") {
        BRACES
    } else {
        return None;
    };
    Some(help.to_string())
}

/// The Rust name for a JavaScript or Python method or property a player might write on a
/// `Vec`, `String`, `HashMap` or number, with how to write it.
pub fn js_alias(name: &str, is_field: bool) -> Option<&'static str> {
    Some(match name {
        "length" | "size" | "count" => "`.len()`",
        "push_back" | "append" | "add" => "`.push(x)`",
        "pop_back" => "`.pop()`",
        "indexOf" | "index_of" | "index" | "find_index" | "findIndex" => "`.iter().position(|x| ..)`",
        "includes" | "has" | "__contains__" => "`.contains(&x)`",
        "substring" | "substr" | "slice" => "`&s[a..b]`",
        "toUpperCase" | "upper" => "`.to_uppercase()`",
        "toLowerCase" | "lower" => "`.to_lowercase()`",
        "charAt" | "char_at" => "`.chars().nth(i)`",
        "concat" if !is_field => "`+` or `.push_str(..)` on a `String`, `.extend(..)` on a `Vec`",
        "forEach" | "for_each" | "map" | "filter" | "reduce" | "some" | "every" | "find" | "sort_by_key"
            if !is_field =>
        {
            "`.iter().map(..)` / `.iter().filter(..)` / `.iter().any(..)` and so on, then `.collect()`"
        }
        "toString" | "str" | "to_str" => "`.to_string()`",
        "parseInt" | "parseFloat" | "int" | "float" => "`.parse::<i32>()` / `.parse::<f64>()`",
        "floor" | "ceil" | "round" | "trunc" | "sqrt" | "abs" if is_field => "a method: `.floor()`",
        "splice" => "`.drain(a..b)` or `.remove(i)` / `.insert(i, x)`",
        "shift" => "`.remove(0)` (or a `VecDeque`'s `.pop_front()`)",
        "unshift" => "`.insert(0, x)`",
        "reverse" if is_field => "a method: `.reverse()`",
        "join" if is_field => "a method: `.join(\", \")`",
        "keys" | "values" | "entries" if is_field => "a method: `.keys()` / `.values()` / `.iter()`",
        "trim" | "split" | "replace" | "startsWith" | "endsWith" if is_field => "a method: `.trim()`",
        "startsWith" | "starts_with_str" => "`.starts_with(..)`",
        "endsWith" => "`.ends_with(..)`",
        "isEmpty" | "is_empty_" | "empty" => "`.is_empty()`",
        "get_or_default" | "getOrDefault" => "`.get(&k).copied().unwrap_or(d)`",
        "set" if !is_field => "`.insert(k, v)`",
        "delete" => "`.remove(&k)`",
        _ => return None,
    })
}

/// A parser error with ra_ap_syntax's token names (`expected SEMICOLON`) written as the token
/// (`expected `;``).
pub fn parser_message(message: &str) -> String {
    let mut out = String::with_capacity(message.len());
    for (i, word) in message.split(' ').enumerate() {
        if i > 0 {
            out.push(' ');
        }
        let (core, trail) = match word.strip_suffix(',') {
            Some(c) => (c, ","),
            None => (word, ""),
        };
        match token_text(core) {
            Some(t) => {
                out.push('`');
                out.push_str(t);
                out.push('`');
            }
            None => out.push_str(core),
        }
        out.push_str(trail);
    }
    out
}

/// The text of a punctuation or keyword token by its `SyntaxKind` name.
fn token_text(kind: &str) -> Option<&'static str> {
    if let Some(kw) = kind.strip_suffix("_KW")
        && kw.chars().all(|c| c.is_ascii_uppercase() || c == '_')
    {
        return Some(match kw {
            "FN" => "fn",
            "LET" => "let",
            "MUT" => "mut",
            "IF" => "if",
            "ELSE" => "else",
            "MATCH" => "match",
            "FOR" => "for",
            "IN" => "in",
            "WHILE" => "while",
            "LOOP" => "loop",
            "RETURN" => "return",
            "STRUCT" => "struct",
            "ENUM" => "enum",
            "IMPL" => "impl",
            "TRAIT" => "trait",
            "PUB" => "pub",
            "USE" => "use",
            "MOD" => "mod",
            "AS" => "as",
            "ASYNC" => "async",
            "AWAIT" => "await",
            "TYPE" => "type",
            "CONST" => "const",
            "STATIC" => "static",
            "WHERE" => "where",
            "SELF" => "self",
            "SELF_TYPE" => "Self",
            "TRUE" => "true",
            "FALSE" => "false",
            "DYN" => "dyn",
            "MOVE" => "move",
            "REF" => "ref",
            "BREAK" => "break",
            "CONTINUE" => "continue",
            "UNSAFE" => "unsafe",
            "CRATE" => "crate",
            "SUPER" => "super",
            "EXTERN" => "extern",
            "MACRO_RULES" => "macro_rules",
            _ => return None,
        });
    }
    Some(match kind {
        "SEMICOLON" => ";",
        "COMMA" => ",",
        "COLON" => ":",
        "COLON2" => "::",
        "DOT" => ".",
        "DOT2" => "..",
        "DOT2EQ" => "..=",
        "DOT3" => "...",
        "EQ" => "=",
        "EQ2" => "==",
        "FAT_ARROW" => "=>",
        "THIN_ARROW" => "->",
        "L_PAREN" => "(",
        "R_PAREN" => ")",
        "L_CURLY" => "{",
        "R_CURLY" => "}",
        "L_BRACK" => "[",
        "R_BRACK" => "]",
        "L_ANGLE" => "<",
        "R_ANGLE" => ">",
        "PIPE" => "|",
        "PIPE2" => "||",
        "AMP" => "&",
        "AMP2" => "&&",
        "BANG" => "!",
        "QUESTION" => "?",
        "AT" => "@",
        "POUND" => "#",
        "DOLLAR" => "$",
        "TILDE" => "~",
        "PLUS" => "+",
        "MINUS" => "-",
        "STAR" => "*",
        "SLASH" => "/",
        "PERCENT" => "%",
        "CARET" => "^",
        "UNDERSCORE" => "_",
        "IDENT" => "an identifier",
        "INT_NUMBER" => "a number",
        "STRING" => "a string",
        "LIFETIME_IDENT" => "a lifetime",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parser_tokens_become_text() {
        assert_eq!(parser_message("expected SEMICOLON"), "expected `;`");
        assert_eq!(parser_message("expected R_CURLY"), "expected `}`");
        assert_eq!(parser_message("expected COMMA, got R_PAREN"), "expected `,`, got `)`");
        assert_eq!(parser_message("expected FN_KW"), "expected `fn`");
        assert_eq!(parser_message("expected expression"), "expected expression");
        assert_eq!(parser_message("expected an item"), "expected an item");
    }

    #[test]
    fn help_by_shape() {
        assert_eq!(for_message("use of moved value: `s` (moved on line 1:2 by `let`)", "error").as_deref(), Some(MOVES));
        assert_eq!(
            for_message("cannot borrow `v` as mutable because it is also borrowed as immutable", "error").as_deref(),
            Some(BORROWS)
        );
        assert!(
            for_message("cannot borrow `v` as mutable because it is also borrowed as immutable", "warning")
                .is_some_and(|h| h.ends_with(LAX_BORROWS))
        );
        assert_eq!(for_message("cannot borrow `v` as mutable, as it is not declared as mutable", "error").as_deref(), Some(MUTABILITY));
        assert_eq!(for_message("cannot return reference to local variable `s`", "error").as_deref(), Some(RETURNED_REF));
        assert_eq!(for_message("expected `;`", "error").as_deref(), Some(SEMI));
        assert_eq!(for_message("this function takes 1 argument but 2 were supplied", "error"), None);
    }

    #[test]
    fn js_aliases() {
        assert_eq!(js_alias("length", true), Some("`.len()`"));
        assert_eq!(js_alias("length", false), Some("`.len()`"));
        assert_eq!(js_alias("push_back", false), Some("`.push(x)`"));
        assert_eq!(js_alias("len", false), None);
    }
}
