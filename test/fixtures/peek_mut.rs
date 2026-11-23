// `Peekable::peek_mut`: a slot over the buffered item for numbers, strings and chars (matched,
// unwrapped, kept in a `let`), and the item itself for a `Vec`.
fn main() {
    let v = vec![1, 2, 3];
    let mut it = v.into_iter().peekable();
    if let Some(p) = it.peek_mut() {
        *p *= 10;
    }
    println!("{:?}", it.next());
    *it.peek_mut().unwrap() += 7;
    println!("{:?}", it.peek());
    match it.peek_mut() {
        Some(x) => *x -= 1,
        None => {}
    }
    let rest: Vec<i32> = it.collect();
    println!("{:?}", rest);

    let words = vec![String::from("a"), String::from("b")];
    let mut w = words.into_iter().peekable();
    if let Some(s) = w.peek_mut() {
        s.push_str("!");
    }
    let r = w.peek_mut().unwrap();
    r.push('?');
    println!("{}", w.peek_mut().is_some());
    println!("{:?}", w.collect::<Vec<_>>());

    let mut e = Vec::<i32>::new().into_iter().peekable();
    println!("{}", e.peek_mut().is_none());

    let rows = vec![vec![1], vec![2]];
    let mut ri = rows.into_iter().peekable();
    if let Some(r) = ri.peek_mut() {
        r.push(9);
    }
    ri.peek_mut().map(|r| r.push(8));
    println!("{:?}", ri.next());

    let mut c = "abc".chars().peekable();
    while let Some(ch) = c.peek_mut() {
        if *ch == 'b' {
            *ch = 'B';
        }
        print!("{}", c.next().unwrap());
    }
    println!();
}
