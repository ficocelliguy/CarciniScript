// `DirEntry::path()` and `Child`'s `stdin` / `stdout` pipes (Rust by Example `std_misc/fs`,
// `std_misc/process/pipe`). A script has no filesystem and can't run processes, so both fail;
// the programs only need to compile and report the errors.
use std::fs;
use std::io::prelude::*;
use std::process::{Command, Stdio};

static PANGRAM: &'static str = "the quick brown fox jumps over the lazy dog\n";

fn list(dir: &str) {
    match fs::read_dir(dir) {
        Err(why) => println!("! {:?}", why.kind()),
        Ok(paths) => {
            for path in paths {
                println!("> {:?}", path.unwrap().path());
            }
        }
    }
}

fn pipe() -> std::io::Result<String> {
    let process = Command::new("definitely-not-wc")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()?;
    process.stdin.unwrap().write_all(PANGRAM.as_bytes())?;
    let mut s = String::new();
    process.stdout.unwrap().read_to_string(&mut s)?;
    Ok(s)
}

fn main() {
    list("./definitely/not/here");
    match pipe() {
        Err(_) => println!("couldn't run wc"),
        Ok(s) => print!("wc responded with:\n{}", s),
    }
    let child = Command::new("definitely-not-a-program").spawn();
    if let Ok(mut c) = child {
        if let Some(err) = c.stderr.as_mut() {
            let mut s = String::new();
            let _ = err.read_to_string(&mut s);
        }
    } else {
        println!("no child");
    }
}
