// `std::fs` (`File`, `OpenOptions`, `Metadata`, `fs::read_to_string` and friends) and
// `std::process::Command` compile; a script has no filesystem and can't run programs, so every
// operation fails: `NotFound` for reading, `Unsupported` otherwise (natively the path doesn't
// exist, so only reads' error kinds are printed). Also `io::IoSlice` with `write_vectored`.
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufRead, BufReader, IoSlice, Read, Write};
use std::process::{Command, Stdio};

const MISSING: &str = "no/such/dir/carcini_missing.txt";

fn load(path: &str) -> io::Result<String> {
    let mut f = File::open(path)?;
    let mut s = String::new();
    f.read_to_string(&mut s)?;
    Ok(s)
}

fn lines(path: &str) -> io::Result<usize> {
    let f = File::open(path)?;
    Ok(BufReader::new(f).lines().count())
}

fn main() {
    match load(MISSING) {
        Ok(_) => println!("loaded"),
        Err(e) => println!("load: {:?}", e.kind()),
    }
    println!("{:?}", lines(MISSING).map_err(|e| e.kind()));
    println!("{:?}", fs::read_to_string(MISSING).map_err(|e| e.kind()));
    println!("{:?}", fs::read(MISSING).is_err());
    println!("{:?}", fs::metadata(MISSING).map(|m| m.len()).map_err(|e| e.kind()));
    println!("{}", fs::read_dir("no/such/dir").is_err());
    println!("{}", fs::write(MISSING, "x").is_err());
    println!("{}", File::create(MISSING).is_err());
    println!("{}", fs::create_dir("no/such/dir/sub").is_err());
    println!("{}", fs::remove_file(MISSING).is_err());
    println!("{:?}", fs::exists(MISSING).ok());
    let r = OpenOptions::new().read(true).open(MISSING);
    println!("{:?}", r.map_err(|e| e.kind()).map(|_| ()));
    let w = OpenOptions::new().append(true).create(false).open(MISSING);
    println!("{}", w.is_err());
    if let Ok(mut f) = File::create(MISSING) {
        writeln!(f, "never").unwrap();
    }
    let mut cmd = Command::new("carcini-no-such-program");
    cmd.arg("-v").args(["a", "b"]).stdout(Stdio::piped());
    match cmd.output() {
        Ok(o) => println!("ran {}", o.status.success()),
        Err(e) => println!("command failed: {}", e.kind() == io::ErrorKind::NotFound || e.kind() == io::ErrorKind::Unsupported),
    }
    println!("{}", Command::new("carcini-no-such-program").status().is_err());
    println!("{}", Command::new("carcini-no-such-program").spawn().is_err());

    let mut v: Vec<u8> = Vec::new();
    let a = [1u8, 2];
    let bufs = [IoSlice::new(&a), IoSlice::new(b"xyz"), IoSlice::new(&[])];
    let n = v.write_vectored(&bufs).unwrap();
    println!("{n} {:?} {} {:?}", v, bufs[1].len(), bufs[1].to_vec());
}
