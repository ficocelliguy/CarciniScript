//! `cargo run --example compile -- path/to/file.rs` prints the generated JS, or diagnostics.
fn main() {
    let path = std::env::args().nth(1).expect("usage: compile <file.rs>");
    let src = std::fs::read_to_string(&path).expect("read source");
    let name = std::path::Path::new(&path)
        .file_name()
        .unwrap()
        .to_string_lossy()
        .to_string();
    let out = carciniscript::compile(&src, &name);
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
