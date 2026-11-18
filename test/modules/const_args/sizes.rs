// Constants another file's const arguments compute with.
pub const WORDS: usize = 4;
pub const BIG: u64 = 1 << 40;

pub struct Limits;

impl Limits {
    pub const ROWS: usize = WORDS * 2;
    pub const SIGNED: bool = true;
}

pub struct Buf<const N: usize> {
    pub data: [u8; N],
}

impl<const N: usize> Buf<N> {
    pub fn new() -> Self {
        Buf { data: [1; N] }
    }

    pub fn total(&self) -> usize {
        self.data.iter().map(|&b| b as usize).sum()
    }
}
