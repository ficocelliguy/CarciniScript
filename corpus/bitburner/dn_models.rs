//! Darknet server models and the pure (no-NS) password math behind them.
//!
//! Everything here is deterministic and unit-tested; `darknet.rs` drives the network side.

use std::collections::{BTreeMap, HashSet};
use std::fmt;
use std::iter::Peekable;
use std::str::{Chars, FromStr};

use itertools::Itertools;
use regex::Regex;
use std::sync::LazyLock;

/// Every darknet server model, keyed by its `modelId`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Model {
    EchoVuln,
    SortedEchoVuln,
    NoPassword,
    Captcha,
    DefaultPassword,
    BufferOverflow,
    MastermindHint,
    TimingAttack,
    LargestPrimeFactor,
    RomanNumeral,
    DogNames,
    GuessNumber,
    CommonPasswordDictionary,
    EuCountryDictionary,
    Yesnt,
    BinaryEncodedFeedback,
    SpiceLevel,
    ConvertToBase10,
    ParsedExpression,
    DivisibilityTest,
    TripleModulo,
    GlobalMaxima,
    PacketSniffer,
    EncryptedPassword,
    Labyrinth,
}

impl Model {
    const IDS: &'static [(&'static str, Model)] = &[
        ("DeskMemo_3.1", Model::EchoVuln),
        ("PHP 5.4", Model::SortedEchoVuln),
        ("ZeroLogon", Model::NoPassword),
        ("CloudBlare(tm)", Model::Captcha),
        ("FreshInstall_1.0", Model::DefaultPassword),
        ("Pr0verFl0", Model::BufferOverflow),
        ("DeepGreen", Model::MastermindHint),
        ("2G_cellular", Model::TimingAttack),
        ("PrimeTime 2", Model::LargestPrimeFactor),
        ("BellaCuore", Model::RomanNumeral),
        ("Laika4", Model::DogNames),
        ("AccountsManager_4.2", Model::GuessNumber),
        ("TopPass", Model::CommonPasswordDictionary),
        ("EuroZone Free", Model::EuCountryDictionary),
        ("NIL", Model::Yesnt),
        ("110100100", Model::BinaryEncodedFeedback),
        ("RateMyPix.Auth", Model::SpiceLevel),
        ("OctantVoxel", Model::ConvertToBase10),
        ("MathML", Model::ParsedExpression),
        ("Factori-Os", Model::DivisibilityTest),
        ("BigMo%od", Model::TripleModulo),
        ("KingOfTheHill", Model::GlobalMaxima),
        ("OpenWebAccessPoint", Model::PacketSniffer),
        ("OrdoXenos", Model::EncryptedPassword),
        ("(The Labyrinth)", Model::Labyrinth),
    ];

    pub fn id(self) -> &'static str {
        Self::IDS
            .iter()
            .find_map(|&(id, m)| (m == self).then_some(id))
            .unwrap_or("?")
    }

    /// Models whose password can be worked out from the details alone, with no feedback.
    pub fn is_static(self) -> bool {
        use Model::*;
        matches!(
            self,
            EchoVuln
                | NoPassword
                | Captcha
                | LargestPrimeFactor
                | RomanNumeral
                | BinaryEncodedFeedback
                | ConvertToBase10
                | ParsedExpression
                | EncryptedPassword
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownModel(pub String);

impl fmt::Display for UnknownModel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "unknown darknet model {:?}", self.0)
    }
}

impl std::error::Error for UnknownModel {}

impl FromStr for Model {
    type Err = UnknownModel;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::IDS
            .iter()
            .find(|(id, _)| *id == s)
            .map(|&(_, m)| m)
            .ok_or_else(|| UnknownModel(s.to_owned()))
    }
}

impl fmt::Display for Model {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?} ({})", self, self.id())
    }
}

pub const DEFAULT_PASSWORDS: [&str; 4] = ["admin", "password", "0000", "12345"];
pub const DOG_NAMES: [&str; 4] = ["fido", "spot", "rover", "max"];
pub const EU_COUNTRIES: &[&str] = &[
    "Austria", "Belgium", "Bulgaria", "Croatia", "Republic of Cyprus", "Czech Republic", "Denmark",
    "Estonia", "Finland", "France", "Germany", "Greece", "Hungary", "Ireland", "Italy", "Latvia",
    "Lithuania", "Luxembourg", "Malta", "Netherlands", "Poland", "Portugal", "Romania", "Slovakia",
    "Slovenia", "Spain", "Sweden",
];

// ---------------------------------------------------------------------------------------
// Static decoders

const ECHO_NOISE: [&str; 6] = [
    "The password is",
    "The PIN is",
    "Remember to use",
    "It's set to",
    "The key is",
    "The secret is",
];

pub fn echo_password(hint: &str) -> String {
    let trimmed = ECHO_NOISE
        .iter()
        .fold(hint, |acc, noise| acc.strip_prefix(noise).unwrap_or(acc));
    trimmed.trim().trim_end_matches('.').to_owned()
}

pub fn captcha(data: &str) -> String {
    data.chars().filter(char::is_ascii_digit).collect()
}

/// `MCMXCIV` → 1994. Accepts the subtractive forms; `None` on anything else.
pub fn parse_roman(s: &str) -> Option<u32> {
    fn value(c: char) -> Option<u32> {
        Some(match c.to_ascii_uppercase() {
            'I' => 1,
            'V' => 5,
            'X' => 10,
            'L' => 50,
            'C' => 100,
            'D' => 500,
            'M' => 1000,
            _ => return None,
        })
    }
    if s.eq_ignore_ascii_case("nulla") {
        return Some(0);
    }
    let values: Vec<u32> = s.chars().map(value).collect::<Option<_>>()?;
    let total = values
        .iter()
        .zip(values.iter().skip(1).map(Some).chain(std::iter::once(None)))
        .map(|(&v, next)| match next {
            Some(&n) if n > v => -(v as i64),
            _ => v as i64,
        })
        .sum::<i64>();
    u32::try_from(total).ok()
}

/// Parses `digits` (possibly with a fractional part) in `base` (2..=36).
pub fn parse_in_base(digits: &str, base: u32) -> Option<f64> {
    if !(2..=36).contains(&base) {
        return None;
    }
    let (int, frac) = digits.trim().split_once('.').unwrap_or((digits.trim(), ""));
    let int_value = int
        .chars()
        .try_fold(0f64, |acc, c| Some(acc * base as f64 + c.to_digit(base)? as f64))?;
    let frac_value = frac
        .chars()
        .rev()
        .try_fold(0f64, |acc, c| Some((acc + c.to_digit(base)? as f64) / base as f64))?;
    Some(int_value + frac_value)
}

/// `"8,1757"` style hint data for the base-conversion model.
pub fn convert_to_base10(data: &str) -> Option<String> {
    let (base, number) = data.split_once(',')?;
    let value = parse_in_base(number, base.trim().parse().ok()?)?;
    Some(format_number(value))
}

/// Formats like JS's `Number.prototype.toString` for the values the game produces.
pub fn format_number(v: f64) -> String {
    if v.fract() == 0.0 && v.abs() < 1e15 {
        format!("{}", v as i64)
    } else {
        let s = format!("{v:.10}");
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

pub fn largest_prime_factor(mut n: u64) -> Option<u64> {
    if n < 2 {
        return None;
    }
    let mut largest = 1;
    let mut f = 2;
    while f * f <= n {
        while n.is_multiple_of(f) {
            largest = f;
            n /= f;
        }
        f += if f == 2 { 1 } else { 2 };
    }
    Some(if n > 1 { n } else { largest })
}

/// Space-separated 8-bit groups back into text.
pub fn decode_binary(data: &str) -> Option<String> {
    data.split_whitespace()
        .map(|byte| u8::from_str_radix(byte, 2).ok().map(char::from))
        .collect()
}

/// `"<masked>;<mask> <mask> ..."`: each char was xored with its mask.
pub fn decode_xor(data: &str) -> Option<String> {
    let (masked, masks) = data.rsplit_once(';')?;
    let masks: Vec<u32> = masks
        .split(' ')
        .map(|m| u32::from_str_radix(m, 2))
        .collect::<Result<_, _>>()
        .ok()?;
    if masks.len() != masked.chars().count() {
        return None;
    }
    masked
        .chars()
        .zip(masks)
        .map(|(c, m)| char::from_u32(c as u32 ^ m))
        .collect()
}

// ---------------------------------------------------------------------------------------
// Arithmetic expressions (MathML model)

#[derive(Debug, Clone, Copy, PartialEq)]
enum Token {
    Num(f64),
    Op(char),
    Open,
    Close,
}

#[derive(Debug, PartialEq)]
pub enum ExprError {
    Unexpected(char),
    UnexpectedEnd,
    Trailing(String),
}

impl fmt::Display for ExprError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ExprError::Unexpected(c) => write!(f, "unexpected {c:?}"),
            ExprError::UnexpectedEnd => f.write_str("unexpected end of expression"),
            ExprError::Trailing(rest) => write!(f, "trailing input {rest:?}"),
        }
    }
}

struct Lexer<'a> {
    chars: Peekable<Chars<'a>>,
}

impl<'a> Lexer<'a> {
    fn new(src: &'a str) -> Self {
        Lexer { chars: src.chars().peekable() }
    }
}

impl Iterator for Lexer<'_> {
    type Item = Result<Token, ExprError>;

    fn next(&mut self) -> Option<Self::Item> {
        while self.chars.next_if(|c| c.is_whitespace()).is_some() {}
        let c = *self.chars.peek()?;
        let tok = match c {
            '0'..='9' | '.' => {
                let mut s = String::new();
                while let Some(d) = self.chars.next_if(|c| c.is_ascii_digit() || *c == '.') {
                    s.push(d);
                }
                return Some(s.parse().map(Token::Num).map_err(|_| ExprError::Unexpected(c)));
            }
            '+' | '➕' => Token::Op('+'),
            '-' | '➖' => Token::Op('-'),
            '*' | 'ҳ' => Token::Op('*'),
            '/' | '÷' => Token::Op('/'),
            '(' => Token::Open,
            ')' => Token::Close,
            other => return Some(Err(ExprError::Unexpected(other))),
        };
        self.chars.next();
        Some(Ok(tok))
    }
}

/// Recursive-descent evaluator: expr := term (('+'|'-') term)*, term := unary (('*'|'/') unary)*.
struct Parser<I: Iterator<Item = Token>> {
    tokens: Peekable<I>,
}

impl<I: Iterator<Item = Token>> Parser<I> {
    fn expr(&mut self) -> Result<f64, ExprError> {
        let mut acc = self.term()?;
        while let Some(&Token::Op(op @ ('+' | '-'))) = self.tokens.peek() {
            self.tokens.next();
            let rhs = self.term()?;
            acc = if op == '+' { acc + rhs } else { acc - rhs };
        }
        Ok(acc)
    }

    fn term(&mut self) -> Result<f64, ExprError> {
        let mut acc = self.unary()?;
        loop {
            match self.tokens.peek() {
                Some(Token::Op('*')) => {
                    self.tokens.next();
                    acc *= self.unary()?;
                }
                Some(Token::Op('/')) => {
                    self.tokens.next();
                    acc /= self.unary()?;
                }
                _ => break Ok(acc),
            }
        }
    }

    fn unary(&mut self) -> Result<f64, ExprError> {
        match self.tokens.next().ok_or(ExprError::UnexpectedEnd)? {
            Token::Op('-') => Ok(-self.unary()?),
            Token::Num(n) => Ok(n),
            Token::Open => {
                let v = self.expr()?;
                match self.tokens.next() {
                    Some(Token::Close) => Ok(v),
                    Some(_) => Err(ExprError::Unexpected(')')),
                    None => Err(ExprError::UnexpectedEnd),
                }
            }
            Token::Op(c) => Err(ExprError::Unexpected(c)),
            Token::Close => Err(ExprError::Unexpected(')')),
        }
    }
}

/// Strips injected code (`ns.exit(),`, trailing `;...`) then evaluates.
pub fn evaluate(expression: &str) -> Result<f64, ExprError> {
    static INJECTION: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"ns\.[a-zA-Z]+\([^)]*\),?|;.*$").unwrap());
    let cleaned = INJECTION.replace_all(expression, "");
    let tokens: Vec<Token> = Lexer::new(&cleaned).collect::<Result<_, _>>()?;
    let mut parser = Parser { tokens: tokens.into_iter().peekable() };
    let value = parser.expr()?;
    match parser.tokens.next() {
        None => Ok(value),
        Some(t) => Err(ExprError::Trailing(format!("{t:?}"))),
    }
}

// ---------------------------------------------------------------------------------------
// Candidate generators for the feedback models

/// Distinct orderings of `chars`; huge inputs yield nothing rather than hang.
pub fn permutations(chars: &str, limit: usize) -> impl Iterator<Item = String> + '_ {
    let n = chars.chars().count();
    let too_many = (1..=n).product::<usize>() > limit;
    chars
        .chars()
        .permutations(if too_many { 0 } else { n })
        .filter(move |_| !too_many)
        .map(|p| p.into_iter().collect::<String>())
        .unique()
}

/// Counts per digit from "how many of this digit are in the password" probes.
pub fn digits_from_counts(counts: &BTreeMap<u8, usize>) -> String {
    counts
        .iter()
        .flat_map(|(&d, &n)| std::iter::repeat_n(char::from(b'0' + d), n))
        .collect()
}

/// Mastermind response `"exact,misplaced"`.
pub fn parse_mastermind(data: &str) -> Option<(usize, usize)> {
    let (a, b) = data.split_once(',')?;
    Some((a.trim().parse().ok()?, b.trim().parse().ok()?))
}

/// Number of 🌶️ in a SpiceLevel response like `"🌶️🌶️/5"` (`"0/5"` means none).
pub fn spice_count(data: &str) -> usize {
    let (peppers, _) = data.split_once('/').unwrap_or((data, ""));
    peppers.matches('🌶').count()
}

/// Packet captures contain ` host:password ` somewhere in the noise.
pub fn sniff_password<'a>(capture: &'a str, host: &str) -> Option<&'a str> {
    let needle = format!(" {host}:");
    let start = capture.find(&needle)? + needle.len();
    let rest = &capture[start..];
    let end = rest.find(' ').unwrap_or(rest.len());
    Some(&rest[..end]).filter(|s| !s.is_empty())
}

/// The alphabet for a server's `passwordFormat`.
pub fn alphabet(format: &str) -> Vec<char> {
    let digits = '0'..='9';
    let lower = 'a'..='z';
    let upper = 'A'..='Z';
    match format {
        "numeric" => digits.collect(),
        "alphabetic" => lower.chain(upper).collect(),
        "alphanumeric" => digits.chain(lower).chain(upper).collect(),
        _ => (' '..='~').collect(),
    }
}

/// Keeps a set of still-possible passwords and narrows it by feedback.
#[derive(Debug, Default)]
pub struct Candidates {
    alive: HashSet<String>,
}

impl Candidates {
    pub fn new(all: impl IntoIterator<Item = String>) -> Self {
        Candidates { alive: all.into_iter().collect() }
    }

    pub fn retain_consistent(&mut self, guess: &str, feedback: impl Fn(&str, &str) -> bool) {
        self.alive.retain(|c| feedback(c, guess));
    }

    pub fn len(&self) -> usize {
        self.alive.len()
    }

    pub fn is_empty(&self) -> bool {
        self.alive.is_empty()
    }

    /// Deterministic pick so reruns behave the same.
    pub fn best(&self) -> Option<&String> {
        self.alive.iter().min()
    }
}

/// Exact and misplaced counts as the DeepGreen model computes them.
pub fn mastermind_score(secret: &str, guess: &str) -> (usize, usize) {
    let exact = secret.chars().zip(guess.chars()).filter(|(a, b)| a == b).count();
    let count = |s: &str| s.chars().counts();
    let (s, g) = (count(secret), count(guess));
    let common: usize = s.iter().map(|(c, n)| (*n).min(*g.get(c).unwrap_or(&0))).sum();
    (exact, common - exact)
}

// ---------------------------------------------------------------------------------------
// Dictionary: TopPass picks uniformly from the game's common-password list.

/// The game's common-password list, one per line.
pub fn common_passwords() -> impl Iterator<Item = &'static str> {
    include_str!("common_passwords.txt")
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
}

// ---------------------------------------------------------------------------------------
// BigMo%od: a wrong guess `n` answers `(password % n) % (((n - 1) % 32) + 1)`.

/// What the server reduces by the second time for a probe `n`.
pub const fn second_modulus(n: u64) -> u64 {
    (n - 1) % 32 + 1
}

/// Probes up to 32, where the second reduction is a no-op so the answer is `password % n`.
/// They're pairwise coprime and their product is lcm(1..=32).
pub const CRT_PROBES: [u64; 11] = [32, 27, 25, 7, 11, 13, 17, 19, 23, 29, 31];

/// `(g, x, y)` with `a·x + b·y = g = gcd(a, b)`.
fn extended_gcd(a: i128, b: i128) -> (i128, i128, i128) {
    if b == 0 {
        (a, 1, 0)
    } else {
        let (g, x, y) = extended_gcd(b, a % b);
        (g, y, x - (a / b) * y)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Congruence {
    pub residue: u128,
    pub modulus: u128,
}

impl Congruence {
    pub const ANY: Congruence = Congruence { residue: 0, modulus: 1 };

    /// Combines `self` with `x ≡ residue (mod modulus)`; `None` if they contradict.
    pub fn and(self, residue: u64, modulus: u64) -> Option<Congruence> {
        let (m1, m2) = (self.modulus as i128, i128::from(modulus));
        let (r1, r2) = (self.residue as i128, i128::from(residue));
        let (g, p, _) = extended_gcd(m1, m2);
        if (r2 - r1) % g != 0 {
            return None;
        }
        let lcm = m1 / g * m2;
        let k = ((r2 - r1) / g * p).rem_euclid(m2 / g);
        Some(Congruence {
            residue: (r1 + m1 * k).rem_euclid(lcm) as u128,
            modulus: lcm as u128,
        })
    }

    /// Every number with exactly `digits` digits (no leading zeros) that fits.
    pub fn with_digits(self, digits: u32) -> impl Iterator<Item = u128> {
        let lo = if digits <= 1 { 0 } else { 10u128.pow(digits - 1) };
        let hi = 10u128.pow(digits);
        let first = lo + (self.residue + self.modulus - lo % self.modulus) % self.modulus;
        (first..hi).step_by(self.modulus as usize)
    }
}

// ---------------------------------------------------------------------------------------
// KingOfTheHill: altitude is a sum of Gaussian hills, the tallest (10 000 m) centred on
// the password.

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Hill {
    pub height: f64,
    pub width: f64,
}

impl Hill {
    pub const PEAK: f64 = 10_000.0;

    /// The password's hill for a numeric password of `digits` digits.
    pub fn for_digits(digits: u32) -> Hill {
        Hill { height: Self::PEAK, width: 10f64.powi(digits.saturating_sub(2) as i32) + 1.0 }
    }

    pub fn altitude(&self, centre: f64, x: f64) -> f64 {
        self.height * (-((x - centre) / self.width).powi(2)).exp()
    }

    /// Where the centre would be if `altitude` was measured at `x` on this hill alone:
    /// one guess on each side.
    pub fn centres(&self, x: f64, altitude: f64) -> Option<[f64; 2]> {
        if !(altitude > 0.0 && altitude <= self.height) {
            return None;
        }
        let d = self.width * (self.height / altitude).ln().sqrt();
        Some([x - d, x + d])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dictionary() {
        let words: Vec<_> = common_passwords().collect();
        assert_eq!(words.len(), 93);
        assert_eq!(words.first(), Some(&"123456"));
    }

    #[test]
    fn triple_modulo() {
        let password: u128 = 913_406_118_250_771;
        let found = CRT_PROBES
            .iter()
            .try_fold(Congruence::ANY, |c, &n| c.and((password % u128::from(n)) as u64, n))
            .unwrap();
        assert_eq!(found.modulus, 144_403_552_893_600);
        assert!(found.with_digits(15).any(|x| x == password));
        assert_eq!(second_modulus(32), 32);
        assert_eq!(second_modulus(33), 1);
        assert_eq!(Congruence::ANY.and(1, 4).and_then(|c| c.and(2, 6)), None);
    }

    #[test]
    fn hill() {
        let hill = Hill::for_digits(6);
        let at = hill.altitude(456_789.0, 450_000.0);
        let [left, right] = hill.centres(450_000.0, at).unwrap();
        assert!((right - 456_789.0).abs() < 1e-3, "{left} {right}");
    }

    #[test]
    fn roman() {
        assert_eq!(parse_roman("MCMXCIV"), Some(1994));
        assert_eq!(parse_roman("nulla"), Some(0));
        assert_eq!(parse_roman("ABC"), None);
    }

    #[test]
    fn base() {
        assert_eq!(convert_to_base10("16,FF").as_deref(), Some("255"));
        assert_eq!(parse_in_base("0.1", 2), Some(0.5));
    }

    #[test]
    fn expressions() {
        assert_eq!(evaluate("4 + 5 * ( 6 + 7 ) / 2"), Ok(36.5));
        assert_eq!(evaluate("(ns.exit(),2) ҳ 3"), Ok(6.0));
        assert!(matches!(evaluate("2 +"), Err(ExprError::UnexpectedEnd)));
    }

    #[test]
    fn decoders() {
        assert_eq!(decode_binary("01101000 01101001").as_deref(), Some("hi"));
        assert_eq!(decode_xor("ih;00000001 00000001").as_deref(), Some("hi"));
        assert_eq!(largest_prime_factor(13195), Some(29));
        assert_eq!(sniff_password("xx  dn-1:hunter2  yy", "dn-1"), Some("hunter2"));
        assert_eq!(mastermind_score("1123", "1312"), (1, 3));
    }

    #[test]
    fn model_ids_round_trip() {
        for &(id, m) in Model::IDS {
            assert_eq!(id.parse::<Model>(), Ok(m));
        }
    }
}
