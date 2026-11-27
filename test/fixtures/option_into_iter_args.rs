// An `Option` passed where an `impl IntoIterator` / `I: IntoIterator` is expected: zero or one
// items.
fn ext(v: &mut Vec<u32>, it: impl IntoIterator<Item = u32>) {
    v.extend(it)
}

fn sum<I: IntoIterator<Item = u32>>(it: I) -> u32 {
    it.into_iter().sum()
}

fn cnt<I>(it: I) -> usize
where
    I: IntoIterator,
    I::Item: Copy,
{
    it.into_iter().count()
}

fn names(it: impl IntoIterator<Item = String>) -> String {
    let mut out = String::new();
    for s in it {
        out.push_str(&s);
        out.push(';');
    }
    out
}

fn main() {
    let mut v = vec![1];
    let o: Option<u32> = Some(7);
    ext(&mut v, o);
    ext(&mut v, Some(3));
    ext(&mut v, None);
    let n: Option<u32> = None;
    ext(&mut v, n);
    println!("{:?}", v);
    println!("{} {} {}", sum(o), sum(Some(4)), sum(n));
    println!("{} {} {}", cnt(o), cnt(n), cnt(&o));
    println!("[{}] [{}]", names(Some("a".to_string())), names(None));
    let nested: Option<Option<u32>> = Some(None);
    println!("{}", cnt(nested));
}
