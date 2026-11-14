
// (proc-macro crate prelude)
// What CarciniScript compiles into a proc-macro crate in place of `proc_macro`, `proc_macro2`
// and `quote`. A token stream keeps its tokens as text; iterating it lexes the text into token
// trees. Every token type has `__tt` (its text), which `quote!` and collecting use.

/// `proc_macro::TokenStream` (and `proc_macro2`'s).
#[derive(Clone, Debug, Default)]
pub struct TokenStream {
    __text: String,
}

impl TokenStream {
    pub fn new() -> TokenStream {
        TokenStream {
            __text: String::new(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.__text.trim().is_empty()
    }

    pub fn span(&self) -> Span {
        Span
    }

    pub fn __tt(&self) -> String {
        self.__text.clone()
    }

    /// Append tokens (written as text).
    pub fn __push(&mut self, text: String) {
        if text.trim().is_empty() {
            return;
        }
        if !self.__text.is_empty() {
            self.__text.push(' ');
        }
        self.__text.push_str(&text);
    }
}

impl IntoIterator for TokenStream {
    type Item = TokenTree;
    fn into_iter(self) -> impl Iterator<Item = TokenTree> {
        __lex(&self.__text).into_iter()
    }
}

impl std::fmt::Display for TokenStream {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.__text)
    }
}

impl From<TokenTree> for TokenStream {
    fn from(tree: TokenTree) -> TokenStream {
        TokenStream {
            __text: tree.__tt(),
        }
    }
}

/// `Span`: positions aren't tracked; every span is the call site.
#[derive(Clone, Copy, Debug)]
pub struct Span;

impl Span {
    pub fn call_site() -> Span {
        Span
    }

    pub fn mixed_site() -> Span {
        Span
    }

    pub fn join(&self, _other: Span) -> Option<Span> {
        Some(Span)
    }
}

/// One token, or a delimited group of them.
#[derive(Clone, Debug)]
pub enum TokenTree {
    Group(Group),
    Ident(Ident),
    Punct(Punct),
    Literal(Literal),
}

impl TokenTree {
    pub fn span(&self) -> Span {
        Span
    }

    pub fn __tt(&self) -> String {
        match self {
            TokenTree::Group(g) => g.__tt(),
            TokenTree::Ident(i) => i.__tt(),
            TokenTree::Punct(p) => p.__tt(),
            TokenTree::Literal(l) => l.__tt(),
        }
    }

    /// Whether the next token follows without a space (`:` of `::`).
    pub fn __joint(&self) -> bool {
        match self {
            TokenTree::Punct(p) => p.__spacing == Spacing::Joint,
            _ => false,
        }
    }
}

impl std::fmt::Display for TokenTree {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.__tt())
    }
}

impl From<Group> for TokenTree {
    fn from(g: Group) -> TokenTree {
        TokenTree::Group(g)
    }
}

impl From<Ident> for TokenTree {
    fn from(i: Ident) -> TokenTree {
        TokenTree::Ident(i)
    }
}

impl From<Punct> for TokenTree {
    fn from(p: Punct) -> TokenTree {
        TokenTree::Punct(p)
    }
}

impl From<Literal> for TokenTree {
    fn from(l: Literal) -> TokenTree {
        TokenTree::Literal(l)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Delimiter {
    Parenthesis,
    Brace,
    Bracket,
    None,
}

/// `( .. )`, `[ .. ]` or `{ .. }` around a token stream.
#[derive(Clone, Debug)]
pub struct Group {
    __delimiter: Delimiter,
    __stream: TokenStream,
}

impl Group {
    pub fn new(delimiter: Delimiter, stream: TokenStream) -> Group {
        Group {
            __delimiter: delimiter,
            __stream: stream,
        }
    }

    pub fn delimiter(&self) -> Delimiter {
        self.__delimiter
    }

    pub fn stream(&self) -> TokenStream {
        self.__stream.clone()
    }

    pub fn span(&self) -> Span {
        Span
    }

    pub fn __tt(&self) -> String {
        let inner = self.__stream.__tt();
        match self.__delimiter {
            Delimiter::Parenthesis => format!("({inner})"),
            Delimiter::Brace if inner.is_empty() => "{}".to_string(),
            Delimiter::Brace => format!("{{ {inner} }}"),
            Delimiter::Bracket => format!("[{inner}]"),
            Delimiter::None => inner,
        }
    }
}

impl std::fmt::Display for Group {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.__tt())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Spacing {
    Alone,
    Joint,
}

/// A punctuation character; `Joint` when the next one follows it directly (`::`, `->`).
#[derive(Clone, Debug)]
pub struct Punct {
    __ch: char,
    __spacing: Spacing,
}

impl Punct {
    pub fn new(ch: char, spacing: Spacing) -> Punct {
        Punct {
            __ch: ch,
            __spacing: spacing,
        }
    }

    pub fn as_char(&self) -> char {
        self.__ch
    }

    pub fn spacing(&self) -> Spacing {
        self.__spacing
    }

    pub fn span(&self) -> Span {
        Span
    }

    pub fn __tt(&self) -> String {
        self.__ch.to_string()
    }
}

impl std::fmt::Display for Punct {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.__ch)
    }
}

/// `Ident`: a name, spliced as is by `quote!`.
#[derive(Clone, Debug)]
pub struct Ident {
    __text: String,
}

impl Ident {
    pub fn new(name: &str, _span: Span) -> Ident {
        Ident {
            __text: name.to_string(),
        }
    }

    pub fn new_raw(name: &str, _span: Span) -> Ident {
        Ident {
            __text: format!("r#{name}"),
        }
    }

    pub fn span(&self) -> Span {
        Span
    }

    pub fn __tt(&self) -> String {
        self.__text.clone()
    }
}

impl std::fmt::Display for Ident {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.__text)
    }
}

/// `Literal`: a literal's source text.
#[derive(Clone, Debug)]
pub struct Literal {
    __text: String,
}

impl Literal {
    pub fn string(s: &str) -> Literal {
        Literal {
            __text: format!("{:?}", s),
        }
    }

    pub fn character(c: char) -> Literal {
        Literal {
            __text: format!("{:?}", c),
        }
    }

    pub fn usize_unsuffixed(n: usize) -> Literal {
        Literal {
            __text: n.to_string(),
        }
    }

    pub fn usize_suffixed(n: usize) -> Literal {
        Literal {
            __text: format!("{n}usize"),
        }
    }

    pub fn u8_suffixed(n: u8) -> Literal {
        Literal {
            __text: format!("{n}u8"),
        }
    }

    pub fn u32_unsuffixed(n: u32) -> Literal {
        Literal {
            __text: n.to_string(),
        }
    }

    pub fn u32_suffixed(n: u32) -> Literal {
        Literal {
            __text: format!("{n}u32"),
        }
    }

    pub fn u64_unsuffixed(n: u64) -> Literal {
        Literal {
            __text: n.to_string(),
        }
    }

    pub fn u64_suffixed(n: u64) -> Literal {
        Literal {
            __text: format!("{n}u64"),
        }
    }

    pub fn i32_unsuffixed(n: i32) -> Literal {
        Literal {
            __text: n.to_string(),
        }
    }

    pub fn i32_suffixed(n: i32) -> Literal {
        Literal {
            __text: format!("{n}i32"),
        }
    }

    pub fn i64_unsuffixed(n: i64) -> Literal {
        Literal {
            __text: n.to_string(),
        }
    }

    pub fn i64_suffixed(n: i64) -> Literal {
        Literal {
            __text: format!("{n}i64"),
        }
    }

    pub fn f64_unsuffixed(n: f64) -> Literal {
        Literal {
            __text: format!("{:?}", n),
        }
    }

    pub fn f64_suffixed(n: f64) -> Literal {
        Literal {
            __text: format!("{n}f64"),
        }
    }

    pub fn span(&self) -> Span {
        Span
    }

    pub fn __tt(&self) -> String {
        self.__text.clone()
    }
}

impl std::fmt::Display for Literal {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.__text)
    }
}

/// The punctuation characters (a `Punct` is `Joint` when another follows directly).
fn __is_punct(c: char) -> bool {
    "+-*/%^!&|=<>@.,;:#$?~\\".contains(c)
}

fn __is_ident_start(c: char) -> bool {
    c.is_alphabetic() || c == '_'
}

fn __is_ident_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// The token trees of `text`, like rustc's lexer: comments dropped (`///` docs become
/// `#[doc = ".."]`), groups nested.
pub fn __lex(text: &str) -> Vec<TokenTree> {
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0usize;
    __lex_until(&chars, &mut i, ' ')
}

/// The text of `chars[from..to]`.
fn __slice(chars: &[char], from: usize, to: usize) -> String {
    chars[from..to].iter().collect()
}

/// Lex up to `close` (consumed); `' '` lexes to the end.
fn __lex_until(chars: &[char], i: &mut usize, close: char) -> Vec<TokenTree> {
    let mut out: Vec<TokenTree> = Vec::new();
    let n = chars.len();
    while *i < n {
        let c = chars[*i];
        let next = if *i + 1 < n { chars[*i + 1] } else { ' ' };
        let next2 = if *i + 2 < n { chars[*i + 2] } else { ' ' };
        if c == close && close != ' ' {
            *i += 1;
            return out;
        }
        if c.is_whitespace() {
            *i += 1;
            continue;
        }
        // Comments; doc comments are `#[doc = ".."]` attributes.
        if c == '/' && next == '/' {
            let start = *i;
            while *i < n && chars[*i] != '\n' {
                *i += 1;
            }
            let line = __slice(chars, start, *i).trim_end().to_string();
            let inner = line.starts_with("//!");
            if (line.starts_with("///") && !line.starts_with("////")) || inner {
                out.push(TokenTree::Punct(Punct::new(
                    '#',
                    if inner { Spacing::Joint } else { Spacing::Alone },
                )));
                if inner {
                    out.push(TokenTree::Punct(Punct::new('!', Spacing::Alone)));
                }
                let doc: String = line.chars().skip(3).collect();
                out.push(TokenTree::Group(Group::new(
                    Delimiter::Bracket,
                    TokenStream {
                        __text: format!("doc = {:?}", doc),
                    },
                )));
            }
            continue;
        }
        if c == '/' && next == '*' {
            *i += 2;
            while *i + 1 < n && !(chars[*i] == '*' && chars[*i + 1] == '/') {
                *i += 1;
            }
            *i += 2;
            continue;
        }
        if c == '(' || c == '[' || c == '{' {
            let (closing, delimiter) = if c == '(' {
                (')', Delimiter::Parenthesis)
            } else if c == '[' {
                (']', Delimiter::Bracket)
            } else {
                ('}', Delimiter::Brace)
            };
            *i += 1;
            let start = *i;
            let _inner = __lex_until(chars, i, closing);
            let end = if *i > start && *i <= n && chars[*i - 1] == closing {
                *i - 1
            } else {
                *i
            };
            let text = __slice(chars, start, end);
            out.push(TokenTree::Group(Group::new(
                delimiter,
                TokenStream {
                    __text: text.trim().to_string(),
                },
            )));
            continue;
        }
        // String-like literals: "..", r#".."#, b"..", b'.', br"..", c"..".
        let raw_at = |k: usize| -> bool {
            k < n && chars[k] == 'r' && k + 1 < n && (chars[k + 1] == '"' || chars[k + 1] == '#')
        };
        let prefix = if c == '"' {
            Some(0)
        } else if (c == 'b' || c == 'c') && (next == '"' || next == '\'') {
            Some(1)
        } else if c == 'b' && raw_at(*i + 1) {
            Some(1)
        } else if raw_at(*i) && !(next == '#' && __is_ident_start(next2)) {
            Some(0)
        } else {
            None
        };
        if let Some(skip) = prefix {
            let start = *i;
            *i += skip;
            if chars[*i] == 'r' {
                // Raw: `r###"..."###`
                *i += 1;
                let mut hashes = 0;
                while *i < n && chars[*i] == '#' {
                    hashes += 1;
                    *i += 1;
                }
                *i += 1;
                loop {
                    if *i >= n {
                        break;
                    }
                    if chars[*i] == '"' {
                        let mut k = 0;
                        while k < hashes && *i + 1 + k < n && chars[*i + 1 + k] == '#' {
                            k += 1;
                        }
                        if k == hashes {
                            *i += 1 + hashes;
                            break;
                        }
                    }
                    *i += 1;
                }
            } else {
                let quote = chars[*i];
                *i += 1;
                while *i < n && chars[*i] != quote {
                    if chars[*i] == '\\' {
                        *i += 1;
                    }
                    *i += 1;
                }
                *i += 1;
            }
            // A suffix: `"x"suffix`
            while *i < n && __is_ident_char(chars[*i]) {
                *i += 1;
            }
            let end = if *i > n { n } else { *i };
            out.push(TokenTree::Literal(Literal {
                __text: __slice(chars, start, end),
            }));
            continue;
        }
        if c == '\'' {
            // A char literal (`'x'`, `'\n'`) or a lifetime's `'`.
            if next == '\\' || (next != ' ' && next2 == '\'') {
                let start = *i;
                *i += 1;
                while *i < n && chars[*i] != '\'' {
                    if chars[*i] == '\\' {
                        *i += 1;
                    }
                    *i += 1;
                }
                *i += 1;
                let end = if *i > n { n } else { *i };
                out.push(TokenTree::Literal(Literal {
                    __text: __slice(chars, start, end),
                }));
            } else {
                out.push(TokenTree::Punct(Punct::new('\'', Spacing::Joint)));
                *i += 1;
            }
            continue;
        }
        if __is_ident_start(c) {
            let start = *i;
            if c == 'r' && next == '#' {
                *i += 2;
            }
            while *i < n && __is_ident_char(chars[*i]) {
                *i += 1;
            }
            out.push(TokenTree::Ident(Ident {
                __text: __slice(chars, start, *i),
            }));
            continue;
        }
        if c.is_ascii_digit() {
            let start = *i;
            let mut seen_dot = false;
            while *i < n {
                let d = chars[*i];
                let after = if *i + 1 < n { chars[*i + 1] } else { ' ' };
                if __is_ident_char(d) {
                    *i += 1;
                    // `1e-3`
                    if (d == 'e' || d == 'E')
                        && (after == '+' || after == '-')
                        && !__slice(chars, start, *i).starts_with("0x")
                    {
                        *i += 1;
                    }
                } else if d == '.' && !seen_dot && after != '.' && !__is_ident_start(after) {
                    seen_dot = true;
                    *i += 1;
                } else {
                    break;
                }
            }
            out.push(TokenTree::Literal(Literal {
                __text: __slice(chars, start, *i),
            }));
            continue;
        }
        let joint = __is_punct(next) && *i + 1 < n;
        out.push(TokenTree::Punct(Punct::new(
            c,
            if joint { Spacing::Joint } else { Spacing::Alone },
        )));
        *i += 1;
    }
    out
}
