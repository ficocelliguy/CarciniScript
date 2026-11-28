// `#[cfg(..)]` on an element of an array, a tuple, a call's arguments or a std macro's arguments
// is decided with every feature off, as rustc builds a lone file.
fn add3(a: u32, b: u32, c: u32) -> u32 {
    a * 100 + b * 10 + c
}

fn main() {
    let v = vec![
        1u8,
        #[cfg(feature = "extra")]
        2,
        #[cfg(not(feature = "extra"))]
        3,
        #[cfg(any(windows, not(windows)))]
        4,
        #[cfg(all(unix, windows))]
        5,
    ];
    println!("{:?}", v);
    let a = [10, #[cfg(feature = "x")] 20, 30, #[cfg(not(feature = "x"))] 40];
    println!("{:?} {}", a, a.len());
    let t = (1, #[cfg(feature = "x")] "no", "yes");
    println!("{:?}", t);
    println!("{}", add3(1, #[cfg(feature = "x")] 9, 2, 3));
    assert_eq!(v.len(), 3);
    assert_eq!(
        vec![#[cfg(feature = "x")] 0u8, 1],
        [1]
    );
}
