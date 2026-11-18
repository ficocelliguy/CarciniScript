// `TryFrom`, `Extend` and `FromIterator` implemented more than once for a type: a call picks the
// impl by its argument's type (`TryFrom<Vec<u8>>` or `TryFrom<char>`, `FromIterator<Tok>` or
// `FromIterator<Stream>`), and between impls whose types differ only by references
// (`TryFrom<&str>` / `TryFrom<String>` / `TryFrom<&String>`, `Extend<&T>` / `Extend<T>`) by how
// the argument is written: `"x"` / `s.as_str()`, `s.to_string()`, `&s`, `v.iter()` / `vec![..]`.
// An impl that hands on to another never picks itself.
use std::convert::TryFrom;

#[derive(Debug, Default)]
struct Bag {
    items: Vec<i32>,
}

impl Extend<i32> for Bag {
    fn extend<I: IntoIterator<Item = i32>>(&mut self, iter: I) {
        for x in iter {
            self.items.push(x);
        }
    }
}

impl<'a> Extend<&'a i32> for Bag {
    fn extend<I: IntoIterator<Item = &'a i32>>(&mut self, iter: I) {
        for x in iter {
            self.items.push(*x * 10);
        }
    }
}

#[derive(Debug)]
struct Name(String);

impl TryFrom<&str> for Name {
    type Error = String;
    fn try_from(s: &str) -> Result<Self, Self::Error> {
        if s.is_empty() {
            Err("empty".into())
        } else {
            Ok(Name(format!("s:{s}")))
        }
    }
}

impl TryFrom<String> for Name {
    type Error = String;
    fn try_from(s: String) -> Result<Self, Self::Error> {
        Ok(Name(format!("S:{s}")))
    }
}

impl TryFrom<Vec<u8>> for Name {
    type Error = String;
    fn try_from(v: Vec<u8>) -> Result<Self, Self::Error> {
        String::from_utf8(v)
            .map(|s| Name(format!("v:{s}")))
            .map_err(|_| "utf8".to_string())
    }
}

impl TryFrom<&String> for Name {
    type Error = String;
    fn try_from(s: &String) -> Result<Self, Self::Error> {
        Ok(Name(format!("&S:{s}")))
    }
}

#[derive(Debug, Clone, PartialEq)]
struct Ident(String);

// The owned impl comes first and hands on to the `&str` one.
impl TryFrom<String> for Ident {
    type Error = String;
    fn try_from(s: String) -> Result<Self, Self::Error> {
        Self::try_from(s.as_str())
    }
}

impl TryFrom<&String> for Ident {
    type Error = String;
    fn try_from(s: &String) -> Result<Self, Self::Error> {
        Ident::try_from(s.as_str())
    }
}

impl TryFrom<&str> for Ident {
    type Error = String;
    fn try_from(s: &str) -> Result<Self, Self::Error> {
        if !s.is_empty() && s.chars().all(|c| c.is_alphanumeric() || c == '_') {
            Ok(Ident(s.to_string()))
        } else {
            Err(format!("bad ident {:?}", s))
        }
    }
}

impl TryFrom<char> for Ident {
    type Error = String;
    fn try_from(c: char) -> Result<Self, Self::Error> {
        Ident::try_from(c.to_string())
    }
}

#[derive(Debug, Default)]
struct Totals {
    sum: i64,
    n: usize,
}

impl Extend<i64> for Totals {
    fn extend<I: IntoIterator<Item = i64>>(&mut self, iter: I) {
        for x in iter {
            self.sum += x;
            self.n += 1;
        }
    }
}

impl<'a> Extend<&'a i64> for Totals {
    fn extend<I: IntoIterator<Item = &'a i64>>(&mut self, iter: I) {
        self.extend(iter.into_iter().copied())
    }
}

impl FromIterator<i64> for Totals {
    fn from_iter<I: IntoIterator<Item = i64>>(iter: I) -> Self {
        let mut t = Totals::default();
        t.extend(iter);
        t
    }
}

impl<'a> FromIterator<&'a i64> for Totals {
    fn from_iter<I: IntoIterator<Item = &'a i64>>(iter: I) -> Self {
        iter.into_iter().copied().collect()
    }
}

#[derive(Debug, Clone)]
struct Tok(String);

#[derive(Debug, Default)]
struct Stream {
    toks: Vec<String>,
}

impl FromIterator<Tok> for Stream {
    fn from_iter<I: IntoIterator<Item = Tok>>(iter: I) -> Self {
        Stream {
            toks: iter.into_iter().map(|t| t.0).collect(),
        }
    }
}

impl FromIterator<Stream> for Stream {
    fn from_iter<I: IntoIterator<Item = Stream>>(iter: I) -> Self {
        let mut toks = Vec::new();
        for s in iter {
            toks.extend(s.toks);
        }
        toks.push("|".into());
        Stream { toks }
    }
}

impl Extend<Tok> for Stream {
    fn extend<I: IntoIterator<Item = Tok>>(&mut self, iter: I) {
        for t in iter {
            self.toks.push(t.0);
        }
    }
}

impl Extend<Stream> for Stream {
    fn extend<I: IntoIterator<Item = Stream>>(&mut self, iter: I) {
        for s in iter {
            self.toks.push(format!("[{}]", s.toks.join(",")));
        }
    }
}

fn main() {
    let mut b = Bag::default();
    b.extend(vec![1, 2]);
    b.extend(&[3, 4]);
    b.extend([5].iter());
    println!("{:?}", b);

    println!("{:?}", Name::try_from("abc"));
    println!("{:?}", Name::try_from(""));
    println!("{:?}", Name::try_from("own".to_string()));
    let s = String::from("ref");
    println!("{:?}", Name::try_from(&s));
    println!("{:?}", Name::try_from(vec![104u8, 105]));
    println!("{:?}", Name::try_from(vec![255u8]));
    let n: Result<Name, _> = "conv".try_into();
    println!("{:?}", n);
    let n2: Result<Name, String> = String::from("conv2").try_into();
    println!("{:?}", n2);

    let owned = String::from("abc_1");
    println!("{:?}", Ident::try_from(owned.clone()));
    println!("{:?}", Ident::try_from(&owned));
    println!("{:?}", Ident::try_from("x y"));
    println!("{:?}", Ident::try_from('q'));
    println!("{:?}", Ident::try_from('-'));
    let r: Result<Ident, String> = "zz".try_into();
    println!("{:?}", r);
    let r2: Result<Ident, _> = owned.try_into();
    println!("{:?}", r2);
    let words = vec!["a".to_string(), "b c".to_string()];
    for w in &words {
        println!("{:?}", Ident::try_from(w));
    }

    let v = vec![1i64, 2, 3];
    let mut t = Totals::default();
    t.extend(&v);
    t.extend(v.iter());
    t.extend(vec![10i64]);
    println!("{:?}", t);
    let x: Totals = v.iter().collect();
    let y: Totals = v.into_iter().collect();
    println!("{:?} {:?}", x, y);

    let a: Stream = vec![Tok("a".into()), Tok("b".into())].into_iter().collect();
    println!("{:?}", a);
    let c: Stream = vec![Tok("c".into())].into_iter().collect();
    let joined: Stream = vec![a, c].into_iter().collect();
    println!("{:?}", joined);
    let mut d = Stream::default();
    d.extend(vec![Tok("x".into())]);
    d.extend(vec![Stream {
        toks: vec!["y".into(), "z".into()],
    }]);
    println!("{:?}", d);
}
