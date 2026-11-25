// Moves that patterns and closures make, in code rustc accepts: by-reference matches, `ref`
// bindings, `Copy` parts, reassigning what moved, and closures that only borrow.
struct Node { val: i32, next: Option<Box<Node>> }
#[allow(dead_code)]
enum Cmd { Say(String), Num(i32), Pair { a: String, b: i32 } }
struct P { name: String, n: i32 }
fn get() -> Option<String> { Some("x".to_string()) }
fn take(s: String) -> usize { s.len() }
fn opt_param(o: Option<String>) -> String {
    if let Some(ref s) = o { println!("{s}"); }
    match &o { Some(s) => println!("{s}"), None => {} }
    match o { Some(s) => s, None => String::new() }
}
fn main() {
    let mut cur = Some(Box::new(Node { val: 1, next: Some(Box::new(Node { val: 2, next: None })) }));
    while let Some(node) = cur { println!("{}", node.val); cur = node.next; }
    let o = Some(String::from("a"));
    if let Some(x) = &o { println!("{x}"); }
    if let Some(x) = o.as_ref() { println!("{x}"); }
    if let Some(_) = o { }
    if o.is_some() { println!("{:?}", o); }
    let c = Cmd::Say("hi".into());
    match &c { Cmd::Say(s) => println!("{s}"), _ => {} }
    match c { Cmd::Num(n) => println!("{n}"), Cmd::Say(ref s) => println!("{s}"), Cmd::Pair { b, .. } => println!("{b}") }
    if let Cmd::Say(s) = c { println!("{s}"); }
    let p = P { name: "n".into(), n: 3 };
    let P { n, .. } = p;
    println!("{} {}", n, p.name);
    let q = P { name: "m".into(), n: 4 };
    match q { P { n, .. } if n > 3 => println!("{n}"), _ => {} }
    println!("{}", q.name);
    let g = get();
    let len = match g { Some(s) => take(s), None => 0 };
    println!("{len}");
    let s = String::from("z");
    let f = || println!("{s}");
    f();
    println!("{s}");
    let s2 = String::from("y");
    let f2 = || s2.len();
    println!("{} {}", f2(), s2);
    let v = vec![String::from("a")];
    let h = || v.iter().count();
    println!("{} {:?}", h(), v);
    let r: Result<String, String> = Ok("ok".into());
    match r { Ok(ref a) | Err(ref a) => println!("{a}") }
    println!("{:?}", r);
    let mut st = Some(String::from("w"));
    if let Some(x) = st { println!("{x}"); st = None; }
    println!("{:?}", st);
    let t = Some(String::from("t"));
    let t2 = match t { Some(s) => Some(s), None => None };
    println!("{:?}", t2);
    println!("{}", opt_param(Some("q".into())));
}
