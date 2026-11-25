// `std::os::unix::ffi::OsStrExt` / `OsStringExt` (unix only, so no native oracle on every host):
// an `OsStr` is a `String` here, so bytes that aren't UTF-8 become U+FFFD.
use std::ffi::{OsStr, OsString};
use std::os::unix::ffi::{OsStrExt, OsStringExt};

fn name(bytes: &[u8]) -> OsString {
    OsStr::from_bytes(bytes).to_os_string()
}

fn main() {
    let s = OsStr::from_bytes(b"hello.txt");
    println!("{:?} {}", s, s.len());
    println!("{:?}", s.as_bytes());
    println!("{:?}", name(b"a\xffb").to_string_lossy());
    let o = OsString::from_vec(vec![b'x', b'y']);
    println!("{:?}", o);
    println!("{:?}", o.into_vec());
}
