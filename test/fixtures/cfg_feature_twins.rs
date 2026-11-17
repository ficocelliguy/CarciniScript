trait Name {
    fn name(&self) -> &'static str;
}
struct A;
#[cfg(feature = "fancy")]
impl Name for A {
    fn name(&self) -> &'static str {
        "fancy"
    }
}
#[cfg(not(feature = "fancy"))]
impl Name for A {
    fn name(&self) -> &'static str {
        "plain"
    }
}
#[cfg(feature = "fancy")]
fn mode() -> u8 {
    1
}
#[cfg(not(feature = "fancy"))]
fn mode() -> u8 {
    0
}
fn main() {
    println!("{} {}", A.name(), mode());
}
