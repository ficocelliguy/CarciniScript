// `Option<()>` (`then_some(())`, `?`, a `HashMap<K, ()>`), `Option<Option<T>>` read through
// `flatten()`, `Into<f64>` bounds and `T: DeserializeOwned` (GAPS.md C6 and C10).
use serde::de::DeserializeOwned;
use serde::Deserialize;
use std::collections::HashMap;

fn check(ok: bool) -> Option<()> {
    ok.then_some(())
}

fn mean<I>(xs: I) -> f64
where
    I: IntoIterator,
    I::Item: Into<f64>,
{
    let mut n = 0.0;
    let mut t = 0.0;
    for x in xs {
        t += x.into();
        n += 1.0;
    }
    t / n
}

fn total<T: Into<f64> + Copy>(xs: &[T]) -> f64 {
    xs.iter().map(|x| (*x).into()).sum()
}

#[derive(Deserialize, Debug)]
struct Msg {
    n: i32,
    tags: Vec<String>,
}

fn decode<T: DeserializeOwned>(s: &str) -> Option<T> {
    serde_json::from_str(s).ok()
}

fn decode_all<T: DeserializeOwned>(lines: &[&str]) -> Vec<T> {
    lines.iter().filter_map(|l| decode::<T>(l)).collect()
}

fn find(v: &[i32], x: i32) -> Option<()> {
    v.contains(&x).then_some(())?;
    Some(())
}

fn ready(m: &HashMap<&str, ()>, k: &str) -> Option<()> {
    m.get(k).copied()
}

fn main() {
    println!("{:?} {:?}", check(true), check(false));
    let vals = vec![Some(1), None, Some(3)];
    println!("{:?}", vals.first().copied().flatten());
    println!("{:?}", vals.get(1).cloned().flatten());
    println!("{:?}", vals.get(9).cloned().flatten());
    let hot = true;
    println!("{:?}", hot.then(|| vals[2]).flatten());
    println!("{:?} {:?}", find(&[1, 2], 2), find(&[1], 5));
    let o: Option<()> = Some(());
    if let Some(()) = o {
        println!("unit some {}", o.is_some());
    }
    let mut seen: HashMap<&str, ()> = HashMap::new();
    println!("{:?}", seen.insert("a", ()));
    println!("{:?}", seen.insert("a", ()));
    println!("{:?} {:?}", ready(&seen, "a"), ready(&seen, "b"));
    let units: Vec<Option<()>> = vec![Some(()), None];
    println!("{:?} {}", units, units.iter().filter(|u| u.is_some()).count());
    println!("{:?}", check(true).map(|()| 5));
    println!("{}", mean(vec![1u8, 2, 3]));
    println!("{}", mean(vec![1.5f32, 2.5]));
    println!("{}", total(&[1i32, 2, 3]));
    let m: Option<Msg> = decode(r#"{"n": 5, "tags": ["x"]}"#);
    println!("{:?}", m);
    let bad: Option<Msg> = decode("x");
    println!("{:?}", bad);
    let nums: Vec<u8> = decode_all(&["1", "300", "7"]);
    println!("{:?}", nums);
    let words: Vec<String> = decode_all(&["\"a\"", "2"]);
    println!("{:?}", words);
    let alt: Option<f64> = Some(2.5);
    let r: Result<Option<f64>, &str> = alt.map(Some).ok_or("none");
    println!("{:?}", r);
    let xs: Vec<f64> = Some(vec![1.0, 2.0]).into_iter().flatten().collect();
    println!("{:?} {}", xs, Some(3).iter().count());
}
