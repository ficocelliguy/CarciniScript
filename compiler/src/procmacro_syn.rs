
// (syn prelude)
// The parts of `syn` 2 that macros use most, for proc-macro crates that use `syn`: parsing
// derive input, functions, structs and enums, literals, identifiers, types and paths, and the
// syntax tree types they produce (each turns back into tokens with `quote!`). Parsing works on
// the token trees of the input.

pub type SynResult<T> = std::result::Result<T, Error>;

/// `syn::Error`: a message, reported with `compile_error!` at the macro call.
#[derive(Clone, Debug)]
pub struct Error {
    __msg: String,
}

impl Error {
    pub fn new(_span: Span, message: &str) -> Error {
        Error {
            __msg: message.to_string(),
        }
    }

    pub fn new_spanned<T>(_tokens: T, message: &str) -> Error {
        Error {
            __msg: message.to_string(),
        }
    }

    pub fn span(&self) -> Span {
        Span
    }

    pub fn combine(&mut self, other: Error) {
        self.__msg = format!("{}\n{}", self.__msg, other.__msg);
    }

    pub fn to_compile_error(&self) -> TokenStream {
        TokenStream {
            __text: format!("::core::compile_error! {{ {:?} }}", self.__msg),
        }
    }

    pub fn into_compile_error(self) -> TokenStream {
        self.to_compile_error()
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.__msg)
    }
}

/// Types that can be parsed from a `ParseStream` (yours too: `impl Parse for Args`).
pub trait Parse {
    fn parse(input: ParseStream) -> SynResult<Self>
    where
        Self: Sized;
}

/// The whole of `tokens` as a `T`: leftover tokens are an error.
pub fn __parse_all<T: Parse>(tokens: TokenStream) -> SynResult<T> {
    let buf = ParseBuffer::__new(tokens);
    let value = T::parse(&buf)?;
    if !buf.is_empty() {
        return Err(Error {
            __msg: format!("unexpected token, {}", buf.__found_text()),
        });
    }
    Ok(value)
}

/// `syn::parse`, `syn::parse2`, `syn::parse_str` (renamed when the crate is prepared, so a
/// crate's own `fn parse` stays its own).
pub fn __sparse<T: Parse>(tokens: TokenStream) -> SynResult<T> {
    __parse_all::<T>(tokens)
}

pub fn __sparse2<T: Parse>(tokens: TokenStream) -> SynResult<T> {
    __parse_all::<T>(tokens)
}

pub fn __sparse_str<T: Parse>(s: &str) -> SynResult<T> {
    __parse_all::<T>(TokenStream {
        __text: s.to_string(),
    })
}

/// `parse_macro_input!(input with Punctuated::<Ident, Token![,]>::parse_terminated)`
pub fn __parse_with<T>(parser: fn(ParseStream) -> SynResult<T>, tokens: TokenStream) -> SynResult<T> {
    let buf = ParseBuffer::__new(tokens);
    let value = parser(&buf)?;
    if !buf.is_empty() {
        return Err(Error {
            __msg: format!("unexpected token, {}", buf.__found_text()),
        });
    }
    Ok(value)
}

/// `parse_quote!(..)`: the tokens `quote!` makes, parsed as the type wanted.
pub fn __parse_quote<T: Parse>(tokens: TokenStream) -> T {
    match __parse_all::<T>(tokens) {
        Ok(value) => value,
        Err(err) => panic!("failed to parse parse_quote! output: {}", err),
    }
}

/// `parse_macro_input!(input as DeriveInput)`: the parsed input, or the error as the macro's
/// output.
macro_rules! parse_macro_input {
    ($tokens:ident as $ty:ty) => {
        match __parse_all::<$ty>($tokens) {
            Ok(data) => data,
            Err(err) => return err.to_compile_error(),
        }
    };
    ($tokens:ident with $($parser:tt)+) => {
        match __parse_with($($parser)+, $tokens) {
            Ok(data) => data,
            Err(err) => return err.to_compile_error(),
        }
    };
    // (`let input: DeriveInput = parse_macro_input!(input);`: the type is the `let`'s.)
    ($tokens:ident) => {
        match __parse_all($tokens) {
            Ok(data) => data,
            Err(err) => return err.to_compile_error(),
        }
    };
}

/// `parse_quote!(Vec<#inner>)`: a syntax tree from quoted tokens.
macro_rules! parse_quote {
    ($($tt:tt)*) => {
        __parse_quote(quote!($($tt)*))
    };
}

/// `parenthesized!(content in input)`: `content` parses the group's inside.
macro_rules! parenthesized {
    ($content:ident in $input:expr) => {{
        $content = $input.__group(Delimiter::Parenthesis)?;
        TkParen
    }};
}

macro_rules! braced {
    ($content:ident in $input:expr) => {{
        $content = $input.__group(Delimiter::Brace)?;
        TkBrace
    }};
}

macro_rules! bracketed {
    ($content:ident in $input:expr) => {{
        $content = $input.__group(Delimiter::Bracket)?;
        TkBracket
    }};
}

/// Tokens and syntax types `peek` can look for (`Token![,]`, `Ident`, `LitStr`, ..).
pub trait __Peek {
    fn __peek_at(&self, input: &ParseBuffer, k: usize) -> bool;
    fn __display(&self) -> String;
}

/// `ParseStream`: a cursor over token trees, shared (`&`) by the parsers.
#[derive(Clone, Debug)]
pub struct ParseBuffer {
    __toks: Vec<TokenTree>,
    __pos: std::cell::Cell<usize>,
}

pub type ParseStream<'a> = &'a ParseBuffer;

impl ParseBuffer {
    pub fn __new(tokens: TokenStream) -> ParseBuffer {
        ParseBuffer {
            __toks: tokens.into_iter().collect(),
            __pos: std::cell::Cell::new(0),
        }
    }

    pub fn parse<T: Parse>(&self) -> SynResult<T> {
        T::parse(self)
    }

    pub fn call<T>(&self, parser: fn(ParseStream) -> SynResult<T>) -> SynResult<T> {
        parser(self)
    }

    pub fn peek<T: __Peek>(&self, token: T) -> bool {
        token.__peek_at(self, 0)
    }

    pub fn peek2<T: __Peek>(&self, token: T) -> bool {
        token.__peek_at(self, 1)
    }

    pub fn peek3<T: __Peek>(&self, token: T) -> bool {
        token.__peek_at(self, 2)
    }

    pub fn is_empty(&self) -> bool {
        self.__pos.get() >= self.__toks.len()
    }

    pub fn span(&self) -> Span {
        Span
    }

    pub fn error(&self, message: &str) -> Error {
        Error {
            __msg: message.to_string(),
        }
    }

    pub fn fork(&self) -> ParseBuffer {
        ParseBuffer {
            __toks: self.__toks.clone(),
            __pos: std::cell::Cell::new(self.__pos.get()),
        }
    }

    /// (`syn::parse::discouraged::Speculative`) Continue from where `fork` got to.
    pub fn advance_to(&self, fork: &ParseBuffer) {
        self.__pos.set(fork.__pos.get());
    }

    pub fn lookahead1(&self) -> Lookahead1 {
        Lookahead1 {
            __buf: self.fork(),
            __expected: std::cell::RefCell::new(Vec::new()),
        }
    }

    /// `input.parse_terminated(Ident::parse, Token![,])`: items with separators, up to the end.
    pub fn parse_terminated<T, P: Parse + __Peek>(
        &self,
        parser: fn(ParseStream) -> SynResult<T>,
        separator: P,
    ) -> SynResult<Punctuated<T, P>> {
        let mut out = Punctuated::new();
        while !self.is_empty() {
            out.push(parser(self)?);
            if self.is_empty() {
                break;
            }
            if !separator.__peek_at(self, 0) {
                return Err(Error {
                    __msg: format!("expected {}, {}", separator.__display(), self.__found_text()),
                });
            }
            out.__punct.push(P::parse(self)?);
        }
        Ok(out)
    }

    // ---- for the parsers here

    pub fn __peek_tree(&self, k: usize) -> Option<TokenTree> {
        self.__toks.get(self.__pos.get() + k).cloned()
    }

    pub fn __bump(&self) -> Option<TokenTree> {
        let t = self.__peek_tree(0);
        if t.is_some() {
            self.__pos.set(self.__pos.get() + 1);
        }
        t
    }

    /// The rest of the tokens (consumed).
    pub fn __rest(&self) -> Vec<TokenTree> {
        let out: Vec<TokenTree> = self.__toks[self.__pos.get()..].to_vec();
        self.__pos.set(self.__toks.len());
        out
    }

    /// Advance past tokens a sub-parser took from the rest.
    pub fn __advance(&self, n: usize) {
        self.__pos.set(self.__pos.get() + n);
    }

    pub fn __found_text(&self) -> String {
        match self.__peek_tree(0) {
            Some(t) => format!("found `{}`", t.__tt()),
            None => "found the end of the input".to_string(),
        }
    }

    /// Whether a punctuation (`::`, `->`: one `Punct` per character) or a keyword comes `k`
    /// trees ahead.
    pub fn __peek_token(&self, k: usize, text: &str, keyword: bool) -> bool {
        if keyword {
            return matches!(self.__peek_tree(k), Some(TokenTree::Ident(i)) if i.__tt() == text);
        }
        let chars: Vec<char> = text.chars().collect();
        for (j, c) in chars.iter().enumerate() {
            match self.__peek_tree(k + j) {
                Some(TokenTree::Punct(p)) if p.as_char() == *c => {
                    // All but the last are joined to the next (`::` but not `: :`).
                    if j + 1 < chars.len() && p.spacing() != Spacing::Joint {
                        return false;
                    }
                }
                _ => return false,
            }
        }
        true
    }

    pub fn __token(&self, text: &str, keyword: bool) -> SynResult<()> {
        if !self.__peek_token(0, text, keyword) {
            return Err(Error {
                __msg: format!("expected `{}`, {}", text, self.__found_text()),
            });
        }
        let n = if keyword { 1 } else { text.chars().count() };
        self.__advance(n);
        Ok(())
    }

    /// The inside of the next group, if it has this delimiter (`parenthesized!` and friends).
    pub fn __group(&self, delimiter: Delimiter) -> SynResult<ParseBuffer> {
        match self.__peek_tree(0) {
            Some(TokenTree::Group(g)) if g.delimiter() == delimiter => {
                self.__advance(1);
                Ok(ParseBuffer::__new(g.stream()))
            }
            _ => {
                let what = match delimiter {
                    Delimiter::Parenthesis => "parentheses",
                    Delimiter::Brace => "curly braces",
                    Delimiter::Bracket => "square brackets",
                    Delimiter::None => "a group",
                };
                Err(Error {
                    __msg: format!("expected {what}, {}", self.__found_text()),
                })
            }
        }
    }

    /// Trees up to a separator outside `<..>` (not consumed): `,`, `;`, and `=`/`{` when
    /// `stop_at_eq` (a type ends there).
    pub fn __until_separator(&self, stop_at_eq: bool) -> Vec<TokenTree> {
        let mut out = Vec::new();
        let mut depth = 0i32;
        while let Some(t) = self.__peek_tree(0) {
            let prev_arrow = out
                .last()
                .is_some_and(|p: &TokenTree| __is_p(p, '-') || __is_p(p, '='));
            if depth == 0 && (__is_p(&t, ',') || __is_p(&t, ';')) {
                break;
            }
            if depth == 0 && stop_at_eq {
                let eq_alone = __is_p(&t, '=')
                    && !matches!(self.__peek_tree(1), Some(TokenTree::Punct(p)) if p.as_char() == '=' || p.as_char() == '>');
                let brace = matches!(&t, TokenTree::Group(g) if g.delimiter() == Delimiter::Brace);
                if eq_alone || brace {
                    break;
                }
                if __is_p(&t, '>') && !prev_arrow {
                    break;
                }
            }
            if __is_p(&t, '<') {
                depth += 1;
            } else if __is_p(&t, '>') && !prev_arrow && depth > 0 {
                depth -= 1;
            }
            // `=>` ends an expression (a match arm).
            if depth == 0
                && !stop_at_eq
                && __is_p(&t, '=')
                && matches!(self.__peek_tree(1), Some(TokenTree::Punct(p)) if p.as_char() == '>')
            {
                break;
            }
            out.push(t);
            self.__advance(1);
        }
        out
    }
}

/// `input.lookahead1()`: peeks that remember what was expected, for one error message.
pub struct Lookahead1 {
    __buf: ParseBuffer,
    __expected: std::cell::RefCell<Vec<String>>,
}

impl Lookahead1 {
    pub fn peek<T: __Peek>(&self, token: T) -> bool {
        if token.__peek_at(&self.__buf, 0) {
            return true;
        }
        self.__expected.borrow_mut().push(token.__display());
        false
    }

    pub fn error(self) -> Error {
        let expected = self.__expected.borrow().clone();
        let msg = match expected.len() {
            0 => "unexpected token".to_string(),
            1 => format!("expected {}", expected[0]),
            2 => format!("expected {} or {}", expected[0], expected[1]),
            _ => format!("expected one of: {}", expected.join(", ")),
        };
        Error { __msg: msg }
    }
}

/// The keywords, which `Ident` doesn't parse or peek.
fn __is_keyword(word: &str) -> bool {
    matches!(
        word,
        "as" | "async" | "await" | "break" | "const" | "continue" | "crate" | "dyn" | "else"
            | "enum" | "extern" | "false" | "fn" | "for" | "if" | "impl" | "in" | "let"
            | "loop" | "match" | "mod" | "move" | "mut" | "pub" | "ref" | "return" | "self"
            | "Self" | "static" | "struct" | "super" | "trait" | "true" | "type" | "unsafe"
            | "use" | "where" | "while" | "abstract" | "become" | "box" | "do" | "final"
            | "macro" | "override" | "priv" | "typeof" | "unsized" | "virtual" | "yield"
            | "try" | "gen"
    )
}

/// `input.peek(Ident)` (written `IDENT` by the time it's compiled).
#[derive(Clone, Copy, Debug, Default)]
pub struct IDENT;

impl __Peek for IDENT {
    fn __peek_at(&self, input: &ParseBuffer, k: usize) -> bool {
        matches!(input.__peek_tree(k), Some(TokenTree::Ident(i)) if !__is_keyword(&i.__tt()))
    }
    fn __display(&self) -> String {
        "identifier".to_string()
    }
}

fn __lit_kind(t: &Option<TokenTree>) -> &'static str {
    match t {
        Some(TokenTree::Literal(l)) => {
            let s = l.__tt();
            if s.starts_with('"') || s.starts_with("r\"") || s.starts_with("r#") {
                "str"
            } else if s.starts_with('\'') {
                "char"
            } else if s.starts_with('b') || s.starts_with('c') {
                "bytes"
            } else if s.contains('.') || s.ends_with("f32") || s.ends_with("f64") || (s.contains('e') && !s.starts_with("0x")) {
                "float"
            } else {
                "int"
            }
        }
        Some(TokenTree::Ident(i)) if i.__tt() == "true" || i.__tt() == "false" => "bool",
        _ => "",
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct LITSTR;

impl __Peek for LITSTR {
    fn __peek_at(&self, input: &ParseBuffer, k: usize) -> bool {
        __lit_kind(&input.__peek_tree(k)) == "str"
    }
    fn __display(&self) -> String {
        "string literal".to_string()
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct LITINT;

impl __Peek for LITINT {
    fn __peek_at(&self, input: &ParseBuffer, k: usize) -> bool {
        __lit_kind(&input.__peek_tree(k)) == "int"
    }
    fn __display(&self) -> String {
        "integer literal".to_string()
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct LITFLOAT;

impl __Peek for LITFLOAT {
    fn __peek_at(&self, input: &ParseBuffer, k: usize) -> bool {
        __lit_kind(&input.__peek_tree(k)) == "float"
    }
    fn __display(&self) -> String {
        "floating point literal".to_string()
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct LITBOOL;

impl __Peek for LITBOOL {
    fn __peek_at(&self, input: &ParseBuffer, k: usize) -> bool {
        __lit_kind(&input.__peek_tree(k)) == "bool"
    }
    fn __display(&self) -> String {
        "boolean literal".to_string()
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct LIT;

impl __Peek for LIT {
    fn __peek_at(&self, input: &ParseBuffer, k: usize) -> bool {
        !__lit_kind(&input.__peek_tree(k)).is_empty()
    }
    fn __display(&self) -> String {
        "literal".to_string()
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct LIFETIME;

impl __Peek for LIFETIME {
    fn __peek_at(&self, input: &ParseBuffer, k: usize) -> bool {
        matches!(input.__peek_tree(k), Some(TokenTree::Punct(p)) if p.as_char() == '\'')
    }
    fn __display(&self) -> String {
        "lifetime".to_string()
    }
}

/// Items separated by punctuation (`a: u8, b: u8`).
#[derive(Clone, Debug)]
pub struct Punctuated<T, P> {
    __items: Vec<T>,
    __punct: Vec<P>,
}

impl<T: Parse, P: Parse + __Peek + Default> Punctuated<T, P> {
    /// `Punctuated::<Ident, Token![,]>::parse_terminated(input)`
    pub fn parse_terminated(input: ParseStream) -> SynResult<Punctuated<T, P>> {
        let separator = P::default();
        let mut out = Punctuated::new();
        while !input.is_empty() {
            out.push(T::parse(input)?);
            if input.is_empty() {
                break;
            }
            if !separator.__peek_at(input, 0) {
                return Err(Error {
                    __msg: format!("expected {}, {}", separator.__display(), input.__found_text()),
                });
            }
            out.__punct.push(P::parse(input)?);
        }
        Ok(out)
    }

    /// `Punctuated::parse_separated_nonempty(input)`: at least one item; stops after an item
    /// with no separator.
    pub fn parse_separated_nonempty(input: ParseStream) -> SynResult<Punctuated<T, P>> {
        let separator = P::default();
        let mut out = Punctuated::new();
        loop {
            out.push(T::parse(input)?);
            if !separator.__peek_at(input, 0) {
                break;
            }
            out.__punct.push(P::parse(input)?);
        }
        Ok(out)
    }
}

impl<T, P> Punctuated<T, P> {
    pub fn new() -> Punctuated<T, P> {
        Punctuated {
            __items: Vec::new(),
            __punct: Vec::new(),
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.__items.iter()
    }

    pub fn len(&self) -> usize {
        self.__items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.__items.is_empty()
    }

    pub fn first(&self) -> Option<&T> {
        self.__items.first()
    }

    pub fn last(&self) -> Option<&T> {
        self.__items.last()
    }

    pub fn push(&mut self, value: T) {
        self.__items.push(value);
    }
}

impl<T, P> IntoIterator for Punctuated<T, P> {
    type Item = T;
    fn into_iter(self) -> impl Iterator<Item = T> {
        self.__items.into_iter()
    }
}

/// `#[name ..]`
#[derive(Clone, Debug)]
pub struct Attribute {
    __text: String,
    __path: Path,
    __args: TokenStream,
}

impl Attribute {
    pub fn path(&self) -> &Path {
        &self.__path
    }

    /// The tokens in `#[name(..)]`'s parentheses, parsed.
    pub fn parse_args<T: Parse>(&self) -> SynResult<T> {
        __parse_all::<T>(self.__args.clone())
    }

    pub fn span(&self) -> Span {
        Span
    }

    /// `input.call(Attribute::parse_outer)`: the `#[..]` attributes here.
    pub fn parse_outer(input: ParseStream) -> SynResult<Vec<Attribute>> {
        let rest = input.__toks[input.__pos.get()..].to_vec();
        let mut c = __Cur::new(rest);
        let attrs = c.attrs();
        input.__advance(c.pos);
        Ok(attrs)
    }

    /// `#![..]` attributes.
    pub fn parse_inner(input: ParseStream) -> SynResult<Vec<Attribute>> {
        Attribute::parse_outer(input)
    }

    pub fn __tt(&self) -> String {
        self.__text.clone()
    }
}

#[derive(Clone, Debug)]
pub struct PathSegment {
    pub ident: Ident,
}

impl PathSegment {
    pub fn __tt(&self) -> String {
        self.ident.__tt()
    }
}

/// `a::b::C`
#[derive(Clone, Debug)]
pub struct Path {
    __text: String,
    pub segments: Punctuated<PathSegment, TkPathSep>,
}

impl Path {
    pub fn is_ident(&self, name: &str) -> bool {
        self.__text == name
    }

    pub fn get_ident(&self) -> Option<&Ident> {
        if self.segments.len() == 1 {
            self.segments.first().map(|s| &s.ident)
        } else {
            None
        }
    }

    pub fn span(&self) -> Span {
        Span
    }

    pub fn __tt(&self) -> String {
        self.__text.clone()
    }
}

/// `pub`, `pub(crate)`, or nothing.
#[derive(Clone, Debug)]
pub enum Visibility {
    Public(TkPub),
    Restricted(VisRestricted),
    Inherited,
}

#[derive(Clone, Debug)]
pub struct VisRestricted {
    __text: String,
}

impl Visibility {
    pub fn __tt(&self) -> String {
        match self {
            Visibility::Public(_) => "pub".to_string(),
            Visibility::Restricted(r) => r.__text.clone(),
            Visibility::Inherited => String::new(),
        }
    }
}

/// A type, as its tokens.
#[derive(Clone, Debug)]
pub struct Type {
    __text: String,
}

impl Type {
    pub fn span(&self) -> Span {
        Span
    }

    pub fn __tt(&self) -> String {
        self.__text.clone()
    }
}

/// An expression, as its tokens.
#[derive(Clone, Debug)]
pub struct Expr {
    __text: String,
}

impl Expr {
    pub fn span(&self) -> Span {
        Span
    }

    pub fn __tt(&self) -> String {
        self.__text.clone()
    }
}

/// A pattern, as its tokens.
#[derive(Clone, Debug)]
pub struct Pat {
    __text: String,
}

impl Pat {
    pub fn __tt(&self) -> String {
        self.__text.clone()
    }
}

/// `{ .. }`
#[derive(Clone, Debug)]
pub struct Block {
    __text: String,
}

impl Block {
    pub fn __tt(&self) -> String {
        self.__text.clone()
    }
}

/// `'a`
#[derive(Clone, Debug)]
pub struct Lifetime {
    pub ident: Ident,
}

impl Lifetime {
    pub fn __tt(&self) -> String {
        format!("'{}", self.ident.__tt())
    }
}

#[derive(Clone, Debug)]
pub struct LifetimeParam {
    pub lifetime: Lifetime,
    __bounds: String,
}

#[derive(Clone, Debug)]
pub struct TypeParam {
    pub ident: Ident,
    __bounds: String,
    __default: String,
}

#[derive(Clone, Debug)]
pub struct ConstParam {
    pub ident: Ident,
    pub ty: Type,
    __default: String,
}

#[derive(Clone, Debug)]
pub enum GenericParam {
    Lifetime(LifetimeParam),
    Type(TypeParam),
    Const(ConstParam),
}

impl GenericParam {
    /// As in `impl<..>`: with bounds, without defaults.
    fn __decl(&self) -> String {
        match self {
            GenericParam::Lifetime(l) => {
                if l.__bounds.is_empty() {
                    l.lifetime.__tt()
                } else {
                    format!("{}: {}", l.lifetime.__tt(), l.__bounds)
                }
            }
            GenericParam::Type(t) => {
                if t.__bounds.is_empty() {
                    t.ident.__tt()
                } else {
                    format!("{}: {}", t.ident.__tt(), t.__bounds)
                }
            }
            GenericParam::Const(c) => format!("const {}: {}", c.ident.__tt(), c.ty.__tt()),
        }
    }

    /// As in `Type<..>`: the name only.
    fn __name(&self) -> String {
        match self {
            GenericParam::Lifetime(l) => l.lifetime.__tt(),
            GenericParam::Type(t) => t.ident.__tt(),
            GenericParam::Const(c) => c.ident.__tt(),
        }
    }

    pub fn __tt(&self) -> String {
        let d = match self {
            GenericParam::Type(t) => t.__default.clone(),
            GenericParam::Const(c) => c.__default.clone(),
            GenericParam::Lifetime(_) => String::new(),
        };
        if d.is_empty() {
            self.__decl()
        } else {
            format!("{} = {}", self.__decl(), d)
        }
    }
}

/// `where ..`
#[derive(Clone, Debug)]
pub struct WhereClause {
    __text: String,
}

impl WhereClause {
    pub fn __tt(&self) -> String {
        self.__text.clone()
    }
}

/// `<T: Clone, 'a>` and the `where` clause.
#[derive(Clone, Debug)]
pub struct Generics {
    pub params: Punctuated<GenericParam, TkComma>,
    pub where_clause: Option<WhereClause>,
}

/// `impl #impl_generics Trait for Name #ty_generics #where_clause`
#[derive(Clone, Debug)]
pub struct ImplGenerics {
    __text: String,
}

impl ImplGenerics {
    pub fn __tt(&self) -> String {
        self.__text.clone()
    }
}

#[derive(Clone, Debug)]
pub struct TypeGenerics {
    __text: String,
}

impl TypeGenerics {
    pub fn __tt(&self) -> String {
        self.__text.clone()
    }
}

impl Generics {
    pub fn split_for_impl(&self) -> (ImplGenerics, TypeGenerics, Option<&WhereClause>) {
        if self.params.is_empty() {
            return (
                ImplGenerics {
                    __text: String::new(),
                },
                TypeGenerics {
                    __text: String::new(),
                },
                self.where_clause.as_ref(),
            );
        }
        let decls: Vec<String> = self.params.iter().map(|p| p.__decl()).collect();
        let names: Vec<String> = self.params.iter().map(|p| p.__name()).collect();
        (
            ImplGenerics {
                __text: format!("<{}>", decls.join(", ")),
            },
            TypeGenerics {
                __text: format!("<{}>", names.join(", ")),
            },
            self.where_clause.as_ref(),
        )
    }

    pub fn type_params(&self) -> impl Iterator<Item = &TypeParam> {
        self.params.iter().filter_map(|p| match p {
            GenericParam::Type(t) => Some(t),
            _ => None,
        })
    }

    pub fn __tt(&self) -> String {
        if self.params.is_empty() {
            return String::new();
        }
        let all: Vec<String> = self.params.iter().map(|p| p.__tt()).collect();
        format!("<{}>", all.join(", "))
    }
}

/// A field: `pub name: Type` (no name in a tuple struct).
#[derive(Clone, Debug)]
pub struct Field {
    pub attrs: Vec<Attribute>,
    pub vis: Visibility,
    pub ident: Option<Ident>,
    pub ty: Type,
}

impl Field {
    pub fn span(&self) -> Span {
        Span
    }

    pub fn __tt(&self) -> String {
        let attrs: Vec<String> = self.attrs.iter().map(|a| a.__tt()).collect();
        let mut out = attrs.join(" ");
        let vis = self.vis.__tt();
        if !vis.is_empty() {
            out = format!("{out} {vis}");
        }
        match &self.ident {
            Some(i) => format!("{out} {}: {}", i.__tt(), self.ty.__tt()).trim().to_string(),
            None => format!("{out} {}", self.ty.__tt()).trim().to_string(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct FieldsNamed {
    pub named: Punctuated<Field, TkComma>,
}

#[derive(Clone, Debug)]
pub struct FieldsUnnamed {
    pub unnamed: Punctuated<Field, TkComma>,
}

#[derive(Clone, Debug)]
pub enum Fields {
    Named(FieldsNamed),
    Unnamed(FieldsUnnamed),
    Unit,
}

impl Fields {
    fn __list(&self) -> Vec<Field> {
        match self {
            Fields::Named(f) => f.named.__items.clone(),
            Fields::Unnamed(f) => f.unnamed.__items.clone(),
            Fields::Unit => Vec::new(),
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = Field> {
        self.__list().into_iter()
    }

    pub fn len(&self) -> usize {
        self.__list().len()
    }

    pub fn is_empty(&self) -> bool {
        self.__list().is_empty()
    }

    pub fn __tt(&self) -> String {
        let all: Vec<String> = self.__list().iter().map(|f| f.__tt()).collect();
        match self {
            Fields::Named(_) => format!("{{ {} }}", all.join(", ")),
            Fields::Unnamed(_) => format!("({})", all.join(", ")),
            Fields::Unit => String::new(),
        }
    }
}

impl IntoIterator for Fields {
    type Item = Field;
    fn into_iter(self) -> impl Iterator<Item = Field> {
        self.__list().into_iter()
    }
}

/// An enum variant: `Name`, `Name(T)`, `Name { a: T }`, `Name = 3`.
#[derive(Clone, Debug)]
pub struct Variant {
    pub attrs: Vec<Attribute>,
    pub ident: Ident,
    pub fields: Fields,
    pub discriminant: Option<(TkEq, Expr)>,
}

impl Variant {
    pub fn span(&self) -> Span {
        Span
    }
}

#[derive(Clone, Debug)]
pub struct DataStruct {
    pub fields: Fields,
}

#[derive(Clone, Debug)]
pub struct DataEnum {
    pub variants: Punctuated<Variant, TkComma>,
}

#[derive(Clone, Debug)]
pub struct DataUnion {
    pub fields: FieldsNamed,
}

#[derive(Clone, Debug)]
pub enum Data {
    Struct(DataStruct),
    Enum(DataEnum),
    Union(DataUnion),
}

/// A `#[derive]` macro's input: a struct, enum or union.
#[derive(Clone, Debug)]
pub struct DeriveInput {
    pub attrs: Vec<Attribute>,
    pub vis: Visibility,
    pub ident: Ident,
    pub generics: Generics,
    pub data: Data,
}

/// `struct Name<..> { .. }` (for attribute macros).
#[derive(Clone, Debug)]
pub struct ItemStruct {
    pub attrs: Vec<Attribute>,
    pub vis: Visibility,
    pub ident: Ident,
    pub generics: Generics,
    pub fields: Fields,
}

impl ItemStruct {
    pub fn __tt(&self) -> String {
        let attrs: Vec<String> = self.attrs.iter().map(|a| a.__tt()).collect();
        let (_, _, wc) = self.generics.split_for_impl();
        let wc = wc.map(|w| w.__tt()).unwrap_or_default();
        let body = match &self.fields {
            Fields::Named(_) => format!("{} {}", wc, self.fields.__tt()),
            Fields::Unnamed(_) => format!("{} {};", self.fields.__tt(), wc),
            Fields::Unit => format!("{wc};"),
        };
        format!(
            "{} {} struct {}{} {}",
            attrs.join(" "),
            self.vis.__tt(),
            self.ident.__tt(),
            self.generics.__tt(),
            body
        )
    }
}

/// `enum Name<..> { .. }` (for attribute macros).
#[derive(Clone, Debug)]
pub struct ItemEnum {
    pub attrs: Vec<Attribute>,
    pub vis: Visibility,
    pub ident: Ident,
    pub generics: Generics,
    pub variants: Punctuated<Variant, TkComma>,
}

impl ItemEnum {
    pub fn __tt(&self) -> String {
        let attrs: Vec<String> = self.attrs.iter().map(|a| a.__tt()).collect();
        let (_, _, wc) = self.generics.split_for_impl();
        let wc = wc.map(|w| w.__tt()).unwrap_or_default();
        let vs: Vec<String> = self.variants.iter().map(|v| __variant_text(v)).collect();
        format!(
            "{} {} enum {}{} {} {{ {} }}",
            attrs.join(" "),
            self.vis.__tt(),
            self.ident.__tt(),
            self.generics.__tt(),
            wc,
            vs.join(", ")
        )
    }
}

fn __variant_text(v: &Variant) -> String {
    let attrs: Vec<String> = v.attrs.iter().map(|a| a.__tt()).collect();
    let disc = match &v.discriminant {
        Some((_, e)) => format!(" = {}", e.__tt()),
        None => String::new(),
    };
    format!("{} {}{}{}", attrs.join(" "), v.ident.__tt(), v.fields.__tt(), disc)
}

/// `self`, `&self`, `&mut self`, `mut self`, `self: Type`
#[derive(Clone, Debug)]
pub struct Receiver {
    __text: String,
}

/// `pat: Type`
#[derive(Clone, Debug)]
pub struct PatType {
    pub attrs: Vec<Attribute>,
    pub pat: Box<Pat>,
    pub ty: Box<Type>,
}

#[derive(Clone, Debug)]
pub enum FnArg {
    Receiver(Receiver),
    Typed(PatType),
}

impl FnArg {
    pub fn __tt(&self) -> String {
        match self {
            FnArg::Receiver(r) => r.__text.clone(),
            FnArg::Typed(t) => format!("{}: {}", t.pat.__tt(), t.ty.__tt()),
        }
    }
}

/// `-> Type`, or nothing.
#[derive(Clone, Debug)]
pub enum ReturnType {
    Default,
    Type(TkRArrow, Box<Type>),
}

impl ReturnType {
    pub fn __tt(&self) -> String {
        match self {
            ReturnType::Default => String::new(),
            ReturnType::Type(_, t) => format!("-> {}", t.__tt()),
        }
    }
}

/// `async fn name<..>(args) -> Out where ..`
#[derive(Clone, Debug)]
pub struct Signature {
    pub constness: Option<TkConst>,
    pub asyncness: Option<TkAsync>,
    pub unsafety: Option<TkUnsafe>,
    pub ident: Ident,
    pub generics: Generics,
    pub inputs: Punctuated<FnArg, TkComma>,
    pub output: ReturnType,
}

impl Signature {
    pub fn __tt(&self) -> String {
        let mut out = String::new();
        if self.constness.is_some() {
            out.push_str("const ");
        }
        if self.asyncness.is_some() {
            out.push_str("async ");
        }
        if self.unsafety.is_some() {
            out.push_str("unsafe ");
        }
        let args: Vec<String> = self.inputs.iter().map(|a| a.__tt()).collect();
        let (_, _, wc) = self.generics.split_for_impl();
        let wc = wc.map(|w| w.__tt()).unwrap_or_default();
        format!(
            "{out}fn {}{}({}) {} {}",
            self.ident.__tt(),
            self.generics.__tt(),
            args.join(", "),
            self.output.__tt(),
            wc
        )
    }
}

/// A function item (for attribute macros).
#[derive(Clone, Debug)]
pub struct ItemFn {
    pub attrs: Vec<Attribute>,
    pub vis: Visibility,
    pub sig: Signature,
    pub block: Box<Block>,
}

impl ItemFn {
    pub fn __tt(&self) -> String {
        let attrs: Vec<String> = self.attrs.iter().map(|a| a.__tt()).collect();
        format!(
            "{} {} {} {}",
            attrs.join(" "),
            self.vis.__tt(),
            self.sig.__tt(),
            self.block.__tt()
        )
    }
}

/// `"text"`
#[derive(Clone, Debug)]
pub struct LitStr {
    __text: String,
}

impl LitStr {
    pub fn new(value: &str, _span: Span) -> LitStr {
        LitStr {
            __text: format!("{:?}", value),
        }
    }

    /// The string's value (escapes resolved).
    pub fn value(&self) -> String {
        let t = &self.__text;
        if t.starts_with('r') {
            let inner: String = t.chars().skip_while(|c| *c == 'r' || *c == '#').collect();
            let inner = inner.trim_end_matches('#');
            return inner[1..inner.len() - 1].to_string();
        }
        let chars: Vec<char> = t.chars().collect();
        let mut out = String::new();
        let mut i = 1;
        while i + 1 < chars.len() {
            let c = chars[i];
            if c == '\\' && i + 2 < chars.len() {
                let e = chars[i + 1];
                i += 2;
                match e {
                    'n' => out.push('\n'),
                    't' => out.push('\t'),
                    'r' => out.push('\r'),
                    '0' => out.push('\0'),
                    'u' => {
                        // `\u{1F600}`
                        let mut hex = String::new();
                        while i < chars.len() && chars[i] != '}' {
                            if chars[i] != '{' {
                                hex.push(chars[i]);
                            }
                            i += 1;
                        }
                        i += 1;
                        if let Some(ch) = u32::from_str_radix(&hex, 16).ok().and_then(|n| char::from_u32(n)) {
                            out.push(ch);
                        }
                    }
                    '\n' => {
                        while i < chars.len() && chars[i].is_whitespace() {
                            i += 1;
                        }
                    }
                    other => out.push(other),
                }
            } else {
                out.push(c);
                i += 1;
            }
        }
        out
    }

    pub fn span(&self) -> Span {
        Span
    }

    pub fn __tt(&self) -> String {
        self.__text.clone()
    }
}

/// `42`, `7u8`
#[derive(Clone, Debug)]
pub struct LitInt {
    __text: String,
}

impl LitInt {
    pub fn base10_digits(&self) -> String {
        let digits: String = self
            .__text
            .chars()
            .take_while(|c| c.is_ascii_digit() || *c == '_')
            .filter(|c| *c != '_')
            .collect();
        digits
    }

    pub fn suffix(&self) -> String {
        self.__text
            .chars()
            .skip_while(|c| c.is_ascii_digit() || *c == '_')
            .collect()
    }

    pub fn span(&self) -> Span {
        Span
    }

    pub fn __tt(&self) -> String {
        self.__text.clone()
    }
}

// ---- parsing

/// The text of tokens, as `quote!` would write them.
fn __join(toks: &[TokenTree]) -> String {
    let mut out = String::new();
    for t in toks {
        out.push_str(&t.__tt());
        if !t.__joint() {
            out.push(' ');
        }
    }
    out.trim().to_string()
}

fn __is_p(t: &TokenTree, c: char) -> bool {
    match t {
        TokenTree::Punct(p) => p.as_char() == c,
        _ => false,
    }
}

fn __is_i(t: &TokenTree, word: &str) -> bool {
    match t {
        TokenTree::Ident(i) => i.__tt() == word,
        _ => false,
    }
}

/// Split at commas outside `<..>` (groups are single trees already).
fn __split_commas(toks: &[TokenTree]) -> Vec<Vec<TokenTree>> {
    let mut out: Vec<Vec<TokenTree>> = vec![Vec::new()];
    let mut depth = 0i32;
    let mut k = 0;
    while k < toks.len() {
        let t = &toks[k];
        // `->` and `=>` aren't angle brackets.
        let arrow = k > 0 && __is_p(t, '>') && (__is_p(&toks[k - 1], '-') || __is_p(&toks[k - 1], '='));
        if __is_p(t, '<') {
            depth += 1;
        } else if __is_p(t, '>') && !arrow && depth > 0 {
            depth -= 1;
        }
        if __is_p(t, ',') && depth == 0 {
            out.push(Vec::new());
        } else {
            out.last_mut().unwrap().push(t.clone());
        }
        k += 1;
    }
    if out.last().is_some_and(|l| l.is_empty()) {
        out.pop();
    }
    out
}

/// A cursor over token trees.
struct __Cur {
    toks: Vec<TokenTree>,
    pos: usize,
}

impl __Cur {
    fn new(toks: Vec<TokenTree>) -> __Cur {
        __Cur { toks, pos: 0 }
    }

    fn of(stream: TokenStream) -> __Cur {
        __Cur::new(stream.into_iter().collect())
    }

    fn at_end(&self) -> bool {
        self.pos >= self.toks.len()
    }

    fn peek(&self) -> Option<TokenTree> {
        self.toks.get(self.pos).cloned()
    }

    fn peek_at(&self, k: usize) -> Option<TokenTree> {
        self.toks.get(self.pos + k).cloned()
    }

    fn bump(&mut self) -> Option<TokenTree> {
        let t = self.toks.get(self.pos).cloned();
        self.pos += 1;
        t
    }

    fn punct(&self, c: char) -> bool {
        self.peek().is_some_and(|t| __is_p(&t, c))
    }

    fn word(&self, w: &str) -> bool {
        self.peek().is_some_and(|t| __is_i(&t, w))
    }

    fn rest(&mut self) -> Vec<TokenTree> {
        let out: Vec<TokenTree> = self.toks[self.pos..].to_vec();
        self.pos = self.toks.len();
        out
    }

    fn found(&self) -> String {
        match self.peek() {
            Some(t) => format!("found `{}`", t.__tt()),
            None => "found the end of the input".to_string(),
        }
    }

    fn ident(&mut self) -> SynResult<Ident> {
        match self.peek() {
            Some(TokenTree::Ident(i)) => {
                self.pos += 1;
                Ok(i)
            }
            _ => Err(Error {
                __msg: format!("expected identifier, {}", self.found()),
            }),
        }
    }

    /// `#[..]` and `#![..]` attributes.
    fn attrs(&mut self) -> Vec<Attribute> {
        let mut out = Vec::new();
        while self.punct('#') {
            let bang = self.peek_at(1).is_some_and(|t| __is_p(&t, '!'));
            let at = if bang { 2 } else { 1 };
            let Some(TokenTree::Group(g)) = self.peek_at(at) else {
                break;
            };
            if g.delimiter() != Delimiter::Bracket {
                break;
            }
            self.pos += at + 1;
            let inner: Vec<TokenTree> = g.stream().into_iter().collect();
            let mut path_end = 0;
            while path_end < inner.len()
                && (matches!(inner[path_end], TokenTree::Ident(_)) || __is_p(&inner[path_end], ':'))
            {
                path_end += 1;
            }
            let path = __path_of(&inner[..path_end]);
            let args = match inner.get(path_end) {
                Some(TokenTree::Group(a)) => a.stream(),
                Some(_) => TokenStream {
                    __text: __join(&inner[path_end..]),
                },
                None => TokenStream::new(),
            };
            out.push(Attribute {
                __text: format!("#{}[{}]", if bang { "!" } else { "" }, __join(&inner)),
                __path: path,
                __args: args,
            });
        }
        out
    }

    fn vis(&mut self) -> Visibility {
        if !self.word("pub") {
            return Visibility::Inherited;
        }
        self.pos += 1;
        if let Some(TokenTree::Group(g)) = self.peek()
            && g.delimiter() == Delimiter::Parenthesis
        {
            self.pos += 1;
            return Visibility::Restricted(VisRestricted {
                __text: format!("pub({})", g.stream().__tt()),
            });
        }
        Visibility::Public(TkPub)
    }

    /// `<..>` after a name.
    fn generics(&mut self) -> Generics {
        let mut params = Punctuated::new();
        if self.punct('<') {
            self.pos += 1;
            let start = self.pos;
            let mut depth = 1;
            while depth > 0 && !self.at_end() {
                let t = self.bump().unwrap();
                let prev_arrow = self.pos >= 2 && self.pos - 2 >= start
                    && (__is_p(&self.toks[self.pos - 2], '-') || __is_p(&self.toks[self.pos - 2], '='));
                if __is_p(&t, '<') {
                    depth += 1;
                } else if __is_p(&t, '>') && !prev_arrow {
                    depth -= 1;
                }
            }
            let inner: Vec<TokenTree> = self.toks[start..self.pos - 1].to_vec();
            for p in __split_commas(&inner) {
                params.push(__generic_param(&p));
            }
        }
        Generics {
            params,
            where_clause: None,
        }
    }

    /// `where ..` up to a `{ .. }` group or `;`.
    fn where_clause(&mut self) -> Option<WhereClause> {
        if !self.word("where") {
            return None;
        }
        let start = self.pos;
        while !self.at_end() {
            match self.peek() {
                Some(TokenTree::Group(g)) if g.delimiter() == Delimiter::Brace => break,
                Some(t) if __is_p(&t, ';') => break,
                _ => self.pos += 1,
            }
        }
        Some(WhereClause {
            __text: __join(&self.toks[start..self.pos]),
        })
    }
}

fn __path_of(toks: &[TokenTree]) -> Path {
    let mut segments = Punctuated::new();
    for t in toks {
        if let TokenTree::Ident(i) = t {
            segments.push(PathSegment { ident: i.clone() });
        }
    }
    Path {
        __text: __join(toks),
        segments,
    }
}

fn __generic_param(toks: &[TokenTree]) -> GenericParam {
    let mut c = __Cur::new(toks.to_vec());
    let _ = c.attrs();
    if c.punct('\'') {
        c.pos += 1;
        let ident = c.ident().unwrap_or(Ident::new("_", Span));
        let bounds = if c.punct(':') {
            c.pos += 1;
            __join(&c.rest())
        } else {
            String::new()
        };
        return GenericParam::Lifetime(LifetimeParam {
            lifetime: Lifetime { ident },
            __bounds: bounds,
        });
    }
    if c.word("const") {
        c.pos += 1;
        let ident = c.ident().unwrap_or(Ident::new("_", Span));
        if c.punct(':') {
            c.pos += 1;
        }
        let rest = c.rest();
        let eq = rest.iter().position(|t| __is_p(t, '='));
        let (ty, default) = match eq {
            Some(k) => (__join(&rest[..k]), __join(&rest[k + 1..])),
            None => (__join(&rest), String::new()),
        };
        return GenericParam::Const(ConstParam {
            ident,
            ty: Type { __text: ty },
            __default: default,
        });
    }
    let ident = c.ident().unwrap_or(Ident::new("_", Span));
    let rest = c.rest();
    let (bounds, default) = {
        let eq = rest.iter().position(|t| __is_p(t, '='));
        let (b, d) = match eq {
            Some(k) => (rest[..k].to_vec(), __join(&rest[k + 1..])),
            None => (rest.clone(), String::new()),
        };
        let b = if !b.is_empty() && __is_p(&b[0], ':') {
            __join(&b[1..])
        } else {
            __join(&b)
        };
        (b, d)
    };
    GenericParam::Type(TypeParam {
        ident,
        __bounds: bounds,
        __default: default,
    })
}

/// `a: T, b: U` inside braces.
fn __named_fields(stream: TokenStream) -> SynResult<FieldsNamed> {
    let toks: Vec<TokenTree> = stream.into_iter().collect();
    let mut named = Punctuated::new();
    for part in __split_commas(&toks) {
        let mut c = __Cur::new(part);
        let attrs = c.attrs();
        let vis = c.vis();
        let ident = c.ident()?;
        if !c.punct(':') {
            return Err(Error {
                __msg: format!("expected `:`, {}", c.found()),
            });
        }
        c.pos += 1;
        let ty = Type {
            __text: __join(&c.rest()),
        };
        named.push(Field {
            attrs,
            vis,
            ident: Some(ident),
            ty,
        });
    }
    Ok(FieldsNamed { named })
}

/// `T, U` inside parentheses.
fn __unnamed_fields(stream: TokenStream) -> FieldsUnnamed {
    let toks: Vec<TokenTree> = stream.into_iter().collect();
    let mut unnamed = Punctuated::new();
    for part in __split_commas(&toks) {
        let mut c = __Cur::new(part);
        let attrs = c.attrs();
        let vis = c.vis();
        let ty = Type {
            __text: __join(&c.rest()),
        };
        unnamed.push(Field {
            attrs,
            vis,
            ident: None,
            ty,
        });
    }
    FieldsUnnamed { unnamed }
}

fn __variants(stream: TokenStream) -> SynResult<Punctuated<Variant, TkComma>> {
    let toks: Vec<TokenTree> = stream.into_iter().collect();
    let mut out = Punctuated::new();
    for part in __split_commas(&toks) {
        let mut c = __Cur::new(part);
        let attrs = c.attrs();
        let ident = c.ident()?;
        let fields = match c.peek() {
            Some(TokenTree::Group(g)) if g.delimiter() == Delimiter::Brace => {
                c.pos += 1;
                Fields::Named(__named_fields(g.stream())?)
            }
            Some(TokenTree::Group(g)) if g.delimiter() == Delimiter::Parenthesis => {
                c.pos += 1;
                Fields::Unnamed(__unnamed_fields(g.stream()))
            }
            _ => Fields::Unit,
        };
        let discriminant = if c.punct('=') {
            c.pos += 1;
            Some((
                TkEq,
                Expr {
                    __text: __join(&c.rest()),
                },
            ))
        } else {
            None
        };
        out.push(Variant {
            attrs,
            ident,
            fields,
            discriminant,
        });
    }
    Ok(out)
}

/// The fields after a struct's name and generics, and its `where` clause.
fn __struct_body(c: &mut __Cur, generics: &mut Generics) -> SynResult<Fields> {
    generics.where_clause = c.where_clause();
    match c.peek() {
        Some(TokenTree::Group(g)) if g.delimiter() == Delimiter::Brace => {
            c.pos += 1;
            Ok(Fields::Named(__named_fields(g.stream())?))
        }
        Some(TokenTree::Group(g)) if g.delimiter() == Delimiter::Parenthesis => {
            c.pos += 1;
            let f = __unnamed_fields(g.stream());
            if generics.where_clause.is_none() {
                generics.where_clause = c.where_clause();
            }
            Ok(Fields::Unnamed(f))
        }
        Some(t) if __is_p(&t, ';') => Ok(Fields::Unit),
        _ => Err(Error {
            __msg: format!("expected `{{`, `(` or `;`, {}", c.found()),
        }),
    }
}

impl Parse for DeriveInput {
    fn parse(input: ParseStream) -> SynResult<DeriveInput> {
        let mut c = __Cur::new(input.__rest());
        let attrs = c.attrs();
        let vis = c.vis();
        let kind = c.ident()?.__tt();
        let ident = c.ident()?;
        let mut generics = c.generics();
        let data = match kind.as_str() {
            "struct" => Data::Struct(DataStruct {
                fields: __struct_body(&mut c, &mut generics)?,
            }),
            "enum" => {
                generics.where_clause = c.where_clause();
                match c.peek() {
                    Some(TokenTree::Group(g)) if g.delimiter() == Delimiter::Brace => {
                        Data::Enum(DataEnum {
                            variants: __variants(g.stream())?,
                        })
                    }
                    _ => {
                        return Err(Error {
                            __msg: format!("expected `{{`, {}", c.found()),
                        });
                    }
                }
            }
            "union" => {
                generics.where_clause = c.where_clause();
                match c.peek() {
                    Some(TokenTree::Group(g)) => Data::Union(DataUnion {
                        fields: __named_fields(g.stream())?,
                    }),
                    _ => {
                        return Err(Error {
                            __msg: format!("expected `{{`, {}", c.found()),
                        });
                    }
                }
            }
            _ => {
                return Err(Error {
                    __msg: "expected `struct`, `enum` or `union`".to_string(),
                });
            }
        };
        Ok(DeriveInput {
            attrs,
            vis,
            ident,
            generics,
            data,
        })
    }
}

impl Parse for ItemStruct {
    fn parse(input: ParseStream) -> SynResult<ItemStruct> {
        let mut c = __Cur::new(input.__rest());
        let attrs = c.attrs();
        let vis = c.vis();
        if !c.word("struct") {
            return Err(Error {
                __msg: format!("expected `struct`, {}", c.found()),
            });
        }
        c.pos += 1;
        let ident = c.ident()?;
        let mut generics = c.generics();
        let fields = __struct_body(&mut c, &mut generics)?;
        Ok(ItemStruct {
            attrs,
            vis,
            ident,
            generics,
            fields,
        })
    }
}

impl Parse for ItemEnum {
    fn parse(input: ParseStream) -> SynResult<ItemEnum> {
        let mut c = __Cur::new(input.__rest());
        let attrs = c.attrs();
        let vis = c.vis();
        if !c.word("enum") {
            return Err(Error {
                __msg: format!("expected `enum`, {}", c.found()),
            });
        }
        c.pos += 1;
        let ident = c.ident()?;
        let mut generics = c.generics();
        generics.where_clause = c.where_clause();
        let variants = match c.peek() {
            Some(TokenTree::Group(g)) if g.delimiter() == Delimiter::Brace => __variants(g.stream())?,
            _ => {
                return Err(Error {
                    __msg: format!("expected `{{`, {}", c.found()),
                });
            }
        };
        Ok(ItemEnum {
            attrs,
            vis,
            ident,
            generics,
            variants,
        })
    }
}

fn __fn_arg(toks: Vec<TokenTree>) -> FnArg {
    let mut c = __Cur::new(toks);
    let attrs = c.attrs();
    let rest = c.rest();
    // The pattern ends at the first lone `:` (not part of `::`).
    let mut colon = None;
    let mut k = 0;
    while k < rest.len() {
        if __is_p(&rest[k], ':') {
            let joint_next = k + 1 < rest.len() && __is_p(&rest[k + 1], ':');
            let joint_prev = k > 0 && __is_p(&rest[k - 1], ':');
            if !joint_next && !joint_prev {
                colon = Some(k);
                break;
            }
        }
        k += 1;
    }
    let is_self = rest.iter().take(colon.unwrap_or(rest.len())).any(|t| __is_i(t, "self"));
    if is_self {
        return FnArg::Receiver(Receiver {
            __text: __join(&rest),
        });
    }
    match colon {
        Some(k) => FnArg::Typed(PatType {
            attrs,
            pat: Box::new(Pat {
                __text: __join(&rest[..k]),
            }),
            ty: Box::new(Type {
                __text: __join(&rest[k + 1..]),
            }),
        }),
        None => FnArg::Typed(PatType {
            attrs,
            pat: Box::new(Pat {
                __text: __join(&rest),
            }),
            ty: Box::new(Type {
                __text: "_".to_string(),
            }),
        }),
    }
}

impl Parse for ItemFn {
    fn parse(input: ParseStream) -> SynResult<ItemFn> {
        let mut c = __Cur::new(input.__rest());
        let attrs = c.attrs();
        let vis = c.vis();
        let mut constness = None;
        let mut asyncness = None;
        let mut unsafety = None;
        loop {
            if c.word("const") {
                constness = Some(TkConst);
            } else if c.word("async") {
                asyncness = Some(TkAsync);
            } else if c.word("unsafe") {
                unsafety = Some(TkUnsafe);
            } else if c.word("extern") {
                c.pos += 1;
                if matches!(c.peek(), Some(TokenTree::Literal(_))) {
                    c.pos += 1;
                }
                continue;
            } else {
                break;
            }
            c.pos += 1;
        }
        if !c.word("fn") {
            return Err(Error {
                __msg: format!("expected `fn`, {}", c.found()),
            });
        }
        c.pos += 1;
        let ident = c.ident()?;
        let mut generics = c.generics();
        let mut inputs = Punctuated::new();
        match c.peek() {
            Some(TokenTree::Group(g)) if g.delimiter() == Delimiter::Parenthesis => {
                c.pos += 1;
                let toks: Vec<TokenTree> = g.stream().into_iter().collect();
                for part in __split_commas(&toks) {
                    inputs.push(__fn_arg(part));
                }
            }
            _ => {
                return Err(Error {
                    __msg: format!("expected parentheses, {}", c.found()),
                });
            }
        }
        let output = if c.punct('-') && c.peek_at(1).is_some_and(|t| __is_p(&t, '>')) {
            c.pos += 2;
            let start = c.pos;
            while !c.at_end() && !c.word("where") {
                if let Some(TokenTree::Group(g)) = c.peek()
                    && g.delimiter() == Delimiter::Brace
                {
                    break;
                }
                c.pos += 1;
            }
            ReturnType::Type(
                TkRArrow,
                Box::new(Type {
                    __text: __join(&c.toks[start..c.pos]),
                }),
            )
        } else {
            ReturnType::Default
        };
        generics.where_clause = c.where_clause();
        let block = match c.peek() {
            Some(TokenTree::Group(g)) if g.delimiter() == Delimiter::Brace => Block { __text: g.__tt() },
            _ => {
                return Err(Error {
                    __msg: format!("expected curly braces, {}", c.found()),
                });
            }
        };
        Ok(ItemFn {
            attrs,
            vis,
            sig: Signature {
                constness,
                asyncness,
                unsafety,
                ident,
                generics,
                inputs,
                output,
            },
            block: Box::new(block),
        })
    }
}

impl Parse for Ident {
    fn parse(input: ParseStream) -> SynResult<Ident> {
        match input.__peek_tree(0) {
            Some(TokenTree::Ident(i)) if !__is_keyword(&i.__tt()) => {
                input.__advance(1);
                Ok(i)
            }
            Some(TokenTree::Ident(i)) => Err(Error {
                __msg: format!("expected identifier, found keyword `{}`", i.__tt()),
            }),
            _ => Err(Error {
                __msg: format!("expected identifier, {}", input.__found_text()),
            }),
        }
    }
}

/// The next literal, if it's of `kind`.
fn __literal(input: ParseStream, kind: &str, what: &str) -> SynResult<String> {
    if __lit_kind(&input.__peek_tree(0)) == kind {
        let t = input.__bump().unwrap();
        return Ok(t.__tt());
    }
    let found = match input.__peek_tree(0) {
        Some(t) => format!("found `{}`", t.__tt()),
        None => "found the end of the input".to_string(),
    };
    Err(Error {
        __msg: format!("expected {what}, {found}"),
    })
}

impl Parse for LitStr {
    fn parse(input: ParseStream) -> SynResult<LitStr> {
        Ok(LitStr {
            __text: __literal(input, "str", "string literal")?,
        })
    }
}

impl Parse for LitInt {
    fn parse(input: ParseStream) -> SynResult<LitInt> {
        Ok(LitInt {
            __text: __literal(input, "int", "integer literal")?,
        })
    }
}

/// `1.5`, `2e3f32`
#[derive(Clone, Debug)]
pub struct LitFloat {
    __text: String,
}

impl LitFloat {
    pub fn base10_digits(&self) -> String {
        self.__text
            .trim_end_matches("f32")
            .trim_end_matches("f64")
            .replace('_', "")
    }

    pub fn span(&self) -> Span {
        Span
    }

    pub fn __tt(&self) -> String {
        self.__text.clone()
    }
}

impl Parse for LitFloat {
    fn parse(input: ParseStream) -> SynResult<LitFloat> {
        Ok(LitFloat {
            __text: __literal(input, "float", "floating point literal")?,
        })
    }
}

/// `true` / `false`
#[derive(Clone, Debug)]
pub struct LitBool {
    pub value: bool,
}

impl LitBool {
    pub fn value(&self) -> bool {
        self.value
    }

    pub fn span(&self) -> Span {
        Span
    }

    pub fn __tt(&self) -> String {
        self.value.to_string()
    }
}

impl Parse for LitBool {
    fn parse(input: ParseStream) -> SynResult<LitBool> {
        let t = __literal(input, "bool", "boolean literal")?;
        Ok(LitBool { value: t == "true" })
    }
}

/// Any literal.
#[derive(Clone, Debug)]
pub enum Lit {
    Str(LitStr),
    Int(LitInt),
    Float(LitFloat),
    Bool(LitBool),
    Verbatim(Literal),
}

impl Lit {
    pub fn __tt(&self) -> String {
        match self {
            Lit::Str(l) => l.__tt(),
            Lit::Int(l) => l.__tt(),
            Lit::Float(l) => l.__tt(),
            Lit::Bool(l) => l.__tt(),
            Lit::Verbatim(l) => l.__tt(),
        }
    }
}

impl Parse for Lit {
    fn parse(input: ParseStream) -> SynResult<Lit> {
        match __lit_kind(&input.__peek_tree(0)) {
            "str" => Ok(Lit::Str(LitStr::parse(input)?)),
            "int" => Ok(Lit::Int(LitInt::parse(input)?)),
            "float" => Ok(Lit::Float(LitFloat::parse(input)?)),
            "bool" => Ok(Lit::Bool(LitBool::parse(input)?)),
            "" => Err(Error {
                __msg: format!("expected literal, {}", input.__found_text()),
            }),
            _ => match input.__bump() {
                Some(TokenTree::Literal(l)) => Ok(Lit::Verbatim(l)),
                _ => Err(Error {
                    __msg: "expected literal".to_string(),
                }),
            },
        }
    }
}

impl Parse for Type {
    fn parse(input: ParseStream) -> SynResult<Type> {
        let toks = input.__until_separator(true);
        if toks.is_empty() {
            return Err(Error {
                __msg: format!("expected type, {}", input.__found_text()),
            });
        }
        Ok(Type {
            __text: __join(&toks),
        })
    }
}

impl Parse for Expr {
    fn parse(input: ParseStream) -> SynResult<Expr> {
        let toks = input.__until_separator(false);
        if toks.is_empty() {
            return Err(Error {
                __msg: format!("expected an expression, {}", input.__found_text()),
            });
        }
        Ok(Expr {
            __text: __join(&toks),
        })
    }
}

impl Parse for Path {
    fn parse(input: ParseStream) -> SynResult<Path> {
        let mut toks = Vec::new();
        while let Some(t) = input.__peek_tree(0) {
            let take = matches!(t, TokenTree::Ident(_))
                || (__is_p(&t, ':')
                    && (matches!(input.__peek_tree(1), Some(TokenTree::Punct(p)) if p.as_char() == ':')
                        || toks.last().is_some_and(|l: &TokenTree| __is_p(l, ':'))));
            if !take {
                break;
            }
            toks.push(t);
            input.__advance(1);
        }
        if toks.is_empty() {
            return Err(Error {
                __msg: format!("expected identifier, {}", input.__found_text()),
            });
        }
        Ok(__path_of(&toks))
    }
}

impl Parse for Block {
    fn parse(input: ParseStream) -> SynResult<Block> {
        match input.__peek_tree(0) {
            Some(TokenTree::Group(g)) if g.delimiter() == Delimiter::Brace => {
                input.__advance(1);
                Ok(Block { __text: g.__tt() })
            }
            _ => Err(Error {
                __msg: format!("expected curly braces, {}", input.__found_text()),
            }),
        }
    }
}

impl Parse for Visibility {
    fn parse(input: ParseStream) -> SynResult<Visibility> {
        let rest = input.__toks[input.__pos.get()..].to_vec();
        let mut c = __Cur::new(rest);
        let vis = c.vis();
        input.__advance(c.pos);
        Ok(vis)
    }
}

impl Parse for Lifetime {
    fn parse(input: ParseStream) -> SynResult<Lifetime> {
        match (input.__peek_tree(0), input.__peek_tree(1)) {
            (Some(q), Some(TokenTree::Ident(i))) if __is_p(&q, '\'') => {
                input.__advance(2);
                Ok(Lifetime { ident: i })
            }
            _ => Err(Error {
                __msg: format!("expected lifetime, {}", input.__found_text()),
            }),
        }
    }
}

impl Parse for TokenStream {
    fn parse(input: ParseStream) -> SynResult<TokenStream> {
        Ok(TokenStream {
            __text: __join(&input.__rest()),
        })
    }
}

impl Parse for TokenTree {
    fn parse(input: ParseStream) -> SynResult<TokenTree> {
        match input.__bump() {
            Some(t) => Ok(t),
            None => Err(Error {
                __msg: "unexpected end of input, expected token tree".to_string(),
            }),
        }
    }
}
