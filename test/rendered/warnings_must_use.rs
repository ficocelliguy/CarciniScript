#[must_use]
fn calc() -> u32 { 3 }
#[must_use = "the sum is the point"]
fn sum(a: u32, b: u32) -> u32 { a + b }
#[must_use]
struct Token(u32);
fn tok() -> Token { Token(1) }
struct S;
impl S {
    #[must_use]
    fn m(&self) -> u32 { 1 }
    #[deprecated(note = "use m")]
    fn old(&self) -> u32 { 2 }
    #[must_use]
    fn new() -> S { S }
}
#[must_use]
enum E { A }
fn e() -> E { E::A }
#[deprecated]
fn gone() {}
#[deprecated(since = "1.2", note = "use calc")]
fn gone2() -> u32 { 0 }
#[deprecated = "plain"]
fn gone3() {}
#[deprecated]
struct Old;
trait Tr { #[must_use] fn t(&self) -> u8; }
impl Tr for S { fn t(&self) -> u8 { 0 } }
fn main() {
    calc();
    sum(1, 2);
    tok();
    let s = S;
    s.m();
    S::new();
    e();
    s.old();
    gone();
    gone2();
    gone3();
    let _o = Old;
    s.t();
    let f = calc; f();
    Some(tok());
    (calc(), 1);
    if true { calc(); }
}
