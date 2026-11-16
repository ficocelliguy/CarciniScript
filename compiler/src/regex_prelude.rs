// (regex prelude)
// The bundled `regex` crate: `Regex` (`is_match`, `find`, `find_iter`, `captures`,
// `captures_iter`, `replace` / `replace_all` / `replacen` with `$1` / `${name}` or a closure,
// `split`, `splitn`), `Match` and `Captures`, on JS regular expressions (Rust's syntax
// translated; offsets in bytes). `replace` gives a `String` where regex gives a `Cow<str>`.

/// `regex::Regex`.
#[derive(Clone)]
pub struct Regex {
    __pat: String,
    __names: Vec<String>,
}

/// `regex::Error`: why a pattern doesn't compile.
#[derive(Clone, PartialEq)]
pub struct RegexError {
    __msg: String,
}

impl std::fmt::Display for RegexError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.__msg)
    }
}

impl std::fmt::Debug for RegexError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.__msg)
    }
}

impl std::error::Error for RegexError {}

/// `regex::Match`: where a match is in the haystack.
#[derive(Clone, PartialEq)]
pub struct Match<'h> {
    __hay: &'h str,
    __s: usize,
    __e: usize,
}

impl<'h> Match<'h> {
    pub fn start(&self) -> usize {
        self.__s
    }
    pub fn end(&self) -> usize {
        self.__e
    }
    pub fn len(&self) -> usize {
        self.__e - self.__s
    }
    pub fn is_empty(&self) -> bool {
        self.__s == self.__e
    }
    pub fn as_str(&self) -> &'h str {
        &self.__hay[self.__s..self.__e]
    }
}

impl<'h> std::fmt::Debug for Match<'h> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(
            f,
            "Match {{ start: {}, end: {}, string: {:?} }}",
            self.__s,
            self.__e,
            self.as_str()
        )
    }
}

/// `regex::Captures`: the groups of one match.
#[derive(Clone)]
pub struct Captures<'h> {
    __hay: &'h str,
    __at: Vec<i32>,
    __names: Vec<String>,
}

impl<'h> Captures<'h> {
    /// Group `i` (0 is the whole match), if it took part.
    pub fn get(&self, i: usize) -> Option<Match<'h>> {
        if 2 * i + 1 >= self.__at.len() || self.__at[2 * i] < 0 {
            return None;
        }
        Some(Match {
            __hay: self.__hay,
            __s: self.__at[2 * i] as usize,
            __e: self.__at[2 * i + 1] as usize,
        })
    }
    /// The group named `name`, if it took part.
    pub fn name(&self, name: &str) -> Option<Match<'h>> {
        for i in 0..self.__names.len() {
            if self.__names[i] == name {
                return self.get(i);
            }
        }
        None
    }
    /// How many groups the pattern has, with the whole match.
    pub fn len(&self) -> usize {
        self.__at.len() / 2
    }
    pub fn iter(&self) -> std::vec::IntoIter<Option<Match<'h>>> {
        let mut out = Vec::new();
        for i in 0..self.len() {
            out.push(self.get(i));
        }
        out.into_iter()
    }
    /// `replacement` with `$1`, `${1}`, `$name`, `${name}` filled in (`$$` is a `$`), added to
    /// `dst`.
    pub fn expand(&self, replacement: &str, dst: &mut String) {
        let chars: Vec<char> = replacement.chars().collect();
        let mut i = 0;
        while i < chars.len() {
            let c = chars[i];
            if c != '$' {
                dst.push(c);
                i += 1;
                continue;
            }
            if i + 1 < chars.len() && chars[i + 1] == '$' {
                dst.push('$');
                i += 2;
                continue;
            }
            let mut name = String::new();
            let mut j = i + 1;
            if j < chars.len() && chars[j] == '{' {
                let mut k = j + 1;
                while k < chars.len() && chars[k] != '}' {
                    k += 1;
                }
                if k >= chars.len() {
                    dst.push('$');
                    i += 1;
                    continue;
                }
                for x in &chars[j + 1..k] {
                    name.push(*x);
                }
                j = k + 1;
            } else {
                while j < chars.len() && (chars[j].is_alphanumeric() || chars[j] == '_') {
                    name.push(chars[j]);
                    j += 1;
                }
            }
            if name.is_empty() {
                dst.push('$');
                i += 1;
                continue;
            }
            let found = match name.parse::<usize>() {
                Ok(n) => self.get(n),
                Err(_) => self.name(&name),
            };
            if let Some(m) = found {
                dst.push_str(m.as_str());
            }
            i = j;
        }
    }
}

impl<'h> std::ops::Index<usize> for Captures<'h> {
    type Output = str;
    fn index(&self, i: usize) -> &str {
        match self.get(i) {
            Some(m) => m.as_str(),
            None => panic!("no group at index '{}'", i),
        }
    }
}

impl<'h> std::ops::Index<&str> for Captures<'h> {
    type Output = str;
    fn index(&self, name: &str) -> &str {
        match self.name(name) {
            Some(m) => m.as_str(),
            None => panic!("no group named '{}'", name),
        }
    }
}

impl<'h> std::fmt::Debug for Captures<'h> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let mut parts = Vec::new();
        for i in 0..self.len() {
            let key = if self.__names[i].is_empty() {
                format!("{}", i)
            } else {
                format!("{}/{:?}", i, self.__names[i])
            };
            let value = match self.get(i) {
                Some(m) => format!("{}..{}/{:?}", m.start(), m.end(), m.as_str()),
                None => "None".to_string(),
            };
            parts.push(format!("{}: {}", key, value));
        }
        write!(f, "Captures({{{}}})", parts.join(", "))
    }
}

/// `regex::NoExpand`: a replacement taken as it is (no `$1`).
pub struct NoExpand<'s>(pub &'s str);

/// What `replace` puts in: a string with `$1` / `${name}`, or `NoExpand`.
pub trait Replacer {
    fn __replace(&self, caps: &Captures, dst: &mut String);
}

impl Replacer for str {
    fn __replace(&self, caps: &Captures, dst: &mut String) {
        caps.expand(self, dst);
    }
}

impl<'s> Replacer for NoExpand<'s> {
    fn __replace(&self, _caps: &Captures, dst: &mut String) {
        dst.push_str(self.0);
    }
}

impl Regex {
    pub fn new(re: &str) -> Result<Regex, RegexError> {
        let err = js::__regex_check(re);
        if !err.is_empty() {
            return Err(RegexError { __msg: err });
        }
        Ok(Regex {
            __pat: re.to_string(),
            __names: js::__regex_names(re),
        })
    }
    pub fn as_str(&self) -> &str {
        &self.__pat
    }
    /// How many groups, with the whole match.
    pub fn captures_len(&self) -> usize {
        self.__names.len()
    }
    pub fn capture_names(&self) -> std::vec::IntoIter<Option<&str>> {
        let mut out = Vec::new();
        for n in &self.__names {
            if n.is_empty() {
                out.push(None);
            } else {
                out.push(Some(n.as_str()));
            }
        }
        out.into_iter()
    }
    fn __at<'h>(&self, hay: &'h str, start: usize) -> Option<Captures<'h>> {
        let at = js::__regex_find(&self.__pat, hay, start);
        if at.is_empty() {
            return None;
        }
        Some(Captures {
            __hay: hay,
            __at: at,
            __names: self.__names.clone(),
        })
    }
    /// Every match, left to right, not overlapping (an empty one never right after another).
    fn __all<'h>(&self, hay: &'h str, limit: usize) -> Vec<Captures<'h>> {
        let mut out = Vec::new();
        let mut pos = 0;
        let mut last_end: Option<usize> = None;
        while pos <= hay.len() && out.len() < limit {
            let Some(caps) = self.__at(hay, pos) else { break };
            let s = caps.__at[0] as usize;
            let e = caps.__at[1] as usize;
            if s == e {
                if last_end == Some(e) {
                    pos = e + js::__regex_char_len(hay, e);
                    continue;
                }
                pos = e + js::__regex_char_len(hay, e);
            } else {
                pos = e;
            }
            last_end = Some(e);
            out.push(caps);
        }
        out
    }
    pub fn is_match(&self, hay: &str) -> bool {
        self.__at(hay, 0).is_some()
    }
    pub fn is_match_at(&self, hay: &str, start: usize) -> bool {
        self.__at(hay, start).is_some()
    }
    pub fn find<'h>(&self, hay: &'h str) -> Option<Match<'h>> {
        match self.__at(hay, 0) {
            Some(c) => c.get(0),
            None => None,
        }
    }
    pub fn find_at<'h>(&self, hay: &'h str, start: usize) -> Option<Match<'h>> {
        match self.__at(hay, start) {
            Some(c) => c.get(0),
            None => None,
        }
    }
    /// Where the leftmost match ends.
    pub fn shortest_match(&self, hay: &str) -> Option<usize> {
        match self.find(hay) {
            Some(m) => Some(m.end()),
            None => None,
        }
    }
    pub fn find_iter<'h>(&self, hay: &'h str) -> std::vec::IntoIter<Match<'h>> {
        let mut out = Vec::new();
        for c in self.__all(hay, usize::MAX) {
            if let Some(m) = c.get(0) {
                out.push(m);
            }
        }
        out.into_iter()
    }
    pub fn captures<'h>(&self, hay: &'h str) -> Option<Captures<'h>> {
        self.__at(hay, 0)
    }
    pub fn captures_at<'h>(&self, hay: &'h str, start: usize) -> Option<Captures<'h>> {
        self.__at(hay, start)
    }
    pub fn captures_iter<'h>(&self, hay: &'h str) -> std::vec::IntoIter<Captures<'h>> {
        self.__all(hay, usize::MAX).into_iter()
    }
    /// The pieces between matches.
    pub fn split<'h>(&self, hay: &'h str) -> std::vec::IntoIter<&'h str> {
        let mut out = Vec::new();
        let mut last = 0;
        for c in self.__all(hay, usize::MAX) {
            let s = c.__at[0] as usize;
            let e = c.__at[1] as usize;
            out.push(&hay[last..s]);
            last = e;
        }
        out.push(&hay[last..]);
        out.into_iter()
    }
    /// At most `limit` pieces: the last is the rest.
    pub fn splitn<'h>(&self, hay: &'h str, limit: usize) -> std::vec::IntoIter<&'h str> {
        let mut out = Vec::new();
        if limit == 0 {
            return out.into_iter();
        }
        let mut last = 0;
        for c in self.__all(hay, limit - 1) {
            let s = c.__at[0] as usize;
            let e = c.__at[1] as usize;
            out.push(&hay[last..s]);
            last = e;
        }
        out.push(&hay[last..]);
        out.into_iter()
    }
    fn __replace_with<R: Replacer + ?Sized>(&self, hay: &str, limit: usize, rep: &R) -> String {
        let mut out = String::new();
        let mut last = 0;
        for c in self.__all(hay, limit) {
            let s = c.__at[0] as usize;
            let e = c.__at[1] as usize;
            out.push_str(&hay[last..s]);
            rep.__replace(&c, &mut out);
            last = e;
        }
        out.push_str(&hay[last..]);
        out
    }
    pub fn replace<R: Replacer>(&self, hay: &str, rep: R) -> String {
        self.__replace_with(hay, 1, &rep)
    }
    pub fn replace_all<R: Replacer>(&self, hay: &str, rep: R) -> String {
        self.__replace_with(hay, usize::MAX, &rep)
    }
    pub fn replacen<R: Replacer>(&self, hay: &str, limit: usize, rep: R) -> String {
        let n = if limit == 0 { usize::MAX } else { limit };
        self.__replace_with(hay, n, &rep)
    }
    // A closure replacer (`|caps: &Captures| ..`): what the compiler calls for one.
    pub fn __replace_fn<T: std::fmt::Display, F: FnMut(&Captures) -> T>(
        &self,
        hay: &str,
        limit: usize,
        mut f: F,
    ) -> String {
        let mut out = String::new();
        let mut last = 0;
        for c in self.__all(hay, limit) {
            let s = c.__at[0] as usize;
            let e = c.__at[1] as usize;
            out.push_str(&hay[last..s]);
            out.push_str(&format!("{}", f(&c)));
            last = e;
        }
        out.push_str(&hay[last..]);
        out
    }
}

impl std::fmt::Display for Regex {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.__pat)
    }
}

impl std::fmt::Debug for Regex {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "Regex({:?})", self.__pat)
    }
}

impl std::str::FromStr for Regex {
    type Err = RegexError;
    fn from_str(s: &str) -> Result<Regex, RegexError> {
        Regex::new(s)
    }
}

/// `regex::escape`: `text` with the regex syntax characters escaped.
pub fn __regex_escape(text: &str) -> String {
    let mut out = String::new();
    for c in text.chars() {
        if "\\.+*?()|[]{}^$#&-~".contains(c) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}
