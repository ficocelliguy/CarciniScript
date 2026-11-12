//! Parser for `format!`-style format strings.

#[derive(Debug, PartialEq, Clone)]
pub enum ArgRef {
    Next,
    Index(usize),
    Name(String),
}

#[derive(Debug, PartialEq, Clone, Default)]
pub struct Spec {
    pub fill: Option<char>,
    /// '<', '>' or '^'
    pub align: Option<char>,
    pub plus: bool,
    pub zero: bool,
    pub width: Option<usize>,
    pub precision: Option<usize>,
    pub debug: bool,
}

#[derive(Debug, PartialEq, Clone)]
pub enum Piece {
    Lit(String),
    Arg { arg: ArgRef, spec: Spec },
}

pub fn parse(s: &str) -> Result<Vec<Piece>, String> {
    let mut pieces = Vec::new();
    let mut lit = String::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '{' if chars.peek() == Some(&'{') => {
                chars.next();
                lit.push('{');
            }
            '}' if chars.peek() == Some(&'}') => {
                chars.next();
                lit.push('}');
            }
            '}' => return Err("invalid format string: unmatched `}` found".into()),
            '{' => {
                let mut inner = String::new();
                loop {
                    match chars.next() {
                        Some('}') => break,
                        Some(c) => inner.push(c),
                        None => {
                            return Err(
                                "invalid format string: expected `}` but string was terminated"
                                    .into(),
                            );
                        }
                    }
                }
                if !lit.is_empty() {
                    pieces.push(Piece::Lit(std::mem::take(&mut lit)));
                }
                pieces.push(parse_placeholder(&inner)?);
            }
            c => lit.push(c),
        }
    }
    if !lit.is_empty() {
        pieces.push(Piece::Lit(lit));
    }
    Ok(pieces)
}

fn parse_placeholder(inner: &str) -> Result<Piece, String> {
    let (arg, spec) = match inner.split_once(':') {
        Some((a, s)) => (a.trim(), Some(s)),
        None => (inner.trim(), None),
    };
    let arg = if arg.is_empty() {
        ArgRef::Next
    } else if let Ok(n) = arg.parse::<usize>() {
        ArgRef::Index(n)
    } else if arg.chars().all(|c| c.is_alphanumeric() || c == '_') {
        ArgRef::Name(arg.to_string())
    } else {
        return Err(format!(
            "invalid format string: invalid argument name `{arg}`"
        ));
    };
    let spec = match spec {
        Some(s) => parse_spec(s)?,
        None => Spec::default(),
    };
    Ok(Piece::Arg { arg, spec })
}

fn parse_spec(s: &str) -> Result<Spec, String> {
    let mut spec = Spec::default();
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    let is_align = |c: char| matches!(c, '<' | '>' | '^');
    if chars.len() >= 2 && is_align(chars[1]) {
        spec.fill = Some(chars[0]);
        spec.align = Some(chars[1]);
        i = 2;
    } else if !chars.is_empty() && is_align(chars[0]) {
        spec.align = Some(chars[0]);
        i = 1;
    }
    if chars.get(i) == Some(&'+') {
        spec.plus = true;
        i += 1;
    }
    if chars.get(i) == Some(&'#') {
        return Err("`#` (alternate) formatting is not supported yet".into());
    }
    if chars.get(i) == Some(&'0') {
        spec.zero = true;
        i += 1;
    }
    let read_num = |i: &mut usize| -> Option<usize> {
        let start = *i;
        while *i < chars.len() && chars[*i].is_ascii_digit() {
            *i += 1;
        }
        if *i > start {
            chars[start..*i].iter().collect::<String>().parse().ok()
        } else {
            None
        }
    };
    spec.width = read_num(&mut i);
    if chars.get(i) == Some(&'.') {
        i += 1;
        spec.precision = read_num(&mut i);
        if spec.precision.is_none() {
            return Err("only literal precision (e.g. `{:.2}`) is supported yet".into());
        }
    }
    match &chars[i..] {
        [] => {}
        ['?'] => spec.debug = true,
        rest => {
            let rest: String = rest.iter().collect();
            return Err(format!("unsupported format spec `{rest}`"));
        }
    }
    Ok(spec)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_mixed() {
        let p = parse("a {} {{b}} {x:>8.2} {0:?}").unwrap();
        assert_eq!(p[0], Piece::Lit("a ".into()));
        assert_eq!(
            p[1],
            Piece::Arg {
                arg: ArgRef::Next,
                spec: Spec::default()
            }
        );
        assert_eq!(p[2], Piece::Lit(" {b} ".into()));
        assert_eq!(
            p[3],
            Piece::Arg {
                arg: ArgRef::Name("x".into()),
                spec: Spec {
                    align: Some('>'),
                    width: Some(8),
                    precision: Some(2),
                    ..Spec::default()
                }
            }
        );
        assert_eq!(
            p[5],
            Piece::Arg {
                arg: ArgRef::Index(0),
                spec: Spec {
                    debug: true,
                    ..Spec::default()
                }
            }
        );
    }

    #[test]
    fn parses_fill_and_zero() {
        let p = parse("{:*^9}{:05}").unwrap();
        assert_eq!(
            p[0],
            Piece::Arg {
                arg: ArgRef::Next,
                spec: Spec {
                    fill: Some('*'),
                    align: Some('^'),
                    width: Some(9),
                    ..Spec::default()
                }
            }
        );
        assert_eq!(
            p[1],
            Piece::Arg {
                arg: ArgRef::Next,
                spec: Spec {
                    zero: true,
                    width: Some(5),
                    ..Spec::default()
                }
            }
        );
    }
}
