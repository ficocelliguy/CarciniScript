// Inline `const { .. }` blocks: ordinary blocks here (their value is computed when they're
// reached rather than at compile time), in expressions, statements, array repeats
// (`[const { None }; N]`, the non-`Copy` repeat idiom) and `static` initializers.
const fn square(n: u32) -> u32 {
    n * n
}

fn table() -> [u8; 8] {
    const {
        let mut t = [0u8; 8];
        let mut i = 0;
        while i < 8 {
            t[i] = (i * i) as u8;
            i += 1;
        }
        t
    }
}

fn checked<const N: usize>(v: [i32; N]) -> i32 {
    const { assert!(N > 0, "N must be positive") };
    v[0]
}

static GREETING: &str = const { "hi" };

fn main() {
    let x = const { 1 + 2 };
    println!("{x}");
    let slots: [Option<String>; 3] = [const { None }; 3];
    println!("{:?}", slots);
    let mut lists: [Vec<u8>; 2] = [const { Vec::new() }; 2];
    lists[0].push(1);
    println!("{:?} {}", lists, lists[1].len());
    println!("{:?}", table());
    println!("{}", checked([7, 8]));
    let little = const { cfg!(target_endian = "little") };
    println!("{little}");
    let sq = const { square(9) };
    println!("{sq} {GREETING}");
    let pick = match x {
        3 => const { "three" },
        _ => "other",
    };
    println!("{pick}");
    let total: u32 = (1..=3).map(|i| i * const { 2 + 3 }).sum();
    println!("{total}");
    const {}
    let unit: () = const {};
    println!("{:?}", unit);
}
