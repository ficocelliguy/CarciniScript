// `Path`'s methods that read the filesystem (`metadata`, `symlink_metadata`, `read_dir`,
// `read_link`, `canonicalize`), their `fs::` functions and `fs::FileType`: a script has no
// filesystem, so they fail with `NotFound`, as they do natively for a path that doesn't exist.
// The shape of Programming Rust's `copy` example. Also `Path::new(..)` used by value more than
// once (a `&Path`, which is `Copy`).
use std::fs;
use std::io;
use std::path::Path;

const MISSING: &str = "no/such/dir/carcini_missing";

fn copy_to(src: &Path, src_type: &fs::FileType, dst: &Path) -> io::Result<()> {
    if src_type.is_file() {
        fs::copy(src, dst)?;
    } else if src_type.is_dir() {
        for entry in src.read_dir()? {
            let entry = entry?;
            let t = entry.file_type()?;
            copy_to(&entry.path(), &t, &dst.join(entry.file_name()))?;
        }
    } else if src_type.is_symlink() {
        let target = src.read_link()?;
        println!("link to {}", target.display());
    }
    Ok(())
}

fn copy_into<P: AsRef<Path>, Q: AsRef<Path>>(source: P, destination: Q) -> io::Result<()> {
    let src = source.as_ref();
    let md = src.metadata()?;
    copy_to(src, &md.file_type(), destination.as_ref())
}

fn kind<T>(r: io::Result<T>) -> String {
    match r {
        Ok(_) => "ok".to_string(),
        Err(e) => format!("{:?}", e.kind()),
    }
}

fn main() {
    let p = Path::new(MISSING);
    println!("{}", kind(p.metadata()));
    println!("{}", kind(p.symlink_metadata()));
    println!("{}", kind(p.read_dir()));
    println!("{}", kind(p.read_link()));
    println!("{}", kind(p.canonicalize()));
    println!("{}", kind(p.join("x").metadata().map(|m| m.file_type().is_dir())));
    println!("{}", kind(fs::symlink_metadata(MISSING)));
    println!("{}", kind(fs::read_link(MISSING)));
    println!("{}", kind(fs::canonicalize(MISSING)));
    let dst = Path::new("no/such/out");
    for src in [MISSING, "no/such/other"] {
        println!("{}", kind(copy_into(src, dst)));
    }
    println!("{}", kind(copy_into(p, dst)));
    println!("{} {}", p.display(), dst.display());
}
