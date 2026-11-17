// One generic trait implemented more than once for a type (`impl IntoS<u8> for Vec<u8>` and
// `impl IntoS<u16> for Vec<u8>`, on your own struct too): a call picks the impl from the
// expected type or the arguments.
trait IntoS<T> {
    fn into_s(&self) -> T;
}

impl IntoS<u8> for Vec<u8> {
    fn into_s(&self) -> u8 {
        self.iter().fold(0u8, |a, b| a.wrapping_add(*b))
    }
}

impl IntoS<u16> for Vec<u8> {
    fn into_s(&self) -> u16 {
        self.iter().map(|b| *b as u16).sum()
    }
}

trait Scale<T> {
    fn scale(&self, by: T) -> String;
}

struct Money(u64);

impl Scale<u32> for Money {
    fn scale(&self, by: u32) -> String {
        format!("x{} = {}", by, self.0 * by as u64)
    }
}

impl Scale<&str> for Money {
    fn scale(&self, by: &str) -> String {
        format!("{}{}", self.0, by)
    }
}

impl Scale<f64> for Money {
    fn scale(&self, by: f64) -> String {
        format!("{:.1}", self.0 as f64 * by)
    }
}

fn main() {
    let v: Vec<u8> = vec![200, 100, 7];
    let small: u8 = v.into_s();
    let wide: u16 = v.into_s();
    println!("{small} {wide}");
    let m = Money(40);
    println!("{} | {} | {}", m.scale(3u32), m.scale("k"), m.scale(0.5));
}
