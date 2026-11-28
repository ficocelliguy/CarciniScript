// A `char` handed to an `impl Into<String>` / `S: Into<String>` parameter.
fn f(s: impl Into<String>) -> String {
    s.into()
}
fn g<S: Into<String>>(s: S) -> usize {
    s.into().len()
}
fn h<S>(s: S) -> String
where
    S: Into<String>,
{
    let mut t: String = s.into();
    t.push('!');
    t
}
struct W {
    n: String,
}
impl W {
    fn new(n: impl Into<String>) -> W {
        W { n: n.into() }
    }
    fn set<S: Into<String>>(&mut self, n: S) {
        self.n = n.into();
    }
}
fn main() {
    println!("{}", f('c'));
    println!("{}", f("ab"));
    println!("{}", f(String::from("xy")));
    println!("{}", g('x'));
    println!("{}", g('é'));
    println!("{}", h('q'));
    let c = 'z';
    let mut w = W::new(c);
    println!("{}", w.n);
    w.set('k');
    println!("{}", w.n);
    w.set(('m'));
    println!("{}", W::new('\n').n.len());
    let v: Vec<String> = vec!['a', 'b'].into_iter().map(Into::into).collect();
    println!("{:?} {}", v, w.n);
}
