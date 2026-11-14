// Debug-build overflow checks: `sum` past the type's range panics (rustc reports it inside std).
fn main() {
    let v: Vec<u8> = vec![200, 100]; let s: u8 = v.iter().sum(); println!("{}", s);
}
