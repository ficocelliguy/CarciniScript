// Array patterns with lengths from a local, a function, a field (GAPS.md C5), and a user
// type passed to `impl AsRef<str>` / `S: AsRef<str>` parameters (C9).
struct H(String);
impl AsRef<str> for H {
    fn as_ref(&self) -> &str {
        &self.0
    }
}
fn shout(s: impl AsRef<str>) -> String {
    s.as_ref().to_uppercase()
}
fn twice<S: AsRef<str>>(s: S) -> usize {
    s.as_ref().len() * 2
}
fn pair() -> [i32; 2] {
    [3, 4]
}
struct P {
    xy: [f64; 2],
}
fn main() {
    let arr = [1, 2, 3];
    let [a, b, c] = arr;
    let [x, y] = pair();
    let p = P { xy: [1.5, 2.5] };
    let [px, py] = p.xy;
    println!("{} {} {} {} {} {} {}", a, b, c, x, y, px, py);
    println!("{} {} {}", shout(H("hi".into())), shout("yo"), twice(H("abc".into())));
}
