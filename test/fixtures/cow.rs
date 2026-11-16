// `std::borrow::Cow`: borrowed or owned strings and slices, `into_owned`, matching,
// `to_mut` on a slice, deref to the data, `{}` / `{:?}`, `==`, `From<&str>`; and `into_owned()`
// on what `String::from_utf8_lossy` gives.
use std::borrow::Cow;

fn normalize(s: &str) -> Cow<str> {
    if s.contains(' ') {
        Cow::Owned(s.replace(' ', "_"))
    } else {
        Cow::Borrowed(s)
    }
}

fn evens(v: &[i32]) -> Cow<[i32]> {
    if v.iter().all(|x| x % 2 == 0) {
        Cow::Borrowed(v)
    } else {
        Cow::Owned(v.iter().map(|x| x * 2).collect())
    }
}

struct Label<'a> {
    text: Cow<'a, str>,
}

fn main() {
    let a = normalize("a b c");
    let b = normalize("abc");
    println!("{} {} {:?} {:?}", a, b, a, b);
    println!("{} {} {} {}", a.len(), b.to_uppercase(), matches!(a, Cow::Owned(_)), matches!(b, Cow::Borrowed(_)));
    println!("{}", matches!(b, Cow::Borrowed(_)));
    let owned: String = a.into_owned();
    println!("{}", owned);
    let c: Cow<str> = "from".into();
    println!("{} {}", c, c == Cow::Borrowed("from"));
    let l = Label {
        text: Cow::Borrowed("static"),
    };
    let m = Label {
        text: Cow::Owned(format!("n={}", 3)),
    };
    println!("{} {}", l.text, m.text);
    let v = vec![2, 4, 6];
    let w = vec![1, 2];
    let e1 = evens(&v);
    let e2 = evens(&w);
    println!("{:?} {:?} {} {}", e1, e2, matches!(e1, Cow::Borrowed(_)), e2.len());
    let mut e3 = evens(&v);
    e3.to_mut().push(8);
    println!("{:?} {}", e3, matches!(e3, Cow::Owned(_)));
    let s = String::from_utf8_lossy(&[104, 105]).into_owned();
    println!("{}", s);
    let names: Vec<Cow<str>> = vec!["x y", "z"].into_iter().map(normalize).collect();
    println!("{:?}", names);
}
