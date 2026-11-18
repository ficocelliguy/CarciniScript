//! `cargo run --example compile -- path/to/file.rs` prints the generated JS, or diagnostics.
fn main() {
    let path = std::env::args().nth(1).expect("usage: compile <file.rs>");
    let src = std::fs::read_to_string(&path).expect("read source");
    // `CARCINI_BINDINGS=bindings/ns.rs`: typed NS bindings, as the editor loads them.
    if let Some(b) = std::env::var_os("CARCINI_BINDINGS") {
        let text = std::fs::read_to_string(b).expect("read bindings");
        carciniscript::set_bindings(&text);
    }
    let name = std::path::Path::new(&path)
        .file_name()
        .unwrap()
        .to_string_lossy()
        .to_string();
    // `mod x;` reads `x.rs` next to the file.
    let dir = std::path::Path::new(&path)
        .parent()
        .map(|d| d.to_path_buf())
        .unwrap_or_default();
    let resolve = |p: &str| std::fs::read_to_string(dir.join(p)).ok();
    let out = carciniscript::compile_with(&src, &name, &resolve);
    for d in &out.diagnostics {
        eprintln!(
            "{}:{}:{}: {}: {}",
            name, d.line, d.column, d.severity, d.message
        );
    }
    match out.code {
        Some(code) => print!("{code}"),
        None => std::process::exit(1),
    }
}
