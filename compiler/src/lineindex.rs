/// Maps byte offsets in UTF-8 source text to zero-based (line, UTF-16 column) pairs,
/// which is what source maps, Monaco and devtools all expect.
pub struct LineIndex<'a> {
    text: &'a str,
    line_starts: Vec<usize>,
    /// When the compiled text is a macro expansion of `text`: expanded offsets → `text`'s.
    map: Option<std::rc::Rc<crate::mbe::OffsetMap>>,
}

impl<'a> LineIndex<'a> {
    pub fn new(text: &'a str) -> Self {
        let mut line_starts = vec![0];
        for (i, b) in text.bytes().enumerate() {
            if b == b'\n' {
                line_starts.push(i + 1);
            }
        }
        LineIndex {
            text,
            line_starts,
            map: None,
        }
    }

    /// Positions of an expansion of `original`, reported in `original`.
    pub fn mapped(original: &'a str, map: std::rc::Rc<crate::mbe::OffsetMap>) -> Self {
        LineIndex {
            map: Some(map),
            ..LineIndex::new(original)
        }
    }

    pub fn line_col(&self, offset: usize) -> (u32, u32) {
        let offset = match &self.map {
            Some(m) => m.orig(offset, false),
            None => offset,
        };
        let offset = offset.min(self.text.len());
        let line = match self.line_starts.binary_search(&offset) {
            Ok(l) => l,
            Err(l) => l - 1,
        };
        let start = self.line_starts[line];
        let col: usize = self.text[start..offset].chars().map(char::len_utf16).sum();
        (line as u32, col as u32)
    }
}

/// Number of UTF-16 code units in `s`, used to track generated-code columns.
pub fn utf16_len(s: &str) -> u32 {
    s.chars().map(char::len_utf16).sum::<usize>() as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_offsets_across_lines_and_wide_chars() {
        let idx = LineIndex::new("ab\n😀x\ny");
        assert_eq!(idx.line_col(0), (0, 0));
        assert_eq!(idx.line_col(3), (1, 0));
        // 😀 is 4 bytes in UTF-8 and 2 code units in UTF-16.
        assert_eq!(idx.line_col(7), (1, 2));
        assert_eq!(idx.line_col(9), (2, 0));
    }
}

/// `src` with each string continuation before a CR LF (`"a \` CR LF `   b"`, as git checks a
/// file out on Windows) written `\` LF CR, the same length: the escaper takes a `\` before a LF
/// only, and skips the CR with the next line's indent, as rustc skips both.
pub fn crlf_continuations(src: &str) -> std::borrow::Cow<'_, str> {
    use ra_ap_syntax::SyntaxKind;
    if !src.contains("\\\r\n") {
        return src.into();
    }
    let lexed = ra_ap_parser::LexedStr::new(crate::macros::EDITION, src);
    let mut out = src.as_bytes().to_vec();
    for i in 0..lexed.len() {
        let raw = lexed.text(i).trim_start_matches(['b', 'c']).starts_with('r');
        let string = matches!(
            lexed.kind(i),
            SyntaxKind::STRING | SyntaxKind::BYTE_STRING | SyntaxKind::C_STRING
        );
        if !string || raw {
            continue;
        }
        let r = lexed.text_range(i);
        let mut j = r.start;
        while j + 2 < r.end {
            match &src.as_bytes()[j..j + 3] {
                [b'\\', b'\r', b'\n'] => {
                    out[j + 1] = b'\n';
                    out[j + 2] = b'\r';
                    j += 3;
                }
                [b'\\', ..] => j += 2,
                _ => j += 1,
            }
        }
    }
    // (Only ASCII bytes were swapped.)
    String::from_utf8(out).unwrap().into()
}

/// `src` with each typed `self` parameter written as its shorthand, padded with spaces to the
/// same length: `self: &mut Self` is `&mut self      `, `self: &'a Self` is `&'a self      `,
/// `mut self: Self` is `mut self      `. (`self: Box<Self>` and the like stay.)
pub fn typed_self_params(src: &str) -> std::borrow::Cow<'_, str> {
    use ra_ap_syntax::SyntaxKind as K;
    if !src.contains("self:") && !src.contains("self :") {
        return src.into();
    }
    let lexed = ra_ap_parser::LexedStr::new(crate::macros::EDITION, src);
    let toks: Vec<usize> = (0..lexed.len())
        .filter(|&i| !matches!(lexed.kind(i), K::WHITESPACE | K::COMMENT))
        .collect();
    let mut out = src.as_bytes().to_vec();
    let mut changed = false;
    for (n, &i) in toks.iter().enumerate() {
        if lexed.kind(i) != K::SELF_KW || toks.get(n + 1).map(|&j| lexed.kind(j)) != Some(K::COLON) {
            continue;
        }
        // `&` [lifetime] [`mut`] `Self`, then `,` or `)`.
        let mut k = n + 2;
        let mut short = String::new();
        if toks.get(k).map(|&j| lexed.kind(j)) == Some(K::AMP) {
            short.push('&');
            k += 1;
            if let Some(&j) = toks.get(k).filter(|&&j| lexed.kind(j) == K::LIFETIME_IDENT) {
                short.push_str(lexed.text(j));
                short.push(' ');
                k += 1;
            }
            if toks.get(k).map(|&j| lexed.kind(j)) == Some(K::MUT_KW) {
                short.push_str("mut ");
                k += 1;
            }
        }
        let Some(&end) = toks.get(k).filter(|&&j| lexed.kind(j) == K::SELF_TYPE_KW) else {
            continue;
        };
        if !matches!(toks.get(k + 1).map(|&j| lexed.kind(j)), Some(K::COMMA | K::R_PAREN)) {
            continue;
        }
        short.push_str("self");
        let (a, b) = (lexed.text_range(i).start, lexed.text_range(end).end);
        if src[a..b].contains('\n') || short.len() > b - a {
            continue;
        }
        let mut padded = short.into_bytes();
        padded.resize(b - a, b' ');
        out[a..b].copy_from_slice(&padded);
        changed = true;
    }
    if !changed {
        return src.into();
    }
    // (Whole tokens were replaced, with text and spaces.)
    String::from_utf8(out).unwrap().into()
}

/// The same-length rewrites made before anything parses a file (`crlf_continuations`,
/// `typed_self_params`).
pub fn source_fixups(src: &str) -> std::borrow::Cow<'_, str> {
    match typed_self_params(src) {
        std::borrow::Cow::Borrowed(s) => crlf_continuations(s),
        std::borrow::Cow::Owned(s) => crlf_continuations(&s).into_owned().into(),
    }
}

/// A string literal's value, with each CR LF in it read as LF, as rustc reads the file (a raw or
/// multi-line string in a file checked out with CR LF line ends).
pub fn string_value(s: &ra_ap_syntax::ast::String) -> Option<String> {
    use ra_ap_syntax::{AstToken, ast};
    match s.value() {
        Ok(v) if !s.text().contains('\r') => Some(v.into_owned()),
        _ => {
            let fixed = s.text().replace("\r\n", "\n");
            let parse = ast::Expr::parse(&fixed, crate::macros::EDITION);
            let ast::Expr::Literal(l) = parse.tree() else { return None };
            match l.kind() {
                ast::LiteralKind::String(s) => s.value().ok().map(|v| v.into_owned()),
                _ => None,
            }
        }
    }
}

/// Whether a parse error is the lexer's bare-CR one in a file whose every CR starts a CR LF:
/// rustc reads those as LF, and `string_value` does.
pub fn crlf_only_error(message: &str, src: &str) -> bool {
    message == "Character must be escaped: `\r`"
        && src.match_indices('\r').all(|(k, _)| src[k + 1..].starts_with('\n'))
}
