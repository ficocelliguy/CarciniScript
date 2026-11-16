// Target `cfg`s, answered for the one target scripts compile for (JS, described as
// `wasm32-unknown-unknown` with 64-bit `usize`): `cfg!(..)`, and `#[cfg(..)]` on alternative
// definitions of one name, where only the active one is kept.
#[cfg(windows)]
fn os() -> &'static str {
    "windows"
}

#[cfg(unix)]
fn os() -> &'static str {
    "unix"
}

#[cfg(not(any(windows, unix)))]
fn os() -> &'static str {
    "js"
}

#[cfg(target_pointer_width = "64")]
const WIDTH: u32 = 64;
#[cfg(target_pointer_width = "32")]
const WIDTH: u32 = 32;

#[cfg(target_feature = "avx2")]
fn sum(v: &[i32]) -> i32 {
    v.iter().sum::<i32>() + 1000
}

#[cfg(not(target_feature = "avx2"))]
fn sum(v: &[i32]) -> i32 {
    v.iter().sum()
}

fn main() {
    println!("{} {} {}", os(), WIDTH, sum(&[1, 2, 3]));
    println!(
        "windows={} unix={} wasm={} wasm32={} os_unknown={} linux={}",
        cfg!(windows),
        cfg!(unix),
        cfg!(target_family = "wasm"),
        cfg!(target_arch = "wasm32"),
        cfg!(target_os = "unknown"),
        cfg!(target_os = "linux"),
    );
    println!(
        "little={} big={} ptr64={} ptr32={} miri={} feature={} vendor={} env={}",
        cfg!(target_endian = "little"),
        cfg!(target_endian = "big"),
        cfg!(target_pointer_width = "64"),
        cfg!(target_pointer_width = "32"),
        cfg!(miri),
        cfg!(target_feature = "sse2"),
        cfg!(target_vendor = "unknown"),
        cfg!(target_env = ""),
    );
    println!("{}", cfg!(all(target_family = "wasm", not(any(unix, windows)), not(test))));
}
