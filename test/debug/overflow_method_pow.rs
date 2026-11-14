// Debug-build overflow checks: `pow` past the type's range panics (rustc reports it inside std).
fn main() {
    let m = std::hint::black_box(3i32); println!("{}", m.pow(30));
}
