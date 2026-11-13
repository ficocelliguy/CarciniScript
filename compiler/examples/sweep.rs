//! Robustness sweep: compile every `.rs` file under a directory and report compiler panics.
//! A panic in the wasm build traps the whole module, so any hit here is a bug.
//!
//! `cargo run --release --example sweep -- <dir> [max_files]`
use std::path::{Path, PathBuf};

fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            collect(&p, out);
        } else if p.extension().is_some_and(|x| x == "rs") {
            out.push(p);
        }
    }
}

fn main() {
    let dir = std::env::args()
        .nth(1)
        .expect("usage: sweep <dir> [max_files]");
    let max: usize = std::env::args()
        .nth(2)
        .and_then(|s| s.parse().ok())
        .unwrap_or(usize::MAX);
    let mut files = Vec::new();
    collect(Path::new(&dir), &mut files);
    files.sort();
    files.truncate(max);
    static LAST: std::sync::Mutex<String> = std::sync::Mutex::new(String::new());
    std::panic::set_hook(Box::new(|info| {
        *LAST.lock().unwrap() = info.location().map(|l| l.to_string()).unwrap_or_default();
    }));
    let (mut ok, mut rejected, mut panics, mut bytes) = (0, 0, Vec::new(), 0usize);
    let mut slowest = Vec::new();
    let start = std::time::Instant::now();
    for f in &files {
        let Ok(src) = std::fs::read_to_string(f) else {
            continue;
        };
        bytes += src.len();
        let name = f.file_name().unwrap().to_string_lossy().to_string();
        let t0 = std::time::Instant::now();
        let result = std::panic::catch_unwind(|| carciniscript::compile(&src, &name));
        slowest.push((t0.elapsed(), f.clone(), src.len()));
        match result {
            Ok(out) if out.code.is_some() => ok += 1,
            Ok(_) => rejected += 1,
            Err(e) => {
                let msg = e
                    .downcast_ref::<String>()
                    .cloned()
                    .or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string()))
                    .unwrap_or_default();
                panics.push(format!(
                    "{}: {msg} at {}",
                    f.display(),
                    LAST.lock().unwrap()
                ));
            }
        }
    }
    println!(
        "{} files, {:.1} MB in {:.2?}: {ok} compiled, {rejected} rejected with diagnostics, {} panics",
        files.len(),
        bytes as f64 / 1e6,
        start.elapsed(),
        panics.len()
    );
    slowest.sort_by(|a, b| b.0.cmp(&a.0));
    for (t, f, len) in slowest.iter().take(5) {
        println!("slow {t:.2?} {} KB {}", len / 1024, f.display());
    }
    for p in panics.iter().take(40) {
        println!("PANIC {p}");
    }
}
