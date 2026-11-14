// Debug-build overflow checks: `abs` past the type's range panics (rustc reports it inside std).
fn main() {
    let m = std::hint::black_box(i32::MIN); println!("{}", m.abs());
}
