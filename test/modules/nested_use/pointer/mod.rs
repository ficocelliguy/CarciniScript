pub mod invariant;

use invariant::Aligned;

pub struct Ptr {
    inner: Aligned,
}

impl Ptr {
    pub fn new(a: Aligned) -> Ptr {
        Ptr { inner: a }
    }

    pub fn get(&self) -> u32 {
        self.inner.0 * 2
    }
}
