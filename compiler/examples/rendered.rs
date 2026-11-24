//! `cargo run --example rendered -- [--test] path/to/file.rs` prints the diagnostics as rustc
//! would (`rendered`): compare with `rustc --edition 2021 --crate-type bin --emit=metadata`.
fn main() {
    let test = std::env::args().any(|a| a == "--test");
    let path = std::env::args()
        .skip(1)
        .find(|a| a != "--test")
        .expect("usage: rendered [--test] <file.rs>");
    let src = std::fs::read_to_string(&path).expect("read source");
    let name = std::path::Path::new(&path)
        .file_name()
        .unwrap()
        .to_string_lossy()
        .to_string();
    let dir = std::path::Path::new(&path)
        .parent()
        .map(|d| d.to_path_buf())
        .unwrap_or_default();
    let resolve = |p: &str| std::fs::read_to_string(dir.join(p)).ok();
    let mut out = if test {
        carciniscript::compile_tests_with(&src, &name, &resolve)
    } else {
        carciniscript::compile_with(&src, &name, &resolve)
    };
    carciniscript::rustc_diag::finish(&src, &name, &mut out);
    print!("{}", out.rendered);
}
