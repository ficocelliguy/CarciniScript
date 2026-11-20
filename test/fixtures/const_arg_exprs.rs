// Const arguments beyond literals, worked out at compile time: arithmetic on `const` items,
// associated consts, `u8::MAX` / `usize::BITS`, `size_of::<T>()`, `as` casts, `if` and calls of
// the file's `const fn`s (also inside macro arguments). Const parameters of 64- and 128-bit types
// (BigInts in the body), a `bool` from an expression, and associated consts naming items
// declared after the type.
const D_CODES: usize = 30;
const BL_CODES: usize = 19;
const MAX_BITS: usize = 200;

struct TreeDesc<const N: usize> {
    v: [u16; N],
}

impl<const N: usize> TreeDesc<N> {
    fn new() -> Self {
        TreeDesc { v: [0; N] }
    }
}

struct CpuFeatures;

impl CpuFeatures {
    const NONE: usize = 0;
    const AVX: usize = 1 << 3;
    // (Names a `const` declared further down.)
    const ALL: usize = Self::AVX | LATE;
}

const LATE: usize = 1 << 5;

fn feat<const F: usize>() -> &'static str {
    if F == CpuFeatures::NONE { "none" } else { "avx" }
}

const fn word_bits(bytes: usize) -> usize {
    bytes * 8
}

const fn elts(bits: usize, bytes: usize) -> usize {
    let width = word_bits(bytes);
    if width == 0 {
        return 0;
    }
    bits / width + (bits % width != 0) as usize
}

const fn clamp_len(n: usize) -> usize {
    if n > 6 { 6 } else if n < 2 { 2 } else { n }
}

fn make<const N: usize>() -> [u64; N] {
    [7; N]
}

fn scan(t: &mut TreeDesc<{ 2 * BL_CODES + 1 }>) -> usize {
    t.v[0] += 1;
    t.v.len()
}

mod m {
    pub const ID: usize = 5;
}

trait MaxLen {
    const MAX_STR_LEN: usize;
}

impl MaxLen for u64 {
    const MAX_STR_LEN: usize = u64::MAX.ilog10() as usize + 1;
}

impl MaxLen for u128 {
    const MAX_STR_LEN: usize = u128::MAX.ilog10() as usize + 1;
}

fn below<const N: u128>() -> u128 {
    N - 1
}

struct Big<const N: u64>;

impl<const N: u64> Big<N> {
    fn get(&self) -> u64 {
        N * 2
    }
}

fn wide<const N: i128>() -> i128 {
    N - 1
}

fn neg<const N: i64>() -> i64 {
    N.wrapping_mul(3)
}

struct Unit<const P: u128>;

impl<const P: u128> Unit<P> {
    fn per(&self) -> u128 {
        P * 1000
    }
}

struct A<const N: i32 = { i32::MAX }>;

impl<const N: i32> A<N> {
    fn n(&self) -> i32 {
        N
    }
}

fn flag<const B: bool>() -> &'static str {
    if B { "on" } else { "off" }
}

fn main() {
    let d: TreeDesc<{ 2 * D_CODES + 1 }> = TreeDesc::new();
    println!("{}", d.v.len());
    let mut b = TreeDesc::<{ 2 * BL_CODES + 1 }>::new();
    println!("{} {}", scan(&mut b), b.v[0]);
    println!("{} {}", feat::<{ CpuFeatures::NONE }>(), feat::<{ CpuFeatures::AVX }>());
    println!("{}", CpuFeatures::ALL);
    println!("{:?}", make::<{ elts(MAX_BITS, size_of::<u64>()) }>());
    println!("{:?}", make::<{ elts(17, 1) }>().len());
    println!("{}", make::<{ usize::BITS as usize / 16 }>().len());
    println!("{:?}", make::<{ size_of::<u32>() }>());
    println!("{:?}", make::<{ crate::m::ID }>());
    println!("{:?}", make::<{ m::ID - 3 }>());
    println!("{}", make::<{ clamp_len(9) }>().len());
    println!("{}", make::<{ u64::MAX_STR_LEN - 16 }>().len());
    println!("{}", make::<{ u128::MAX_STR_LEN - 32 }>().len());
    println!("{}", below::<{ u128::MAX }>());
    println!("{}", make::<{ u8::MAX as usize % 7 }>().len());
    println!("{}", Big::<5>.get());
    println!("{}", Big::<{ (1u64 << 50) - 1 }>.get());
    println!("{}", wide::<{ -170141183460469231731687303715884105727 }>());
    println!("{}", wide::<12>());
    println!("{}", neg::<{ -1_000_000_000_000 }>());
    println!("{}", Unit::<1_000_000_000_000_000_000_000>.per());
    let a: A = A;
    println!("{} {}", a.n(), A::<3>.n());
    println!("{} {}", flag::<{ D_CODES > 20 }>(), flag::<{ MAX_BITS % 2 == 1 }>());
}
